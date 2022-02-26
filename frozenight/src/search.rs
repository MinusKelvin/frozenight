use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cozy_chess::{Board, Move, Square};
use nohash::IntSet;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

use self::ordering::{HistoryTable, MoveOrdering};

mod ordering;
mod qsearch;

const INVALID_MOVE: Move = Move {
    from: Square::A1,
    to: Square::A1,
    promotion: None,
};

pub(crate) struct Searcher {
    pub stats: Statistics,
    pub shared: Arc<SharedState>,
    abort: Arc<AtomicBool>,
    repetition: IntSet<u64>,
    valid: bool,
    killers: Vec<Move>,
    history: HistoryTable,
}

impl Searcher {
    pub fn new(abort: Arc<AtomicBool>, shared: Arc<SharedState>, repetition: IntSet<u64>) -> Self {
        Searcher {
            shared,
            abort,
            repetition,
            valid: true,
            stats: Default::default(),
            killers: vec![INVALID_MOVE; 128],
            history: HistoryTable::new(),
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, root: &Board, depth: u16) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an aborted searcher");
        }
        let mut alpha = -Eval::MATE;
        let mut best_move = INVALID_MOVE;

        let position = Position::from_root(root.clone(), &self.shared.nnue);

        let hashmove = self
            .shared
            .tt
            .get(&position)
            .and_then(|entry| root.is_legal(entry.mv).then(|| entry.mv));

        let mut orderer = MoveOrdering::new(root, hashmove, INVALID_MOVE);
        while let Some(mv) = orderer.next(&self.history) {
            let mut new_board = root.clone();
            new_board.play_unchecked(mv);
            let v = -self.visit_node(
                &position.play_move(&self.shared.nnue, mv),
                -Eval::MATE,
                -alpha,
                depth - 1,
            )?;
            if v > alpha {
                alpha = v;
                best_move = mv;
            }
        }

        if best_move == INVALID_MOVE {
            panic!("root position (FEN: {}) has no moves", root);
        }

        Some((alpha, best_move))
    }

    fn killer(&mut self, ply_index: u16) -> &mut Move {
        let idx = ply_index as usize;
        if idx >= self.killers.len() {
            self.killers
                .extend((self.killers.len()..=idx).map(|_| INVALID_MOVE));
        }
        &mut self.killers[idx]
    }

    fn visit_node(
        &mut self,
        position: &Position,
        alpha: Eval,
        beta: Eval,
        depth: u16,
    ) -> Option<Eval> {
        match position.board.status() {
            cozy_chess::GameStatus::Drawn => return Some(Eval::DRAW),
            cozy_chess::GameStatus::Won => return Some(-Eval::MATE.add_time(position.ply)),
            cozy_chess::GameStatus::Ongoing => {}
        }

        if depth > 0 && self.abort.load(Ordering::Relaxed) {
            return None;
        }

        if !self.repetition.insert(position.board.hash()) {
            return Some(Eval::DRAW);
        }

        let result = if depth == 0 {
            self.qsearch(position, alpha, beta)
        } else {
            self.alpha_beta(position, alpha, beta, depth)?
        };

        // Sanity check that conclusive scores are valid
        #[cfg(debug_assertions)]
        if let Some(plys) = result.plys_to_conclusion() {
            debug_assert!(plys.abs() >= position.ply as i16);
        }

        self.repetition.remove(&position.board.hash());
        Some(result)
    }

    /// Invariant: `self` is unchanged if this function returns `Some`.
    /// If the side to move has no moves, this returns `-Eval::MATE` even if it is stalemate.
    fn alpha_beta(
        &mut self,
        position: &Position,
        mut alpha: Eval,
        mut beta: Eval,
        depth: u16,
    ) -> Option<Eval> {
        self.stats.nodes += 1;

        // reverse futility pruning... but with qsearch
        if depth <= 6 {
            let margin = 250 * depth as i16;
            let eval = self.qsearch(position, beta + margin - 1, beta + margin);
            if eval - margin >= beta {
                return Some(eval);
            }
        }

        if position.board.checkers().is_empty() && depth >= 3 {
            // search with an empty window - we only care about if the score is high or low
            let v =
                -self.visit_node(&position.null_move().unwrap(), -beta - 1, -beta, depth - 3)?;
            if v > beta {
                // Null move pruning
                return Some(v);
            }
        }

        // It is impossible to accidentally return this score because the worst move that could
        // possibly be returned by visit_node is -Eval::MATE.add(1) which is better than this
        let mut best_score = -Eval::MATE;
        let mut best_move = INVALID_MOVE;
        let mut node_kind = NodeKind::UpperBound;

        let hashmove;
        match self.shared.tt.get(&position) {
            None => hashmove = None,
            Some(entry) => {
                hashmove = position.board.is_legal(entry.mv).then(|| entry.mv);

                match entry.kind {
                    _ if entry.search_depth < depth => {}
                    NodeKind::Exact => return Some(entry.eval),
                    NodeKind::LowerBound => {
                        // raise alpha
                        if entry.eval >= beta {
                            // fail-high
                            return Some(entry.eval);
                        }
                        if entry.eval > alpha {
                            alpha = entry.eval;
                        }
                    }
                    NodeKind::UpperBound => {
                        // lower beta
                        if entry.eval <= alpha {
                            // fail-low
                            return Some(entry.eval);
                        }
                        if entry.eval < beta {
                            beta = entry.eval;
                        }
                    }
                }
            }
        }

        let mut ordering = MoveOrdering::new(&position.board, hashmove, *self.killer(position.ply));
        let mut quiets = 0;
        while let Some(mv) = ordering.next(&self.history) {
            let new_pos = &position.play_move(&self.shared.nnue, mv);

            let d = if quiets < 4
                || position.board.color_on(mv.to) == Some(!position.board.side_to_move())
                || !new_pos.board.checkers().is_empty()
            {
                depth
            } else if quiets < 12 && depth >= 2 {
                depth - 1
            } else if depth >= 3 {
                depth - 2
            } else {
                depth
            };

            let mut v = -self.visit_node(new_pos, -beta, -alpha, d - 1)?;

            if v > alpha && d != depth {
                // reduced move unexpected raised alpha; research at full depth
                v = -self.visit_node(new_pos, -beta, -alpha, depth - 1)?;
            }

            let quiet = position.board.color_on(mv.to) != Some(!position.board.side_to_move());
            if quiet {
                quiets += 1;
            }

            if v >= beta {
                self.shared.tt.store(
                    &position,
                    TableEntry {
                        mv,
                        eval: v,
                        search_depth: depth,
                        kind: NodeKind::LowerBound,
                    },
                );
                // caused a beta cutoff, update the killer at this ply
                if quiet {
                    // quiet move - update killer and history
                    *self.killer(position.ply) = mv;
                    self.history
                        .caused_cutoff(position.board.piece_on(mv.from).unwrap(), mv);
                }
                return Some(v);
            } else if quiet {
                // quiet move did not cause cutoff - update history
                self.history
                    .did_not_cause_cutoff(position.board.piece_on(mv.from).unwrap(), mv);
            }
            if v > alpha {
                alpha = v;
                node_kind = NodeKind::Exact;
            }
            if v > best_score {
                best_score = v;
                best_move = mv;
            }
        }

        self.shared.tt.store(
            &position,
            TableEntry {
                mv: best_move,
                eval: best_score,
                search_depth: depth,
                kind: node_kind,
            },
        );

        Some(best_score)
    }
}
