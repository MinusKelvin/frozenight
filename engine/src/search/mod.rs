use std::convert::TryInto;
use std::num::NonZeroU8;

use cozy_chess::*;

use crate::eval::Eval;

mod search;
mod window;
mod cache;
mod moves;
mod helpers;
mod oracle;
mod history;
mod formulas;

use search::*;
use window::Window;
pub use cache::{CacheTable, TableEntry, TableKeyValueEntry};

pub trait SearchHandler {
    fn stop_search(&self) -> bool;
    fn new_result(&mut self, result: SearchResult);
}

impl<H: SearchHandler, R: std::ops::DerefMut<Target=H>> SearchHandler for R {
    fn stop_search(&self) -> bool {
        (**self).stop_search()
    }

    fn new_result(&mut self, search_result: SearchResult) {
        (**self).new_result(search_result)
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub mv: Move,
    pub eval: Eval,
    pub nodes: u64,
    pub depth: u8,
    pub seldepth: u8,
    pub used_cache_entries: u32,
    pub total_cache_entries: u32,
    pub principal_variation: Vec<Move>
}

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub max_depth: NonZeroU8
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            max_depth: 64.try_into().unwrap()
        }
    }
}

pub struct Engine<H> {
    board: Board,
    shared: SearchSharedState<H>,
    options: EngineOptions
}

impl<H: SearchHandler> Engine<H> {
    pub fn new(
        handler: H,
        init_pos: Board,
        moves: impl IntoIterator<Item=Move>,
        options: EngineOptions,
        cache_table: CacheTable
    ) -> Self {
        let mut history = Vec::with_capacity(options.max_depth.get() as usize);
        let mut board = init_pos;
        for mv in moves {
            history.push(board.hash());
            board.play_unchecked(mv);
        }

        Self {
            board,
            shared: SearchSharedState {
                handler,
                history,
                cache_table
            },
            options
        }
    }

    pub fn search(&mut self) {
        let mut prev_eval = None;

        let mut search_data = SearchData::new(self.shared.history.clone());
        for depth in 1..=self.options.max_depth.get() {
            let mut windows = [75].iter().copied().map(Eval::cp);
            let result = loop {
                // CITE: Aspiration window.
                // https://www.chessprogramming.org/Aspiration_Windows
                let mut aspiration_window = Window::INFINITY;
                if depth > 3 {
                    if let Some(prev_eval) = prev_eval {
                        if let Some(bounds) = windows.next() {
                            aspiration_window = Window::around(prev_eval, bounds);
                        }
                    }
                }
                let result = search_data.search(
                    &mut self.shared,
                    &self.board,
                    depth,
                    aspiration_window
                );
                if let Ok(result) = &result {
                    if !aspiration_window.contains(result.eval) {
                        continue;
                    }
                }
                break result;
            };

            if let Ok(SearcherResult { mv, eval, stats }) = result {
                prev_eval = Some(eval);
                let mut principal_variation = Vec::new();
                let mut history = self.shared.history.clone();
                let mut board = self.board.clone();
                while let Some(entry) = self.shared.cache_table.get(&board, 0) {
                    history.push(board.hash());
                    board.play_unchecked(entry.best_move);
                    principal_variation.push(entry.best_move);
                    let repetitions = history.iter()
                        .rev()
                        .take(board.halfmove_clock() as usize + 1)
                        .step_by(2) // Every second ply so it's our turn
                        .skip(1)
                        .filter(|&&hash| hash == board.hash())
                        .count();
                    if repetitions > 2 || board.status() != GameStatus::Ongoing {
                        break;
                    }
                }

                self.shared.handler.new_result(SearchResult {
                    mv,
                    eval,
                    nodes: stats.nodes,
                    depth,
                    seldepth: stats.seldepth,
                    used_cache_entries: self.shared.cache_table.len(),
                    total_cache_entries: self.shared.cache_table.capacity(),
                    principal_variation
                });
            } else {
                break;
            }
        }
    }

    pub fn into_cache_table(self) -> CacheTable {
        self.shared.cache_table
    }
}
