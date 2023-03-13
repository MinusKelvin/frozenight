use cozy_chess::{Color, Move, Piece, Square};

use crate::position::Position;

use super::see::static_exchange_eval;
use super::{PrivateState, Searcher, INVALID_MOVE};

const MAX_HISTORY: i32 = 4096;

pub struct MovePicker<'a> {
    pos: &'a Position,
    hashmv: Option<Move>,
    moves: Vec<(Move, MoveScore)>,
    next: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoveScore {
    Quiet(i16),
    Capture(i16),
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

    pub(crate) fn pick_move(&mut self, state: &PrivateState) -> Option<(usize, Move, MoveScore)> {
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
                            _ if capture_targets.has(mv.to) => MoveScore::Capture(
                                self.pos.board.piece_on(mv.to).unwrap() as i16 * 8
                                    - mvs.piece as i16,
                            ),
                            _ => {
                                let mut history = state.history[stm][mvs.piece][mv.to];
                                if let Some((c_piece, c_mv)) = state.stack(self.pos.ply - 1).mv {
                                    history += state.continuation_history[!stm][c_piece][c_mv.to]
                                        [stm][mvs.piece][mv.to];
                                }
                                MoveScore::Quiet(history)
                            }
                        };
                        self.moves.push((mv, score));
                    }
                    false
                });
            }
            _ => {}
        }

        let (j, &(mv, score)) = self.moves[i..]
            .iter()
            .enumerate()
            .max_by_key(|&(_, &(_, s))| s)?;
        self.moves[i..].swap(0, j);
        self.next += 1;
        Some((i, mv, score))
    }
}

impl Searcher<'_> {
    pub fn update_history(&mut self, picker: MovePicker, cutoff_move: Move, depth: i16) {
        let change = depth as i32 * depth as i32;
        let stm = picker.pos.board.side_to_move();
        let capture_targets = picker.pos.board.colors(!stm);

        for &(mv, _) in &picker.moves[..picker.next - 1] {
            if capture_targets.has(mv.to) {
                continue;
            }

            let piece = picker.pos.board.piece_on(mv.from).unwrap();
            history_dec(&mut self.state.history[stm][piece][mv.to], change);

            if let Some((c_piece, c_mv)) = self.state.stack(picker.pos.ply).mv {
                history_dec(
                    &mut self.state.continuation_history[!stm][c_piece][c_mv.to][stm][piece][mv.to],
                    change,
                );
            }
        }

        let piece = picker.pos.board.piece_on(cutoff_move.from).unwrap();
        history_inc(&mut self.state.history[stm][piece][cutoff_move.to], change);

        if let Some((c_piece, c_mv)) = self.state.stack(picker.pos.ply).mv {
            history_inc(
                &mut self.state.continuation_history[!stm][c_piece][c_mv.to][stm][piece]
                    [cutoff_move.to],
                change,
            );
        }
    }
}

fn history_inc(hist: &mut i16, change: i32) {
    *hist += (change - change * *hist as i32 / MAX_HISTORY) as i16;
}

fn history_dec(hist: &mut i16, change: i32) {
    *hist -= (change + change * *hist as i32 / MAX_HISTORY) as i16;
}
