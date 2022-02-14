use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use nnue::Nnue;
use nohash::{IntMap, IntSet};

mod eval;
mod nnue;
mod search;

pub use eval::Eval;
use search::Searcher;

pub struct Frozenight {
    board: Board,
    history: IntSet<u64>,
    shared_state: Arc<SharedState>,
    abort: Arc<AtomicBool>,
}

struct SharedState {
    nnue: Nnue,
}

impl Frozenight {
    pub fn new() -> Self {
        Frozenight {
            board: Default::default(),
            history: Default::default(),
            shared_state: Arc::new(SharedState { nnue: Nnue::new() }),
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
        alarm: Option<Instant>,
        depth_limit: i16,
        info: impl Listener,
        conclude: impl FnOnce(Eval, Move) + Send + 'static,
    ) {
        self.stop_search();

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
            conclude,
        );

        // Spawn timeout thread
        if let Some(alarm) = alarm {
            let abort = self.abort.clone();
            std::thread::spawn(move || {
                while let Some(to_go) = alarm.checked_duration_since(Instant::now()) {
                    std::thread::sleep(to_go.min(Duration::from_secs(1)));
                    if abort.load(Ordering::Relaxed) {
                        return;
                    }
                }
                abort.store(true, Ordering::Relaxed);
            });
        }
    }

    pub fn stop_search(&mut self) {
        self.abort.store(true, Ordering::Relaxed);
    }
}

impl Drop for Frozenight {
    fn drop(&mut self) {
        self.stop_search();
    }
}

fn spawn_search_thread(
    mut searcher: Searcher,
    board: &Board,
    depth_limit: i16,
    mut listener: impl Listener,
    conclude: impl FnOnce(Eval, Move) + Send + 'static,
) -> JoinHandle<()> {
    let board = board.clone();
    let mut best_move = None;
    std::thread::spawn(move || {
        for depth in 1..depth_limit + 1 {
            if let Some(result) = searcher.search(&board, depth) {
                listener.info(
                    depth,
                    searcher.stats.selective_depth,
                    searcher.stats.nodes,
                    result.0,
                    &board,
                    &[result.1],
                );
                best_move = Some(result);
            } else {
                break;
            }
        }
        let (e, m) = best_move.unwrap();
        conclude(e, m);
    })
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Statistics {
    pub selective_depth: i16,
    pub nodes: u64,
}

pub trait Listener: Send + 'static {
    fn info(
        &mut self,
        depth: i16,
        seldepth: i16,
        nodes: u64,
        eval: Eval,
        board: &Board,
        pv: &[Move],
    );
}

impl Listener for () {
    fn info(&mut self, _: i16, _: i16, _: u64, _: Eval, _: &Board, _: &[Move]) {}
}
