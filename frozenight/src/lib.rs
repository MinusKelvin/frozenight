use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use nohash::{IntMap, IntSet};

mod eval;
mod nnue;
mod position;
mod search;
mod tt;

pub use eval::Eval;
use nnue::Nnue;
use search::{SearchState, Searcher};
use tt::TranspositionTable;

pub struct Frozenight {
    board: Board,
    history: IntSet<u64>,
    shared_state: Arc<SharedState>,
    tl_data: Vec<Arc<(Statistics, Mutex<SearchState>)>>,
    abort: Arc<AtomicBool>,
}

struct SharedState {
    nnue: Nnue,
    tt: TranspositionTable,
}

impl Frozenight {
    pub fn new(hash_mb: usize) -> Self {
        Frozenight {
            board: Default::default(),
            history: Default::default(),
            shared_state: Arc::new(SharedState {
                nnue: Nnue::new(),
                tt: TranspositionTable::new(hash_mb),
            }),
            tl_data: vec![],
            abort: Default::default(),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn set_position(&mut self, start: Board, mut moves: impl FnMut(&Board) -> Option<Move>) {
        let old_hash = self.board.hash();
        let mut moves_since_occurance = -1;
        self.board = start;
        let mut occurances = IntMap::<_, usize>::default();
        while let Some(mv) = moves(&self.board) {
            *occurances.entry(self.board.hash()).or_default() += 1;
            if self.board.hash() == old_hash {
                moves_since_occurance = 0;
            } else if moves_since_occurance >= 0 {
                moves_since_occurance += 1;
            }
            self.board.play(mv);
        }
        self.shared_state
            .tt
            .increment_age(match moves_since_occurance {
                0..=4 => 1,
                _ => 2,
            });
        self.history = occurances
            .into_iter()
            .filter(|&(_, count)| count > 1)
            .map(|(hash, _)| hash)
            .collect();
        self.history.insert(self.board.hash());
    }

    pub fn start_search(
        &mut self,
        time_use_suggestion: Option<Instant>,
        deadline: Option<Instant>,
        depth_limit: u16,
        nodes_limit: u64,
        info: impl FnMut(u16, &Statistics, Eval, &Board, &[Move]) + Send + 'static,
        best_move: impl FnOnce(Eval, Move, &Board) + Send + 'static,
    ) -> Abort {
        self.abort.store(true, Ordering::Relaxed);

        // Create a new abort search variable
        self.abort = Arc::new(AtomicBool::new(false));

        // Start main search thread
        let searcher = self.searcher(0);
        std::thread::spawn(move || {
            searcher(move |s| {
                let root = s.root.clone();
                let (e, m) = iterative_deepening(
                    s,
                    depth_limit.min(5000),
                    nodes_limit,
                    info,
                    time_use_suggestion,
                );
                best_move(e, m, &root);
            })
        });

        // Spawn timeout thread
        if let Some(deadline) = deadline {
            let abort = self.abort.clone();
            std::thread::spawn(move || {
                while let Some(to_go) = deadline.checked_duration_since(Instant::now()) {
                    std::thread::sleep(to_go.min(Duration::from_secs(1)));
                    if abort.load(Ordering::Relaxed) {
                        return;
                    }
                }
                abort.store(true, Ordering::Relaxed);
            });
        }

        Abort(Some(self.abort.clone()))
    }

    pub fn search_synchronous(
        &mut self,
        time_use_suggestion: Option<Instant>,
        depth_limit: u16,
        nodes_limit: u64,
        info: impl FnMut(u16, &Statistics, Eval, &Board, &[Move]),
    ) -> (Eval, Move) {
        self.searcher(0)(|s| {
            iterative_deepening(
                s,
                depth_limit.min(5000),
                nodes_limit,
                info,
                time_use_suggestion,
            )
        })
    }

    fn searcher<F: FnOnce(Searcher) -> R, R>(
        &mut self,
        thread: usize,
    ) -> impl FnOnce(F) -> R + Send {
        let abort = self.abort.clone();
        let shared = self.shared_state.clone();
        while thread >= self.tl_data.len() {
            self.tl_data.push(Arc::new((
                Statistics::default(),
                Mutex::new(SearchState::default()),
            )));
        }
        let tl_data = self.tl_data[thread].clone();
        tl_data.0.nodes.store(0, Ordering::Relaxed);
        tl_data.0.selective_depth.store(0, Ordering::Relaxed);
        let repetitions = self.history.clone();
        let board = self.board.clone();
        move |f| {
            f(Searcher::new(
                &abort,
                &shared,
                &mut tl_data.1.lock().unwrap(),
                &tl_data.0,
                repetitions,
                board,
            ))
        }
    }
}

pub struct Abort(Option<Arc<AtomicBool>>);

impl Abort {
    pub fn abort(self) {}
    pub fn forget(mut self) {
        self.0.take();
    }
}

impl Drop for Abort {
    fn drop(&mut self) {
        if let Some(abort) = self.0.as_ref() {
            abort.store(true, Ordering::Relaxed);
        }
    }
}

fn iterative_deepening(
    mut searcher: Searcher,
    depth_limit: u16,
    nodes_limit: u64,
    mut info: impl FnMut(u16, &Statistics, Eval, &Board, &[Move]),
    time_use_suggestion: Option<Instant>,
) -> (Eval, Move) {
    let mut best_move = None;
    let mut pv = Vec::with_capacity(32);
    for depth in 1..depth_limit + 1 {
        if let Some(result) = searcher.search(depth as i16) {
            pv.clear();
            pv.push(result.1);
            let mut b = searcher.root.clone();
            b.play(result.1);
            let mut mvs = 0;
            while let Some(mv) = searcher.shared.tt.get_move(&b) {
                mvs += 1;
                if mvs < depth && b.try_play(mv).unwrap() {
                    pv.push(mv);
                } else {
                    break;
                }
            }
            info(depth, searcher.stats, result.0, &searcher.root, &pv);
            best_move = Some(result);
        } else {
            break;
        }

        if let Some(time_use_suggestion) = time_use_suggestion {
            if Instant::now() > time_use_suggestion {
                break;
            }
        }

        if searcher.stats.nodes.load(Ordering::Relaxed) >= nodes_limit {
            break;
        }
    }
    best_move.unwrap()
}

#[derive(Debug, Default)]
pub struct Statistics {
    pub selective_depth: AtomicU16,
    pub nodes: AtomicU64,
}
