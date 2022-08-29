use std::sync::atomic::{AtomicBool, Ordering};

use cozy_chess::{Board, Move, Square};

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

pub use self::abdada::AbdadaTable;
use self::ordering::{OrderingState, BREAK, CONTINUE};
use self::window::Window;

mod abdada;
mod null;
mod oracle;
mod ordering;
mod pv;
mod qsearch;
mod see;
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
    state: &'a mut SearchState,
    rep_list: Vec<u64>,
    rep_table: [u8; 1024],
    reverse_pv: Vec<Move>,
}

pub struct SearchResult {
    pub eval: Eval,
    pub reverse_pv: Vec<Move>,
}

impl SearchResult {
    fn eval(eval: Eval) -> Self {
        SearchResult {
            eval,
            reverse_pv: vec![],
        }
    }

    fn mv(eval: Eval, mv: Move) -> Self {
        SearchResult {
            eval,
            reverse_pv: vec![mv],
        }
    }

    pub fn best_move(&self) -> Move {
        *self.reverse_pv.last().unwrap()
    }
}

impl std::ops::Neg for SearchResult {
    type Output = Self;

    fn neg(self) -> Self::Output {
        SearchResult {
            eval: -self.eval,
            ..self
        }
    }
}

impl<'a> Searcher<'a> {
    pub fn new(
        abort: &'a AtomicBool,
        shared: &'a SharedState,
        state: &'a mut SearchState,
        stats: &'a Statistics,
        root: Board,
        multithreaded: bool,
        rep_list: Vec<u64>,
    ) -> Self {
        state.history.decay();
        let mut rep_table = [0; 1024];
        for &b in &rep_list {
            rep_table[b as usize % 1024] += 1;
        }
        Searcher {
            root,
            shared,
            abort,
            state,
            stats,
            multithreaded,
            rep_table,
            node_limit: u64::MAX,
            valid: true,
            rep_list,
            reverse_pv: vec![],
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, depth: i16, around: Option<Eval>) -> Option<SearchResult> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an aborted searcher");
        }

        if !self.root.generate_moves(|_| true) {
            panic!("root position (FEN: {}) has no moves", self.root);
        }

        let window = match around {
            Some(around) if depth >= 3 && !around.is_conclusive() => {
                Window::new(around - 500, around + 500)
            }
            _ => Window::default(),
        };

        let position = &Position::from_root(self.root.clone(), &self.shared.nnue);
        let reverse_pv = std::mem::take(&mut self.reverse_pv);

        let result = self.pv_search(position, window, depth, &reverse_pv)?;

        let result = if window.fail_low(result.eval) || window.fail_high(result.eval) {
            self.pv_search(position, Window::default(), depth, &result.reverse_pv)?
        } else {
            result
        };
        self.reverse_pv = result.reverse_pv.clone();

        Some(result)
    }

    fn visit_node(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
        f: impl FnOnce(&mut Self) -> Option<SearchResult>,
    ) -> Option<SearchResult> {
        match position.board.status() {
            cozy_chess::GameStatus::Drawn => return Some(SearchResult::eval(Eval::DRAW)),
            cozy_chess::GameStatus::Won => {
                return Some(SearchResult::eval(-Eval::MATE.add_time(position.ply)))
            }
            cozy_chess::GameStatus::Ongoing => {}
        }

        if depth > 0 && self.abort.load(Ordering::Relaxed) {
            return None;
        }

        let result = if depth <= 0 {
            SearchResult::eval(self.qsearch(position, window))
        } else {
            if self.stats.nodes.fetch_add(1, Ordering::Relaxed) >= self.node_limit {
                return None;
            }
            f(self)?
        };

        // Sanity check that conclusive scores are valid
        #[cfg(debug_assertions)]
        if let Some(plys) = result.eval.plys_to_conclusion() {
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
        mut f: impl FnMut(&mut Searcher, usize, Move, &Position, Window) -> Option<SearchResult>,
    ) -> Option<SearchResult> {
        let mut best = SearchResult::eval(-Eval::MATE);
        let mut raised_alpha = false;
        let mut i = 0;

        let mut remaining = vec![];

        self.visit_moves(position, hashmove, |this, mv| {
            let new_pos = position.play_move(&this.shared.nnue, mv);

            let result;
            if let Some(eval) = oracle::oracle(&new_pos.board) {
                result = SearchResult::eval(eval);
            } else if this.is_repetition(&new_pos.board) {
                result = SearchResult::eval(Eval::DRAW);
            } else {
                if this.multithreaded
                    && i > 0
                    && this.shared.abdada.is_searching(new_pos.board.hash())
                {
                    remaining.push((i, mv, new_pos));
                    i += 1;
                    return Some(CONTINUE);
                }

                this.shared.tt.prefetch(&new_pos.board);
                let _guard = match this.multithreaded {
                    true => this.shared.abdada.enter(new_pos.board.hash()),
                    false => None,
                };
                this.push_repetition(&new_pos.board);
                result = f(this, i, mv, &new_pos, window)?;
                this.pop_repetition();
            }

            let eval = result.eval;
            if eval > best.eval {
                best = result;
                best.reverse_pv.push(mv);
            }

            if window.fail_high(eval) {
                return Some(BREAK);
            }

            if window.raise_lb(eval) {
                raised_alpha = true;
            }

            i += 1;
            Some(CONTINUE)
        })?;

        for (i, mv, new_pos) in remaining {
            self.shared.tt.prefetch(&new_pos.board);
            self.push_repetition(&new_pos.board);
            let _guard = self.shared.abdada.enter(new_pos.board.hash());
            let result = f(self, i, mv, &new_pos, window)?;
            self.pop_repetition();

            let eval = result.eval;
            if eval > best.eval {
                best = result;
                best.reverse_pv.push(mv);
            }

            if window.fail_high(eval) {
                break;
            }

            if window.raise_lb(eval) {
                raised_alpha = true;
            }
        }

        if window.fail_high(best.eval) {
            self.failed_high(position, depth, best.eval, *best.reverse_pv.last().unwrap());
        } else if raised_alpha {
            self.shared.tt.store(
                &position,
                TableEntry {
                    mv: *best.reverse_pv.last().unwrap(),
                    eval: best.eval,
                    depth,
                    kind: NodeKind::Exact,
                },
            );
        } else {
            self.failed_low(position, depth, best.eval, *best.reverse_pv.last().unwrap());
        }

        Some(best)
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

    fn push_repetition(&mut self, board: &Board) {
        self.rep_table[board.hash() as usize % 1024] += 1;
        self.rep_list.push(board.hash());
    }

    fn pop_repetition(&mut self) {
        let hash = self.rep_list.pop().unwrap();
        self.rep_table[hash as usize % 1024] -= 1;
    }

    fn is_repetition(&self, board: &Board) -> bool {
        if self.rep_table[board.hash() as usize % 1024] == 0 {
            return false;
        }

        self.rep_list
            .iter()
            .rev()
            .take(board.halfmove_clock() as usize)
            .skip(1)
            .any(|&b| b == board.hash())
    }
}
