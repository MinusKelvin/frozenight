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
                    // Killer is legal; give it the same rank as neutral captures
                    captures.push((mv, 0));
                } else {
                    quiets.push((mv, piece));
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
    piece_to_sq: [[[i16; Square::NUM]; Piece::NUM]; Color::NUM],
    from_sq_to_sq: [[[i16; Square::NUM]; Square::NUM]; Color::NUM],
    killers: [Move; 256],
}

impl OrderingState {
    pub fn new() -> Self {
        OrderingState {
            piece_to_sq: [[[0; Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[0; Square::NUM]; Square::NUM]; Color::NUM],
            killers: [INVALID_MOVE; 256],
        }
    }

    pub fn decay(&mut self) {
        for hh_value in self.piece_to_sq.iter_mut().flatten().flatten() {
            *hh_value /= 4;
        }
        for hh_value in self.from_sq_to_sq.iter_mut().flatten().flatten() {
            *hh_value /= 4;
        }
    }

    pub fn caused_cutoff(&mut self, pos: &Position, mv: Move, _depth: i16) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if !capture {
            let piece_to = &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
            *piece_to = 1024.min(*piece_to + 1);

            let from_to = &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            *from_to = 1024.min(*from_to + 1);

            if let Some(killer) = self.killers.get_mut(pos.ply as usize) {
                *killer = mv;
            }
        }
    }

    pub fn did_not_cause_cutoff(&mut self, pos: &Position, mv: Move, depth: i16) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if !capture {
            let piece_to = &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
            *piece_to = (-1024).max(*piece_to - depth);

            let from_to = &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
            *from_to = (-1024).max(*from_to - depth);
        }
    }

    fn rank(&self, piece: Piece, mv: Move, stm: Color) -> i16 {
        let piece_to = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let from_to = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        piece_to + from_to
    }

    fn killer(&self, ply: u16) -> Move {
        self.killers
            .get(ply as usize)
            .copied()
            .unwrap_or(INVALID_MOVE)
    }
}
