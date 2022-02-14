use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cozy_chess::{Board, Move};
use nohash::IntSet;

use crate::nnue::NnueAccumulator;
use crate::{Eval, SharedState, Statistics};

pub(crate) struct Searcher {
    pub stats: Statistics,
    shared: Arc<SharedState>,
    abort: Arc<AtomicBool>,
    history: IntSet<u64>,
    target_depth: i16,
    valid: bool,
    nnue: NnueAccumulator,
}

impl Searcher {
    pub fn new(abort: Arc<AtomicBool>, shared: Arc<SharedState>, history: IntSet<u64>) -> Self {
        Searcher {
            nnue: NnueAccumulator::new(&shared.nnue),
            shared,
            abort,
            history,
            target_depth: 0,
            valid: true,
            stats: Default::default(),
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, root: &Board, depth: i16) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an invalid searcher");
        }
        if !root.generate_moves(|_| true) {
            panic!("root position has no legal moves");
        }
        self.target_depth = depth;
        let result = self.alpha_beta(root, -Eval::MATE, Eval::MATE, depth);
        self.valid = result.is_some();
        result
    }

    fn visit_node(
        &mut self,
        board: &Board,
        alpha: Eval,
        beta: Eval,
        depth_remain: i16,
    ) -> Option<Eval> {
        match board.status() {
            cozy_chess::GameStatus::Drawn => return Some(Eval::DRAW),
            cozy_chess::GameStatus::Won => return Some(-Eval::MATE),
            cozy_chess::GameStatus::Ongoing => {}
        }

        if depth_remain == 1 && self.abort.load(Ordering::Relaxed) {
            return None;
        }

        if !self.history.insert(board.hash()) {
            return Some(Eval::DRAW);
        }

        let result = if depth_remain == 0 {
            self.stats.nodes += 1;
            Some(self.nnue.calculate(&self.shared.nnue, board))
        } else {
            self.alpha_beta(board, alpha, beta, depth_remain)
                .map(|(e, _)| e)
        };

        self.history.remove(&board.hash());
        result
    }

    /// Invariant: `self` is unchanged if this function returns `Some`.
    fn alpha_beta(
        &mut self,
        board: &Board,
        mut alpha: Eval,
        beta: Eval,
        depth_remain: i16,
    ) -> Option<(Eval, Move)> {
        let mut moves = Vec::with_capacity(64);
        board.generate_moves(|mvset| {
            moves.extend(mvset);
            false
        });

        let mut best_move = (-Eval::MATE, moves[0]);
        for mv in moves {
            let mut new_board = board.clone();
            new_board.play_unchecked(mv);
            let v = -self.visit_node(&new_board, -beta, -alpha, depth_remain - 1)?;
            let v = v.add_time(1);
            if v >= beta {
                return Some((v, mv));
            }
            if v > alpha {
                alpha = v;
            }
            if v > best_move.0 {
                best_move = (v, mv);
            }
        }

        Some(best_move)
    }
}
