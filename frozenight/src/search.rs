use std::sync::atomic::{AtomicBool, Ordering};

use cozy_chess::{Board, Move, Square};
use nohash::IntSet;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

use self::ordering::{HistoryTable, MoveOrdering};
use self::window::Window;

mod ordering;
mod qsearch;
mod window;

const INVALID_MOVE: Move = Move {
    from: Square::A1,
    to: Square::A1,
    promotion: None,
};

pub(crate) struct SearchState {
    killers: Vec<Move>,
    history: HistoryTable,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState {
            killers: vec![INVALID_MOVE; 128],
            history: HistoryTable::new(),
        }
    }
}

pub(crate) struct Searcher<'a> {
    pub root: Board,
    pub stats: &'a Statistics,
    pub shared: &'a SharedState,
    abort: &'a AtomicBool,
    valid: bool,
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
    ) -> Self {
        state.history.decay();
        Searcher {
            root,
            shared,
            abort,
            repetition,
            state,
            stats,
            valid: true,
        }
    }

    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, depth: u16) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        if !self.valid {
            panic!("attempt to search using an aborted searcher");
        }
        let mut window = Window::default();
        let mut best_move = INVALID_MOVE;

        let position = Position::from_root(self.root.clone(), &self.shared.nnue);

        let hashmove = self
            .shared
            .tt
            .get(&position)
            .and_then(|entry| self.root.is_legal(entry.mv).then(|| entry.mv));

        let mut orderer = MoveOrdering::new(&position.board, hashmove, INVALID_MOVE);
        let mut quiets = 0;
        while let Some(mv) = orderer.next(&self.state.history) {
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

            let mut v = -self.visit_node(new_pos, -window, d - 1)?;

            if d != depth && v > window.lb() {
                v = -self.visit_node(new_pos, -window, depth - 1)?;
            }

            if window.raise_lb(v) {
                best_move = mv;
            }

            if !self.root.colors(!self.root.side_to_move()).has(mv.to) {
                quiets += 1;
            }
        }

        if best_move == INVALID_MOVE {
            panic!("root position (FEN: {}) has no moves", self.root);
        }

        Some((window.lb(), best_move))
    }

    fn killer(&mut self, ply_index: u16) -> &mut Move {
        let idx = ply_index as usize;
        if idx >= self.state.killers.len() {
            self.state
                .killers
                .extend((self.state.killers.len()..=idx).map(|_| INVALID_MOVE));
        }
        &mut self.state.killers[idx]
    }

    fn visit_node(&mut self, position: &Position, window: Window, depth: u16) -> Option<Eval> {
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
            self.qsearch(position, window)
        } else {
            self.alpha_beta(position, window, depth)?
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
    fn alpha_beta(&mut self, position: &Position, mut window: Window, depth: u16) -> Option<Eval> {
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

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
                        if window.fail_high(entry.eval) {
                            return Some(entry.eval);
                        }
                    }
                    NodeKind::UpperBound => {
                        if window.fail_low(entry.eval) {
                            return Some(entry.eval);
                        }
                    }
                }
            }
        }

        // reverse futility pruning... but with qsearch
        if depth <= 6 {
            let margin = 250 * depth as i16;
            let rfp_window = Window::test_lower_ub(window.ub() + margin);
            let eval = self.qsearch(position, rfp_window);
            if rfp_window.fail_high(eval) {
                return Some(eval);
            }
        }

        if position.board.checkers().is_empty() && depth >= 3 {
            // search with an empty window - we only care about if the score is high or low
            let nmp_window = Window::test_lower_ub(window.ub());
            let v = -self.visit_node(&position.null_move().unwrap(), -nmp_window, depth - 3)?;
            if nmp_window.fail_high(v) {
                // Null move pruning
                return Some(v);
            }
        }

        let mut ordering = MoveOrdering::new(&position.board, hashmove, *self.killer(position.ply));
        let mut quiets = 0;
        while let Some(mv) = ordering.next(&self.state.history) {
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

            let mut v = -self.visit_node(new_pos, -window, d - 1)?;

            if !window.fail_low(v) && d < depth {
                // reduced move unexpectedly raised alpha; research at full depth
                v = -self.visit_node(new_pos, -window, depth - 1)?;
            }

            let quiet = position.board.color_on(mv.to) != Some(!position.board.side_to_move());
            if quiet {
                quiets += 1;
            }

            if window.fail_high(v) {
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
                    self.state.history.caused_cutoff(
                        position.board.piece_on(mv.from).unwrap(),
                        mv,
                        position.board.side_to_move(),
                    );
                }
                return Some(v);
            } else if quiet {
                // quiet move did not cause cutoff - update history
                self.state.history.did_not_cause_cutoff(
                    position.board.piece_on(mv.from).unwrap(),
                    mv,
                    position.board.side_to_move(),
                );
            }

            if window.raise_lb(v) {
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
