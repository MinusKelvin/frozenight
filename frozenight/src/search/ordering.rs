use cozy_chess::Move;

use crate::position::Position;

use super::see::static_exchange_eval;
use super::table::HistoryTable;
use super::{PrivateState, Searcher};

const MAX_HISTORY: i32 = 4096;

pub struct MovePicker<'a> {
    pos: &'a Position,
    hashmv: Option<Move>,
    moves: Vec<(Move, MoveScore)>,
    next: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoveScore {
    BadCapture(i16),
    Quiet(i16),
    GoodCapture(i16),
    Hash,
}

impl<'a> MovePicker<'a> {
    pub fn new(pos: &'a Position, hashmv: Option<Move>) -> Self {
        MovePicker {
            pos,
            hashmv,
            moves: Vec::with_capacity(64),
            next: 0,
        }
    }

    pub(super) fn pick_move(&mut self, state: &PrivateState) -> Option<(usize, Move, MoveScore)> {
        let i = self.next;
        match self.hashmv {
            Some(mv) if i == 0 => {
                self.next += 1;
                return Some((i, mv, MoveScore::Hash));
            }
            _ if self.moves.is_empty() => {
                if let Some(mv) = self.hashmv {
                    self.moves.push((mv, MoveScore::Hash));
                }

                let stm = self.pos.board.side_to_move();
                let capture_targets = self.pos.board.colors(!stm);

                self.pos.board.generate_moves(|mvs| {
                    for mv in mvs {
                        let score = match () {
                            _ if Some(mv) == self.hashmv => continue,
                            _ if capture_targets.has(mv.to) => {
                                let see = static_exchange_eval(&self.pos.board, mv);
                                let score = self.pos.board.piece_on(mv.to).unwrap() as i16 * 8
                                    - mvs.piece as i16;
                                if see >= 0 {
                                    MoveScore::GoodCapture(score)
                                } else {
                                    MoveScore::BadCapture(score)
                                }
                            }
                            _ => {
                                let mut score = state.history[stm][mvs.piece][mv.to];
                                if let Some(table) = state.counter_hist_table(self.pos) {
                                    score += table[stm][mvs.piece][mv.to];
                                }
                                if let Some(table) = state.followup_hist_table(self.pos) {
                                    score += table[stm][mvs.piece][mv.to];
                                }
                                MoveScore::Quiet(score)
                            }
                        };
                        self.moves.push((mv, score));
                    }
                    false
                });

                self.moves.sort_unstable_by_key(|&(_, s)| std::cmp::Reverse(s));
            }
            _ => {}
        }

        let &(mv, score) = self.moves.get(i)?;
        self.next += 1;
        Some((i, mv, score))
    }
}

impl Searcher<'_> {
    pub fn update_history(&mut self, picker: MovePicker, cutoff_move: Move, depth: i16) {
        let change = depth as i32 * depth as i32;
        let stm = picker.pos.board.side_to_move();

        if picker.pos.is_capture(cutoff_move) {
            return;
        }

        for &(mv, _) in &picker.moves[..picker.next - 1] {
            if picker.pos.is_capture(mv) {
                continue;
            }

            let piece = picker.pos.board.piece_on(mv.from).unwrap();
            history_dec(&mut self.state.history[stm][piece][mv.to], change);

            if let Some(table) = self.state.counter_hist_table_mut(picker.pos) {
                history_dec(&mut table[stm][piece][mv.to], change);
            }

            if let Some(table) = self.state.followup_hist_table_mut(picker.pos) {
                history_dec(&mut table[stm][piece][mv.to], change);
            }
        }

        let piece = picker.pos.board.piece_on(cutoff_move.from).unwrap();
        history_inc(&mut self.state.history[stm][piece][cutoff_move.to], change);

        if let Some(table) = self.state.counter_hist_table_mut(picker.pos) {
            history_inc(&mut table[stm][piece][cutoff_move.to], change);
        }

        if let Some(table) = self.state.followup_hist_table_mut(picker.pos) {
            history_inc(&mut table[stm][piece][cutoff_move.to], change);
        }
    }
}

impl PrivateState {
    fn counter_hist_table(&self, pos: &Position) -> Option<&HistoryTable<i16>> {
        if pos.ply == 0 {
            return None;
        }
        let stm = pos.board.side_to_move();
        match self.move_stack[pos.ply as usize - 1] {
            Some((p, s)) => Some(&self.cont_hist[!stm][p][s]),
            None => Some(&self.null_move_conthist[!stm]),
        }
    }

    fn counter_hist_table_mut(&mut self, pos: &Position) -> Option<&mut HistoryTable<i16>> {
        if pos.ply == 0 {
            return None;
        }
        let stm = pos.board.side_to_move();
        match self.move_stack[pos.ply as usize - 1] {
            Some((p, s)) => Some(&mut self.cont_hist[!stm][p][s]),
            None => Some(&mut self.null_move_conthist[!stm]),
        }
    }

    fn followup_hist_table(&self, pos: &Position) -> Option<&HistoryTable<i16>> {
        if pos.ply <= 1 {
            return None;
        }
        let stm = pos.board.side_to_move();
        match self.move_stack[pos.ply as usize - 2] {
            Some((p, s)) => Some(&self.cont_hist[stm][p][s]),
            None => Some(&self.null_move_conthist[stm]),
        }
    }

    fn followup_hist_table_mut(&mut self, pos: &Position) -> Option<&mut HistoryTable<i16>> {
        if pos.ply <= 1 {
            return None;
        }
        let stm = pos.board.side_to_move();
        match self.move_stack[pos.ply as usize - 2] {
            Some((p, s)) => Some(&mut self.cont_hist[stm][p][s]),
            None => Some(&mut self.null_move_conthist[stm]),
        }
    }
}

fn history_inc(hist: &mut i16, change: i32) {
    *hist += (change - change * *hist as i32 / MAX_HISTORY) as i16;
}

fn history_dec(hist: &mut i16, change: i32) {
    *hist -= (change + change * *hist as i32 / MAX_HISTORY) as i16;
}
