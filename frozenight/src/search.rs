use std::sync::atomic::{AtomicBool, Ordering};

use cozy_chess::{Board, Move, Square};
use nohash::IntSet;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

pub use self::abdada::AbdadaTable;
use self::ordering::{OrderingState, BREAK, CONTINUE};
use self::window::Window;

mod abdada;
mod null;
mod ordering;
mod pv;
mod qsearch;
mod window;

pub const INVALID_MOVE: Move = Move {
    from: Square::A1,
    to: Square::A1,
    promotion: None,
};

pub(crate) struct SearchState {
    history: OrderingState,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState {
            history: OrderingState::new(),
        }
    }
}

pub(crate) struct Searcher<'a> {
    pub root: Board,
    pub stats: &'a Statistics,
    pub shared: &'a SharedState,
    pub node_limit: u64,
    pub abort: &'a AtomicBool,
    valid: bool,
    multithreaded: bool,
    repetition: IntSet<u64>,
    state: &'a mut SearchState,
}

impl<'a> Searcher<'a> {
    pub fn new(
        abort: &'a AtomicBool,
        shared: &'a SharedState,
        state: &'a mut SearchState,
        stats: &'a Statistics,
        repetition: IntSet<u64>,
        root: Board,
        multithreaded: bool,
    ) -> Self {
        state.history.decay();
        Searcher {
            root,
            shared,
            abort,
            repetition,
            state,
            stats,
            multithreaded,
            node_limit: u64::MAX,
            valid: true,
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, depth: i16) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an aborted searcher");
        }

        if !self.root.generate_moves(|_| true) {
            panic!("root position (FEN: {}) has no moves", self.root);
        }

        self.pv_search(
            &Position::from_root(self.root.clone(), &self.shared.nnue),
            Window::default(),
            depth,
        )
    }

    fn visit_node(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
        f: impl FnOnce(&mut Self) -> Option<Eval>,
    ) -> Option<Eval> {
        match position.board.status() {
            cozy_chess::GameStatus::Drawn => return Some(Eval::DRAW),
            cozy_chess::GameStatus::Won => return Some(-Eval::MATE.add_time(position.ply)),
            cozy_chess::GameStatus::Ongoing => {}
        }

        if depth > 0 && self.abort.load(Ordering::Relaxed) {
            return None;
        }

        let result = if depth <= 0 {
            self.qsearch(position, window)
        } else {
            if self.stats.nodes.fetch_add(1, Ordering::Relaxed) >= self.node_limit {
                return None;
            }
            f(self)?
        };

        // Sanity check that conclusive scores are valid
        #[cfg(debug_assertions)]
        if let Some(plys) = result.plys_to_conclusion() {
            debug_assert!(plys.abs() >= position.ply as i16);
        }

        Some(result)
    }

    fn search_moves(
        &mut self,
        position: &Position,
        hashmove: Option<Move>,
        mut window: Window,
        depth: i16,
        mut f: impl FnMut(&mut Searcher, usize, Move, &Position, Window) -> Option<Eval>,
    ) -> Option<(Eval, Move)> {
        let mut best_move = INVALID_MOVE;
        let mut best_score = -Eval::MATE;
        let mut raised_alpha = false;
        let mut i = 0;

        let mut remaining = vec![];

        self.visit_moves(position, hashmove, |this, mv| {
            let new_pos = position.play_move(&this.shared.nnue, mv);

            let v;
            if this.repetition.insert(new_pos.board.hash()) {
                if this.multithreaded
                    && i > 0
                    && this.shared.abdada.is_searching(new_pos.board.hash())
                {
                    this.repetition.remove(&new_pos.board.hash());
                    remaining.push((i, mv, new_pos));
                    i += 1;
                    return Some(CONTINUE);
                }

                this.shared.tt.prefetch(&new_pos.board);
                let _guard = match this.multithreaded {
                    true => this.shared.abdada.enter(new_pos.board.hash()),
                    false => None,
                };
                v = f(this, i, mv, &new_pos, window)?;
                this.repetition.remove(&new_pos.board.hash());
            } else {
                // repetition
                v = Eval::DRAW;
            };

            if v > best_score {
                best_move = mv;
                best_score = v;
            }

            if window.fail_high(v) {
                return Some(BREAK);
            }

            if window.raise_lb(v) {
                raised_alpha = true;
            }

            i += 1;
            Some(CONTINUE)
        })?;

        for (i, mv, new_pos) in remaining {
            self.shared.tt.prefetch(&new_pos.board);
            self.repetition.insert(new_pos.board.hash());
            let _guard = self.shared.abdada.enter(new_pos.board.hash());
            let v = f(self, i, mv, &new_pos, window)?;
            self.repetition.remove(&new_pos.board.hash());

            if v > best_score {
                best_move = mv;
                best_score = v;
            }

            if window.fail_high(v) {
                break;
            }

            if window.raise_lb(v) {
                raised_alpha = true;
            }
        }

        if window.fail_high(best_score) {
            self.failed_high(position, depth, best_score, best_move);
        } else if raised_alpha {
            self.shared.tt.store(
                &position,
                TableEntry {
                    mv: best_move,
                    eval: best_score,
                    depth,
                    kind: NodeKind::Exact,
                },
            );
        } else {
            self.failed_low(position, depth, best_score, best_move);
        }

        Some((best_score, best_move))
    }

    fn failed_low(&mut self, position: &Position, depth: i16, eval: Eval, mv: Move) {
        self.shared.tt.store(
            &position,
            TableEntry {
                mv,
                eval,
                depth,
                kind: NodeKind::UpperBound,
            },
        );
    }

    fn failed_high(&mut self, position: &Position, depth: i16, eval: Eval, mv: Move) {
        self.shared.tt.store(
            &position,
            TableEntry {
                mv,
                eval,
                depth,
                kind: NodeKind::LowerBound,
            },
        );
        self.state.history.caused_cutoff(position, mv);
    }

    fn psuedo_random(&self) -> u64 {
        let mut x = self.stats.nodes.load(Ordering::Relaxed);
        // https://stackoverflow.com/questions/664014/what-integer-hash-function-are-good-that-accepts-an-integer-hash-key
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
}
