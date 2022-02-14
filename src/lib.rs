use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use nohash::{IntMap, IntSet};

mod eval;
mod search;

pub use eval::Eval;
use search::Searcher;

pub struct Frozenight {
    board: Board,
    history: IntSet<u64>,
    shared_state: Arc<SharedState>,
    running: Arc<AtomicBool>,
}

struct SharedState {}

impl Frozenight {
    pub fn new() -> Self {
        Frozenight {
            board: Default::default(),
            history: Default::default(),
            shared_state: Arc::new(SharedState {}),
            running: Default::default(),
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
        depth_limit: u16,
        info: impl Listener,
        conclude: impl FnOnce(Eval, Move) + Send + 'static,
    ) {
        self.stop_search();

        // Create a new running search variable
        self.running = Arc::new(AtomicBool::new(true));

        // Start main search thread
        spawn_search_thread(
            Searcher::new(
                self.running.clone(),
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
            let running = self.running.clone();
            std::thread::spawn(move || {
                while let Some(to_go) = alarm.checked_duration_since(Instant::now()) {
                    std::thread::sleep(to_go.min(Duration::from_secs(1)));
                    if !running.load(Ordering::Relaxed) {
                        return;
                    }
                }
                running.store(false, Ordering::Relaxed);
            });
        }
    }

    pub fn stop_search(&mut self) {
        self.running.store(false, Ordering::Relaxed);
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
    depth_limit: u16,
    mut listener: impl Listener,
    conclude: impl FnOnce(Eval, Move) + Send + 'static,
) -> JoinHandle<()> {
    let board = board.clone();
    let mut best_move = None;
    std::thread::spawn(move || {
        for depth in 0..depth_limit {
            if let Some(result) = searcher.search(&board, depth) {
                listener.info(
                    depth + 1,
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
        best_move.map(|(e, m)| conclude(e, m));
    })
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Statistics {
    pub selective_depth: u16,
    pub nodes: u64,
}

pub trait Listener: Send + 'static {
    fn info(
        &mut self,
        depth: u16,
        seldepth: u16,
        nodes: u64,
        eval: Eval,
        board: &Board,
        pv: &[Move],
    );
}
