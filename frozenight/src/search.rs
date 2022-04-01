use std::sync::atomic::{AtomicBool, Ordering};

use cozy_chess::{Board, Move, Square};
use cozy_syzygy::Wdl;
use nohash::IntSet;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::{Eval, SharedState, Statistics};

use self::ordering::HistoryTable;
use self::window::Window;

mod null;
mod ordering;
mod pv;
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

    fn killer(&mut self, ply_index: u16) -> &mut Move {
        let idx = ply_index as usize;
        if idx >= self.state.killers.len() {
            self.state
                .killers
                .extend((self.state.killers.len()..=idx).map(|_| INVALID_MOVE));
        }
        &mut self.state.killers[idx]
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

        if position.board.halfmove_clock() == 0
            && position.board.occupied().popcnt() <= self.shared.tb.max_pieces()
        {
            let result = self.shared.tb.probe_wdl(&position.board).map(|t| t.0);
            if result.is_some() {
                self.stats.tb_hits.fetch_add(1, Ordering::Relaxed);
            }
            match result {
                Some(Wdl::Win) => return Some(Eval::TB_WIN.add_time(position.ply)),
                Some(Wdl::Loss) => return Some(-Eval::TB_WIN.add_time(position.ply)),
                Some(_) => return Some(Eval::DRAW),
                None => {}
            }
        }

        if !self.repetition.insert(position.board.hash()) {
            return Some(Eval::DRAW);
        }

        let result = if depth <= 0 {
            self.qsearch(position, window)
        } else {
            self.stats.nodes.fetch_add(1, Ordering::Relaxed);
            f(self)?
        };

        // Sanity check that conclusive scores are valid
        #[cfg(debug_assertions)]
        if let Some(plys) = result.plys_to_conclusion() {
            debug_assert!(plys.abs() >= position.ply as i16);
        }

        self.repetition.remove(&position.board.hash());
        Some(result)
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
        if !position.is_capture(mv) {
            self.state.history.caused_cutoff(&position.board, mv);
            *self.killer(position.ply) = mv;
        }
    }
}
