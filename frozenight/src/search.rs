use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cozy_chess::{Board, Move, Square};
use nohash::IntSet;

use crate::nnue::NnueAccumulator;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

pub(crate) struct Searcher {
    pub stats: Statistics,
    shared: Arc<SharedState>,
    abort: Arc<AtomicBool>,
    history: IntSet<u64>,
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
            valid: true,
            stats: Default::default(),
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
        if !root.generate_moves(|_| true) {
            panic!("root position has no legal moves");
        }
        let result = self.alpha_beta(root, -Eval::MATE, Eval::MATE, 0, depth);
        self.valid = result.is_some();
        result
    }

    fn visit_node(
        &mut self,
        board: &Board,
        alpha: Eval,
        beta: Eval,
        current_depth: u16,
        depth_remain: u16,
    ) -> Option<Eval> {
        self.stats.nodes += 1;
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
            self.stats.selective_depth = self.stats.selective_depth.max(current_depth);
            Some(self.nnue.calculate(&self.shared.nnue, board))
        } else {
            self.alpha_beta(board, alpha, beta, current_depth, depth_remain)
                .map(|(e, _)| e)
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
        current_depth: u16,
        depth_remain: u16,
    ) -> Option<(Eval, Move)> {
        // It is impossible to accidentally return this move because
        let mut best_move = (
            -Eval::MATE,
            Move {
                from: Square::A1,
                to: Square::A1,
                promotion: None,
            },
        );

        // TODO: Factor move ordering code out so we don't duplicate stuff
        // try hash move first
        let mut skip = None;
        if let Some(entry) = self.shared.tt.get(board) {
            if entry.search_depth >= depth_remain {
                // already have better data for this node; provide TT move and eval
                return Some((entry.eval, entry.mv));
            }

            let mut new_board = board.clone();
            if new_board.try_play(entry.mv).unwrap() {
                skip = Some(entry.mv);
                let v = -self.visit_node(
                    &new_board,
                    -beta,
                    -alpha,
                    current_depth + 1,
                    depth_remain - 1,
                )?;
                let v = v.add_time(1);
                if v >= beta {
                    self.shared.tt.store(
                        board,
                        TableEntry {
                            mv: entry.mv,
                            eval: v,
                            search_depth: depth_remain,
                            kind: NodeKind::Cut,
                        },
                    );
                    return Some((v, entry.mv));
                }
                if v > alpha {
                    alpha = v;
                }
                if v > best_move.0 {
                    best_move = (v, entry.mv);
                }
            }
        }

        let mut moves = Vec::with_capacity(64);
        board.generate_moves(|mvset| {
            moves.extend(mvset.into_iter().filter(|&mv| Some(mv) != skip));
            false
        });

        for mv in moves {
            let mut new_board = board.clone();
            new_board.play_unchecked(mv);
            let v = -self.visit_node(
                &new_board,
                -beta,
                -alpha,
                current_depth + 1,
                depth_remain - 1,
            )?;
            let v = v.add_time(1);
            if v >= beta {
                self.shared.tt.store(
                    board,
                    TableEntry {
                        mv,
                        eval: v,
                        search_depth: depth_remain,
                        kind: NodeKind::Cut,
                    },
                );
                return Some((v, mv));
            }
            if v > alpha {
                alpha = v;
            }
            if v > best_move.0 {
                best_move = (v, mv);
            }
        }

        self.shared.tt.store(
            board,
            TableEntry {
                mv: best_move.1,
                eval: best_move.0,
                search_depth: depth_remain,
                kind: match best_move.0 == alpha {
                    true => NodeKind::Pv,
                    false => NodeKind::All,
                },
            },
        );

        Some(best_move)
    }
}
