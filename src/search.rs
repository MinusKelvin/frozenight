use std::sync::Arc;

use cozy_chess::{Board, Move};
use nohash::IntSet;
use rand::prelude::*;

use crate::{Eval, SharedState, Statistics};

pub(crate) struct Searcher {
    pub stats: Statistics,
    shared: Arc<SharedState>,
    history: IntSet<u64>,
    target_depth: u16,
    valid: bool,
}

impl Searcher {
    pub fn new(shared: Arc<SharedState>, history: IntSet<u64>) -> Self {
        Searcher {
            shared,
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
    pub fn search(&mut self, root: &Board, depth: u16) -> Option<(Eval, Move)> {
        if !self.valid {
            panic!("attempt to search using an invalid searcher");
        }
        if !root.generate_moves(|_| true) {
            panic!("root position has no legal moves");
        }
        self.target_depth = depth;
        let result = self.alpha_beta(root, -Eval::MATE, Eval::MATE, 0);
        self.valid = result.is_some();
        result
    }

    fn visit_node(
        &mut self,
        board: &Board,
        alpha: Eval,
        beta: Eval,
        current_depth: u16,
    ) -> Option<Eval> {
        match board.status() {
            cozy_chess::GameStatus::Drawn => return Some(Eval::DRAW),
            cozy_chess::GameStatus::Won => return Some(-Eval::MATE.add_time(current_depth)),
            cozy_chess::GameStatus::Ongoing => {}
        }

        if !self.history.insert(board.hash()) {
            return Some(Eval::DRAW);
        }

        let result = if current_depth > self.target_depth {
            self.stats.selective_depth = self.stats.selective_depth.max(current_depth);
            self.stats.nodes += 1;
            self.shared
                .running
                .load(std::sync::atomic::Ordering::Relaxed)
                .then(|| static_eval(board))
        } else {
            self.alpha_beta(board, alpha, beta, current_depth)
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
        current_depth: u16,
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
            let v = -self.visit_node(&new_board, -beta, -alpha, current_depth + 1)?;
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

fn static_eval(_board: &Board) -> Eval {
    Eval::new(thread_rng().gen_range(
        bytemuck::cast(-Eval::MAX_INCONCLUSIVE)..=bytemuck::cast(Eval::MAX_INCONCLUSIVE),
    ))
}
