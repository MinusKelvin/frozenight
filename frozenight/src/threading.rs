use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use cozy_chess::Board;

use crate::search::{AbdadaTable, INVALID_MOVE};
use crate::time::{TimeConstraint, TimeManager};
use crate::tt::TranspositionTable;
use crate::{Eval, Frozenight, SearchInfo, SharedState, Statistics};

pub struct MtFrozenight {
    board: Board,
    shared_state: Arc<RwLock<SharedState>>,
    threads: Vec<(Arc<Statistics>, Sender<ThreadCommand>)>,
    abort: Arc<AtomicBool>,
}

enum ThreadCommand {
    SetPosition(Board),
    Go {
        multithreaded: bool,
        max_nodes: u64,
        max_depth: i16,
        deadline: Option<Instant>,
        state: Arc<Mutex<MtSyncState>>,
        abort: Arc<AtomicBool>,
    },
    NewGame,
}

struct MtSyncState {
    recent_info: SearchInfo,
    tm: TimeManager,
    info: Box<dyn FnMut(&SearchInfo) + Send>,
    finish: Option<Box<dyn FnOnce(&SearchInfo) + Send>>,
    stats: Vec<Arc<Statistics>>,
}

impl MtFrozenight {
    pub fn new(hash_mb: usize) -> Self {
        let mut this = MtFrozenight {
            board: Default::default(),
            shared_state: Arc::new(RwLock::new(SharedState {
                tt: TranspositionTable::new(hash_mb),
                abdada: AbdadaTable::new(),
            })),
            threads: vec![],
            abort: Default::default(),
        };
        this.set_threads(1);
        this
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn set_threads(&mut self, threads: usize) {
        self.threads.resize_with(threads, || {
            let (sender, recv) = channel();
            let engine = Frozenight::create(self.shared_state.clone());
            let stats = engine.stats.clone();
            std::thread::spawn(|| run_thread(engine, recv));
            let _ = sender.send(ThreadCommand::SetPosition(self.board.clone()));
            (stats, sender)
        });
    }

    pub fn set_hash(&mut self, hash_mb: usize) {
        self.abort();
        let mut state = self.shared_state.write().unwrap();
        // put dummy value in to drop potentially large previous TT allocation
        state.tt = TranspositionTable::new(1);
        // then create potentially large new TT allocation
        state.tt = TranspositionTable::new(hash_mb);
    }

    pub fn set_position(&mut self, position: Board) {
        if self.board.same_position(&position) {
            return;
        }
        self.abort();
        self.board = position;
        self.shared_state.write().unwrap().tt.increment_age(1);

        for (_, thread) in &self.threads {
            let _ = thread.send(ThreadCommand::SetPosition(self.board.clone()));
        }
    }

    pub fn new_game(&mut self) {
        for (_, thread) in &self.threads {
            let _ = thread.send(ThreadCommand::NewGame);
        }
    }

    pub fn abort(&mut self) {
        self.abort.store(true, Ordering::Relaxed);
    }

    pub fn search(
        &mut self,
        time: TimeConstraint,
        info: impl FnMut(&SearchInfo) + Send + 'static,
        finish: impl FnMut(&SearchInfo) + Send + 'static,
    ) {
        self.abort();
        self.abort = Default::default();

        let stats = self
            .threads
            .iter()
            .map(|(stats, _)| stats.clone())
            .collect();
        let tm = TimeManager::new(&self.board, time);
        let mut deadline = tm.deadline();

        let state = Arc::new(Mutex::new(MtSyncState {
            recent_info: SearchInfo {
                eval: Eval::DRAW,
                nodes: 0,
                depth: 0,
                selective_depth: 0,
                best_move: INVALID_MOVE,
                pv: vec![],
            },
            tm,
            info: Box::new(info),
            finish: Some(Box::new(finish)),
            stats,
        }));

        let multithreaded = self.threads.len() > 1;
        for (_, sender) in &self.threads {
            let _ = sender.send(ThreadCommand::Go {
                multithreaded,
                max_nodes: time.nodes,
                max_depth: time.depth,
                deadline: deadline.take(),
                state: state.clone(),
                abort: self.abort.clone(),
            });
        }
    }
}

fn run_thread(mut engine: Frozenight, recv: Receiver<ThreadCommand>) {
    while let Ok(cmd) = recv.recv() {
        match cmd {
            ThreadCommand::SetPosition(root) => {
                engine.board = root;
            }
            ThreadCommand::NewGame => {
                engine.stats.clear();
            }
            ThreadCommand::Go {
                multithreaded,
                max_nodes,
                max_depth,
                deadline,
                state,
                abort,
            } => {
                engine.search_internal(
                    max_depth,
                    max_nodes,
                    &abort,
                    multithreaded,
                    deadline,
                    |depth, searcher, mv, eval| {
                        let mut state = state.lock().unwrap();
                        let state = &mut *state;
                        if depth <= state.recent_info.depth {
                            return ControlFlow::Continue(());
                        }

                        let mut nodes = 0;
                        let mut selective_depth = 0;

                        for stats in &state.stats {
                            nodes += stats.nodes.load(Ordering::Relaxed);
                            selective_depth =
                                selective_depth.max(stats.selective_depth.load(Ordering::Relaxed));
                        }

                        state.recent_info = SearchInfo {
                            eval,
                            depth,
                            selective_depth,
                            nodes,
                            best_move: mv,
                            pv: searcher.extract_pv(depth),
                        };
                        (state.info)(&state.recent_info);
                        state.tm.update(&state.recent_info)
                    },
                );

                abort.store(true, Ordering::Relaxed);
                let mut state = state.lock().unwrap();
                if let Some(finish) = state.finish.take() {
                    finish(&state.recent_info);
                }
            }
        }
    }
}
