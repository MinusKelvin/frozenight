use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use nohash::{IntMap, IntSet};

mod eval;
mod nnue;
mod search;
mod tt;

pub use eval::Eval;
use nnue::Nnue;
use search::Searcher;
use tt::TranspositionTable;

pub struct Frozenight {
    board: Board,
    history: IntSet<u64>,
    shared_state: Arc<SharedState>,
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
            abort: Default::default(),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn set_position(&mut self, start: Board, mut moves: impl FnMut(&Board) -> Option<Move>) {
        self.board = start;
        let mut occurances = IntMap::<_, usize>::default();
        *occurances.entry(self.board.hash()).or_default() += 1;
        while let Some(mv) = moves(&self.board) {
            self.board.play(mv);
            *occurances.entry(self.board.hash()).or_default() += 1;
        }
        self.history = occurances
            .into_iter()
            .filter(|&(_, count)| count > 1)
            .map(|(hash, _)| hash)
            .collect();
    }

    pub fn start_search(
        &mut self,
        time_use_suggestion: Option<Instant>,
        deadline: Option<Instant>,
        depth_limit: u16,
        info: impl Listener,
    ) -> Abort {
        self.abort.store(true, Ordering::Relaxed);

        // Create a new abort search variable
        self.abort = Arc::new(AtomicBool::new(false));

        // Start main search thread
        spawn_search_thread(
            Searcher::new(
                self.abort.clone(),
                self.shared_state.clone(),
                self.history.clone(),
            ),
            &self.board,
            depth_limit.min(5000),
            info,
            time_use_suggestion,
        );

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

fn spawn_search_thread(
    mut searcher: Searcher,
    board: &Board,
    depth_limit: u16,
    mut listener: impl Listener,
    time_use_suggestion: Option<Instant>
) -> JoinHandle<()> {
    let board = board.clone();
    let mut best_move = None;
    std::thread::spawn(move || {
        for depth in 1..depth_limit + 1 {
            if let Some(result) = searcher.search(&board, depth) {
                listener.info(depth, searcher.stats, result.0, &board, &[result.1]);
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
        let (e, m) = best_move.unwrap();
        listener.best_move(m, e);
    })
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Statistics {
    pub selective_depth: u16,
    pub nodes: u64,
}

pub trait Listener: Send + 'static {
    fn info(&mut self, depth: u16, stats: Statistics, eval: Eval, board: &Board, pv: &[Move]);

    fn best_move(self, mv: Move, eval: Eval);
}

impl Listener for () {
    fn info(&mut self, _: u16, _: Statistics, _: Eval, _: &Board, _: &[Move]) {}
    fn best_move(self, _: Move, _: Eval) {}
}

impl<F> Listener for F
where
    F: FnOnce(Move, Eval) + Send + 'static,
{
    fn info(&mut self, _: u16, _: Statistics, _: Eval, _: &Board, _: &[Move]) {}

    fn best_move(self, mv: Move, eval: Eval) {
        self(mv, eval)
    }
}
