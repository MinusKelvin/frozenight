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

        position.board.generate_moves(|mvs| {
            let piece = mvs.piece;
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
                    captures.push((mv, static_exchange_eval(&position.board, mv)));
                } else if mv == killer {
                    // Killer is legal; put it before neutral captures
                    captures.push((mv, 1));
                } else {
                    quiets.push((mv, piece));
                }
            }
            false
        });

        // Iterate captures
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

        // Iterate quiets
        while !quiets.is_empty() {
            let mut index = 0;
            let mut rank =
                self.state
                    .history
                    .rank(quiets[0].1, quiets[0].0, position.board.side_to_move());
            for i in 1..quiets.len() {
                let r = self.state.history.rank(
                    quiets[i].1,
                    quiets[i].0,
                    position.board.side_to_move(),
                );
                if r > rank {
                    index = i;
                    rank = r;
                }
            }

            if search(self, quiets.swap_remove(index).0)? {
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
    piece_to_sq: [[[(u32, u32); Square::NUM]; Piece::NUM]; Color::NUM],
    from_sq_to_sq: [[[(u32, u32); Square::NUM]; Square::NUM]; Color::NUM],
    killers: [Move; 256],
}

impl OrderingState {
    pub fn new() -> Self {
        OrderingState {
            piece_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Square::NUM]; Color::NUM],
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

    pub fn caused_cutoff(&mut self, pos: &Position, mv: Move) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if !capture {
            let (piece_to, total) =
                &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
            let diff = 2_000_000_000 - *piece_to;
            *total += 1;
            *piece_to += diff / *total;

            let (from_to, total) =
                &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            let diff = 2_000_000_000 - *from_to;
            *total += 1;
            *from_to += diff / *total;

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
            *piece_to -= *piece_to / *total;

            let (from_to, total) =
                &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            *total += 1;
            *from_to -= *from_to / *total;
        }
    }

    fn rank(&self, piece: Piece, mv: Move, stm: Color) -> u32 {
        let (piece_to, _) = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let (from_to, _) = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        piece_to + from_to
    }

    fn killer(&self, ply: u16) -> Move {
        self.killers
            .get(ply as usize)
            .copied()
            .unwrap_or(INVALID_MOVE)
    }
}
