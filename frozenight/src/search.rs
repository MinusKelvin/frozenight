use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cozy_chess::{Board, Move, Square};
use nohash::IntSet;

use crate::nnue::NnueAccumulator;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

use self::ordering::MoveOrdering;

mod ordering;
mod qsearch;

const INVALID_MOVE: Move = Move {
    from: Square::A1,
    to: Square::A1,
    promotion: None,
};

pub(crate) struct Searcher {
    pub stats: Statistics,
    shared: Arc<SharedState>,
    abort: Arc<AtomicBool>,
    history: IntSet<u64>,
    valid: bool,
    nnue: NnueAccumulator,
    killers: Vec<Move>,
}

impl Searcher {
    pub fn new(abort: Arc<AtomicBool>, shared: Arc<SharedState>, history: IntSet<u64>) -> Self {
        Searcher {
            nnue: NnueAccumulator::new(&shared.nnue),
            shared,
            abort,
            history,
            valid: true,
            stats: Default::default(),
            killers: vec![
                Move {
                    from: Square::A1,
                    to: Square::A1,
                    promotion: None,
                };
                128
            ],
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, root: &Board, depth: u16) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an invalid searcher");
        }
        let mut alpha = -Eval::MATE;
        let mut best_move = INVALID_MOVE;

        let hashmove = self
            .shared
            .tt
            .get(root)
            .and_then(|entry| root.is_legal(entry.mv).then(|| entry.mv));

        for mv in MoveOrdering::new(root, hashmove, INVALID_MOVE) {
            let mut new_board = root.clone();
            new_board.play_unchecked(mv);
            let v = -self.visit_node(&new_board, -Eval::MATE, -alpha, 1, depth - 1)?;
            if v > alpha {
                alpha = v;
                best_move = mv;
            }
        }

        if best_move == INVALID_MOVE {
            panic!("root position has no moves");
        }

        Some((alpha, best_move))
    }

    fn killer(&mut self, ply_index: u16) -> &mut Move {
        let idx = ply_index as usize;
        if idx >= self.killers.len() {
            self.killers
                .extend((self.killers.len()..=idx).map(|_| Move {
                    from: Square::A1,
                    to: Square::A1,
                    promotion: None,
                }));
        }
        &mut self.killers[idx]
    }

    fn visit_node(
        &mut self,
        board: &Board,
        alpha: Eval,
        beta: Eval,
        ply_index: u16,
        depth: u16,
    ) -> Option<Eval> {
        match board.status() {
            cozy_chess::GameStatus::Drawn => return Some(Eval::DRAW),
            cozy_chess::GameStatus::Won => return Some(-Eval::MATE.add_time(ply_index)),
            cozy_chess::GameStatus::Ongoing => {}
        }

        if depth > 0 && self.abort.load(Ordering::Relaxed) {
            return None;
        }

        if !self.history.insert(board.hash()) {
            return Some(Eval::DRAW);
        }

        let result = if depth == 0 {
            Some(self.qsearch(board, alpha, beta, ply_index))
        } else {
            self.alpha_beta(board, alpha, beta, ply_index, depth)
        };

        self.history.remove(&board.hash());
        result
    }

    /// Invariant: `self` is unchanged if this function returns `Some`.
    /// The board must have legal moves.
    fn alpha_beta(
        &mut self,
        board: &Board,
        mut alpha: Eval,
        beta: Eval,
        ply_index: u16,
        depth: u16,
    ) -> Option<Eval> {
        self.stats.nodes += 1;
        // It is impossible to accidentally return this score because the worst move that could
        // possibly be returned by visit_node is -Eval::MATE.add(1) which is better than this
        let mut best_score = -Eval::MATE;
        let mut best_move = INVALID_MOVE;

        let hashmove = self
            .shared
            .tt
            .get(board)
            .and_then(|entry| board.is_legal(entry.mv).then(|| entry.mv));

        for mv in MoveOrdering::new(board, hashmove, *self.killer(ply_index)) {
            let mut new_board = board.clone();
            new_board.play_unchecked(mv);
            let v = -self.visit_node(&new_board, -beta, -alpha, ply_index + 1, depth - 1)?;
            if v >= beta {
                self.shared.tt.store(
                    board,
                    TableEntry {
                        mv,
                        eval: v.sub_time(ply_index),
                        search_depth: depth,
                        kind: NodeKind::Cut,
                    },
                );
                // caused a beta cutoff, update the killer at this ply
                if board.color_on(mv.to) != Some(!board.side_to_move()) {
                    *self.killer(ply_index) = mv;
                }
                return Some(v);
            }
            if v > alpha {
                alpha = v;
            }
            if v > best_score {
                best_score = v;
                best_move = mv;
            }
        }

        self.shared.tt.store(
            board,
            TableEntry {
                mv: best_move,
                eval: best_score.sub_time(ply_index),
                search_depth: depth,
                kind: match best_score == alpha {
                    true => NodeKind::Pv,
                    false => NodeKind::All,
                },
            },
        );

        Some(best_score)
    }
}
