use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use cozy_chess::{Board, Move};

mod eval;
mod nnue;
mod position;
mod search;
mod threading;
mod time;
mod tt;

pub use eval::Eval;
pub use threading::MtFrozenight;
pub use time::TimeConstraint;

use search::{PrivateState, Searcher, INVALID_MOVE};
use time::TimeManager;
use tt::TranspositionTable;

pub use search::all_parameters;

pub struct Frozenight {
    board: Board,
    prehistory: Vec<u64>,
    shared_state: Arc<RwLock<SharedState>>,
    stats: Arc<Statistics>,
    state: PrivateState,
}

#[derive(Clone, Debug)]
pub struct SearchInfo {
    pub eval: Eval,
    pub nodes: u64,
    pub depth: i16,
    pub selective_depth: i16,
    pub best_move: Move,
    pub pv: Vec<Move>,
}

#[derive(Debug, Default)]
struct Statistics {
    selective_depth: AtomicI16,
    nodes: AtomicU64,
}

struct SharedState {
    tt: TranspositionTable,
}

impl Frozenight {
    pub fn new(hash_mb: usize) -> Self {
        Self::create(Arc::new(RwLock::new(SharedState {
            tt: TranspositionTable::new(hash_mb),
        })))
    }

    fn create(shared_state: Arc<RwLock<SharedState>>) -> Self {
        Frozenight {
            board: Default::default(),
            prehistory: vec![],
            shared_state,
            stats: Default::default(),
            state: Default::default(),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn new_game(&mut self) {
        self.state = Default::default();
        Arc::get_mut(&mut self.shared_state)
            .unwrap()
            .get_mut()
            .unwrap()
            .tt
            .increment_age(2);
    }

    pub fn set_position(&mut self, position: Board, moves: impl Iterator<Item = Move>) {
        let mut new = position;
        let age_inc = update_position(&mut new, &mut self.prehistory, &self.board, moves);
        self.board = new;
        Arc::get_mut(&mut self.shared_state)
            .unwrap()
            .get_mut()
            .unwrap()
            .tt
            .increment_age(age_inc);
    }

    pub fn set_hash(&mut self, hash_mb: usize) {
        let mut shared = Arc::get_mut(&mut self.shared_state)
            .unwrap()
            .get_mut()
            .unwrap();
        // drop the existing TT before allocating the new one
        shared.tt = TranspositionTable::new(1);
        shared.tt = TranspositionTable::new(hash_mb);
    }

    pub fn search(
        &mut self,
        time: TimeConstraint,
        mut info: impl FnMut(&SearchInfo),
    ) -> SearchInfo {
        let mut recent_info = SearchInfo {
            eval: Eval::DRAW,
            nodes: 0,
            depth: 0,
            selective_depth: 0,
            best_move: INVALID_MOVE,
            pv: vec![],
        };
        let mut tm = TimeManager::new(&self.board, time);
        self.search_internal(
            time.depth,
            time.nodes,
            &Default::default(),
            tm.deadline(),
            |depth, searcher, best_move, eval| {
                recent_info = SearchInfo {
                    eval,
                    depth,
                    selective_depth: searcher.stats.selective_depth.load(Ordering::Relaxed),
                    nodes: searcher.stats.nodes.load(Ordering::Relaxed),
                    best_move,
                    pv: searcher.extract_pv(depth),
                };
                info(&recent_info);

                tm.update(&recent_info)
            },
        );
        recent_info
    }

    fn search_internal(
        &mut self,
        max_depth: i16,
        max_nodes: u64,
        abort: &AtomicBool,
        deadline: Option<Instant>,
        mut depth_complete: impl FnMut(i16, &mut Searcher, Move, Eval) -> ControlFlow<()>,
    ) {
        self.stats.clear();

        self.with_searcher(max_nodes, abort, deadline, |mut searcher| {
            let mut prev_eval = Eval::DRAW;

            for depth in 1..=max_depth {
                let (eval, mv) = match searcher.search(depth, prev_eval) {
                    Some(v) => v,
                    None => break,
                };

                if depth_complete(depth, &mut searcher, mv, eval).is_break() {
                    break;
                }

                prev_eval = eval;
            }
        })
    }
}

impl Statistics {
    fn clear(&self) {
        self.selective_depth.store(0, Ordering::Relaxed);
        self.nodes.store(0, Ordering::Relaxed);
    }
}

fn update_position(
    board: &mut Board,
    prehistory: &mut Vec<u64>,
    old: &Board,
    moves: impl Iterator<Item = Move>,
) -> u8 {
    let mut moves_since_last = 3;
    if board.same_position(old) {
        moves_since_last = 0;
    }
    let mut occurances = HashMap::<_, i32>::new();

    for mv in moves {
        moves_since_last += 1;
        *occurances.entry(board.hash()).or_default() += 1;
        board.play(mv);
        if board.halfmove_clock() == 0 {
            occurances.clear();
        }
        if board.same_position(old) {
            moves_since_last = 0;
        }
    }

    prehistory.clear();
    prehistory.extend(
        occurances
            .into_iter()
            .filter(|&(_, c)| c > 1)
            .map(|(h, _)| h),
    );
    prehistory.push(board.hash());

    match moves_since_last {
        0 => 0,
        1 | 2 => 1,
        _ => 2,
    }
}
