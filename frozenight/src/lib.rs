use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use cozy_syzygy::Tablebase;
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
    tb: Arc<Tablebase>,
}

impl Frozenight {
    pub fn new(hash_mb: usize, tb: Arc<Tablebase>) -> Self {
        Frozenight {
            board: Default::default(),
            history: Default::default(),
            shared_state: Arc::new(SharedState {
                nnue: Nnue::new(),
                tt: TranspositionTable::new(hash_mb),
                tb,
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
                let (e, m) =
                    iterative_deepening(s, depth_limit.min(5000), info, time_use_suggestion);
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
        info: impl FnMut(u16, &Statistics, Eval, &Board, &[Move]),
    ) -> (Eval, Move) {
        self.searcher(0)(|s| {
            iterative_deepening(s, depth_limit.min(5000), info, time_use_suggestion)
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
        tl_data.0.tb_hits.store(0, Ordering::Relaxed);
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
    }
    best_move.unwrap()
}

#[derive(Debug, Default)]
pub struct Statistics {
    pub selective_depth: AtomicU16,
    pub nodes: AtomicU64,
    pub tb_hits: AtomicU64,
}

pub fn load_embedded() -> Tablebase {
    let mut tb = Tablebase::new();
    tb.load_bytes("KBBvK", include_bytes!("../tb/KBBvK.rtbw"));
    tb.load_bytes("KBNvK", include_bytes!("../tb/KBNvK.rtbw"));
    tb.load_bytes("KBPvK", include_bytes!("../tb/KBPvK.rtbw"));
    tb.load_bytes("KBvKB", include_bytes!("../tb/KBvKB.rtbw"));
    tb.load_bytes("KBvKN", include_bytes!("../tb/KBvKN.rtbw"));
    tb.load_bytes("KBvKP", include_bytes!("../tb/KBvKP.rtbw"));
    tb.load_bytes("KBvK", include_bytes!("../tb/KBvK.rtbw"));
    tb.load_bytes("KNNvK", include_bytes!("../tb/KNNvK.rtbw"));
    tb.load_bytes("KNPvK", include_bytes!("../tb/KNPvK.rtbw"));
    tb.load_bytes("KNvKN", include_bytes!("../tb/KNvKN.rtbw"));
    tb.load_bytes("KNvKP", include_bytes!("../tb/KNvKP.rtbw"));
    tb.load_bytes("KNvK", include_bytes!("../tb/KNvK.rtbw"));
    tb.load_bytes("KPPvK", include_bytes!("../tb/KPPvK.rtbw"));
    tb.load_bytes("KPvKP", include_bytes!("../tb/KPvKP.rtbw"));
    tb.load_bytes("KPvK", include_bytes!("../tb/KPvK.rtbw"));
    tb.load_bytes("KQBvK", include_bytes!("../tb/KQBvK.rtbw"));
    tb.load_bytes("KQNvK", include_bytes!("../tb/KQNvK.rtbw"));
    tb.load_bytes("KQPvK", include_bytes!("../tb/KQPvK.rtbw"));
    tb.load_bytes("KQQvK", include_bytes!("../tb/KQQvK.rtbw"));
    tb.load_bytes("KQRvK", include_bytes!("../tb/KQRvK.rtbw"));
    tb.load_bytes("KQvKB", include_bytes!("../tb/KQvKB.rtbw"));
    tb.load_bytes("KQvKN", include_bytes!("../tb/KQvKN.rtbw"));
    tb.load_bytes("KQvKP", include_bytes!("../tb/KQvKP.rtbw"));
    tb.load_bytes("KQvKQ", include_bytes!("../tb/KQvKQ.rtbw"));
    tb.load_bytes("KQvKR", include_bytes!("../tb/KQvKR.rtbw"));
    tb.load_bytes("KQvK", include_bytes!("../tb/KQvK.rtbw"));
    tb.load_bytes("KRBvK", include_bytes!("../tb/KRBvK.rtbw"));
    tb.load_bytes("KRNvK", include_bytes!("../tb/KRNvK.rtbw"));
    tb.load_bytes("KRPvK", include_bytes!("../tb/KRPvK.rtbw"));
    tb.load_bytes("KRRvK", include_bytes!("../tb/KRRvK.rtbw"));
    tb.load_bytes("KRvKB", include_bytes!("../tb/KRvKB.rtbw"));
    tb.load_bytes("KRvKN", include_bytes!("../tb/KRvKN.rtbw"));
    tb.load_bytes("KRvKP", include_bytes!("../tb/KRvKP.rtbw"));
    tb.load_bytes("KRvKR", include_bytes!("../tb/KRvKR.rtbw"));
    tb.load_bytes("KRvK", include_bytes!("../tb/KRvK.rtbw"));
    tb
}
