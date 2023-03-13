use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move, Piece, Square};

use crate::position::Position;
use crate::search::negamax::Pv;
use crate::tt::TranspositionTable;
use crate::{Eval, Frozenight, Statistics};

pub use self::params::all_parameters;
use self::table::HistoryTable;
use self::window::Window;

mod negamax;
mod oracle;
mod ordering;
mod params;
mod qsearch;
mod see;
mod table;
mod window;

pub const INVALID_MOVE: Move = Move {
    from: Square::A1,
    to: Square::A1,
    promotion: None,
};

pub(crate) struct PrivateState {
    history: HistoryTable<i16>,
    continuation_history: HistoryTable<HistoryTable<i16>>,
    search_stack: [StackState; 512],
}

#[derive(Default, Copy, Clone)]
struct StackState {
    mv: Option<(Piece, Move)>,
}

impl Default for PrivateState {
    fn default() -> Self {
        PrivateState {
            history: Default::default(),
            continuation_history: Default::default(),
            search_stack: [Default::default(); 512],
        }
    }
}

impl PrivateState {
    fn stack(&self, ply: i16) -> &StackState {
        &self.search_stack[(ply + 2) as usize]
    }

    fn stack_mut(&mut self, ply: i16) -> &mut StackState {
        &mut self.search_stack[(ply + 2) as usize]
    }
}

pub(crate) struct Searcher<'a> {
    pub root: &'a Board,
    pub stats: &'a Statistics,
    pub tt: &'a TranspositionTable,
    pub node_limit: u64,
    pub abort: &'a AtomicBool,
    state: &'a mut PrivateState,
    valid: bool,
    allow_abort: bool,
    deadline: Option<Instant>,
    next_deadline_check: u64,
    rep_list: Vec<u64>,
    rep_table: [u8; 1024],
}

impl Frozenight {
    pub(super) fn with_searcher<T>(
        &mut self,
        node_limit: u64,
        abort: &AtomicBool,
        deadline: Option<Instant>,
        f: impl FnOnce(Searcher) -> T,
    ) -> T {
        let mut rep_table = [0; 1024];
        for &b in &self.prehistory {
            rep_table[b as usize % 1024] += 1;
        }
        let tt = self.tt.read().unwrap();
        f(Searcher {
            root: &self.board,
            tt: &tt,
            abort,
            state: &mut self.state,
            stats: &self.stats,
            rep_table,
            node_limit,
            deadline,
            next_deadline_check: match deadline {
                Some(deadline) => deadline
                    .checked_duration_since(Instant::now())
                    .map_or(0, estimate_nodes_to_deadline),
                None => u64::MAX,
            },
            valid: true,
            allow_abort: false,
            rep_list: self.prehistory.clone(),
        })
    }
}

impl<'a> Searcher<'a> {
    /// Launch the search.
    ///
    /// Invariant: `self` is unchanged if this function returns `Some`. If it returns none, then
    /// calling this function again will result in a panic.
    pub fn search(&mut self, depth: i16, around: Eval) -> Option<(Eval, Move)> {
        assert!(depth > 0);
        self.allow_abort = depth > 1;
        if !self.valid {
            panic!("attempt to search using an aborted searcher");
        }

        if !self.root.generate_moves(|_| true) {
            panic!("root position (FEN: {}) has no moves", self.root);
        }

        let position = &Position::from_root(self.root.clone());

        let (eval, mv) = self.negamax(Pv, position, Window::default(), depth)?;

        Some((eval, mv.expect("Search did not find a move at the root")))
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

    pub fn extract_pv(&mut self, depth: i16) -> Vec<Move> {
        let mut board = self.root.clone();
        let mut pv = Vec::with_capacity(16);
        while let Some(mv) = self.tt.get_move(&board) {
            pv.push(mv);
            board.play_unchecked(mv);
            if pv.len() > depth as usize {
                break;
            }
        }
        pv
    }
}

fn estimate_nodes_to_deadline(d: Duration) -> u64 {
    // assume we get at least 1 mnps (very conservative)
    1000 * d.as_millis().min(1) as u64
}
