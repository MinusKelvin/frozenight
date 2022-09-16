use std::sync::atomic::Ordering;

use cozy_chess::{Color, Move, Piece, Square};

use crate::position::Position;

use super::see::static_exchange_eval;
use super::{Searcher, INVALID_MOVE};

pub const CONTINUE: bool = false;
pub const BREAK: bool = true;

impl Searcher<'_> {
    pub fn visit_moves(
        &mut self,
        position: &Position,
        hashmove: Option<Move>,
        mut search: impl FnMut(&mut Searcher, Move) -> Option<bool>,
    ) -> Option<()> {
        // Hashmove
        if let Some(mv) = hashmove {
            if search(self, mv)? {
                return Some(());
            }
        }

        // Generate moves.
        let mut captures = Vec::with_capacity(16);
        let mut quiets = Vec::with_capacity(64);
        let mut underpromotions = vec![];
        let killer = self.state.history.killer(position.ply);

        let log_nodes = (self.stats.nodes.load(Ordering::Relaxed) as f32).ln_1p();
        position.board.generate_moves(|mvs| {
            for mv in mvs {
                if Some(mv) == hashmove {
                    continue;
                }
                if matches!(
                    mv.promotion,
                    Some(Piece::Knight | Piece::Bishop | Piece::Rook)
                ) {
                    underpromotions.push(mv);
                    continue;
                }

                if position.is_capture(mv) {
                    let victim = position.board.piece_on(mv.to).unwrap();
                    let mvv_lva = 8 * victim as i32 - mvs.piece as i32 + 8;
                    captures.push((mv, static_exchange_eval(&position.board, mv) + mvv_lva));
                } else if mv == killer {
                    // Killer is legal; order it after neutral captures
                    captures.push((mv, 0));
                } else {
                    quiets.push((mv, self.state.history.rank(
                        mvs.piece,
                        mv,
                        position.board.side_to_move(),
                        log_nodes,
                    )));
                }
            }
            false
        });

        // Iterate winning & netrual captures
        while !captures.is_empty() {
            let mut index = 0;
            for i in 1..captures.len() {
                if captures[i].1 > captures[index].1 {
                    index = i;
                }
            }

            if captures[index].1 < 0 {
                break;
            }
            if search(self, captures.swap_remove(index).0)? {
                return Some(());
            }
        }

        // Iterate quiets
        while !quiets.is_empty() {
            let mut index = 0;
            let mut rank = quiets[0].1;
            for i in 1..quiets.len() {
                if quiets[i].1 > rank {
                    index = i;
                    rank = quiets[i].1;
                }
            }

            if search(self, quiets.swap_remove(index).0)? {
                return Some(());
            }
        }

        // Iterate losing captures
        while !captures.is_empty() {
            let mut index = 0;
            for i in 1..captures.len() {
                if captures[i].1 > captures[index].1 {
                    index = i;
                }
            }

            if search(self, captures.swap_remove(index).0)? {
                return Some(());
            }
        }

        // Iterate underpromotions
        while let Some(mv) = underpromotions.pop() {
            if search(self, mv)? {
                return Some(());
            }
        }

        Some(())
    }
}

pub struct OrderingState {
    piece_to_sq: [[[(f32, u32); Square::NUM]; Piece::NUM]; Color::NUM],
    from_sq_to_sq: [[[(f32, u32); Square::NUM]; Square::NUM]; Color::NUM],
    killers: [Move; 256],
}

impl OrderingState {
    pub fn new() -> Self {
        OrderingState {
            piece_to_sq: [[[(0.0, 0); Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[(0.0, 0); Square::NUM]; Square::NUM]; Color::NUM],
            killers: [INVALID_MOVE; 256],
        }
    }

    pub fn decay(&mut self) {
        for (_, total) in self.piece_to_sq.iter_mut().flatten().flatten() {
            *total /= 64;
        }
        for (_, total) in self.from_sq_to_sq.iter_mut().flatten().flatten() {
            *total /= 16;
        }
    }

    pub fn caused_cutoff(&mut self, pos: &Position, mv: Move, depth: i16) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if !capture {
            let (piece_to, total) =
                &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
            let diff = depth as f32 - *piece_to;
            *total += 1;
            *piece_to += diff / *total as f32;

            let (from_to, total) =
                &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            let diff = depth as f32 - *from_to;
            *total += 1;
            *from_to += diff / *total as f32;

            if let Some(killer) = self.killers.get_mut(pos.ply as usize) {
                *killer = mv;
            }
        }
    }

    pub fn did_not_cause_cutoff(&mut self, pos: &Position, mv: Move) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if !capture {
            let (piece_to, total) =
                &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
            *total += 1;
            *piece_to -= *piece_to / *total as f32;

            let (from_to, total) =
                &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            *total += 1;
            *from_to -= *from_to / *total as f32;
        }
    }

    fn rank(&self, piece: Piece, mv: Move, stm: Color, log_t: f32) -> f32 {
        let (piece_to, n) = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let variance = log_t / n as f32;
        let (from_to, n) = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        let variance = variance + log_t / n as f32;
        (piece_to + from_to) + 0.2 * variance.sqrt()
    }

    fn killer(&self, ply: u16) -> Move {
        self.killers
            .get(ply as usize)
            .copied()
            .unwrap_or(INVALID_MOVE)
    }
}
