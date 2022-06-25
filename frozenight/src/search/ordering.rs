use cozy_chess::{Board, Color, Move, Piece, Square};

use crate::position::Position;

use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];

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
        let killer = *self.killer(position.ply);

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

                match position.board.piece_on(mv.to) {
                    Some(victim) => {
                        let attacker = PIECE_ORDINALS[mvs.piece as usize];
                        let victim = PIECE_ORDINALS[victim as usize] * 4;
                        captures.push((mv, victim - attacker));
                    }
                    _ if mv == killer => {
                        // Killer is legal; give it the same rank as PxP
                        captures.push((mv, 0));
                    }
                    _ => {
                        quiets.push((mv, mvs.piece));
                    }
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

pub struct HistoryTable {
    piece_to_sq: [[[i16; Square::NUM]; Piece::NUM]; Color::NUM],
    from_sq_to_sq: [[[i16; Square::NUM]; Square::NUM]; Color::NUM],
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            piece_to_sq: [[[0; Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[0; Square::NUM]; Square::NUM]; Color::NUM],
        }
    }

    pub fn decay(&mut self) {
        for v in self.piece_to_sq.iter_mut().flatten().flatten() {
            *v /= 16;
        }
        for v in self.from_sq_to_sq.iter_mut().flatten().flatten() {
            *v /= 4;
        }
    }

    fn values(&mut self, board: &Board, mv: Move) -> (&mut i16, &mut i16) {
        let stm = board.side_to_move();
        let piece = board.piece_on(mv.from).unwrap();
        (&mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize], &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize])
    }

    pub fn caused_cutoff(&mut self, board: &Board, mv: Move) {
        let (piece_to, from_to) = self.values(board, mv);
        *piece_to = (*piece_to + 8).clamp(-1024, 1024);
        *from_to = (*from_to + 8).clamp(-1024, 1024);
    }

    pub fn did_not_cause_cutoff(&mut self, board: &Board, mv: Move) {
        let (piece_to, from_to) = self.values(board, mv);
        *piece_to = (*piece_to - 1).clamp(-1024, 1024);
        *from_to = (*from_to - 1).clamp(-1024, 1024);
    }

    fn rank(&self, piece: Piece, mv: Move, stm: Color) -> i32 {
        let piece_to = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let from_to = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        piece_to as i32 + from_to as i32
    }
}
