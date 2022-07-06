use cozy_chess::{Color, Move, Piece, Square};

use crate::position::Position;

use super::{Searcher, INVALID_MOVE};

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
        let killer = self.state.history.killer(position.ply);
        let counter = self
            .state
            .history
            .counter_move(position.prev, position.board.side_to_move());

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
            let mut rank = self.state.history.rank(
                quiets[0].1,
                quiets[0].0,
                position.board.side_to_move(),
                counter,
            );
            for i in 1..quiets.len() {
                let r = self.state.history.rank(
                    quiets[i].1,
                    quiets[i].0,
                    position.board.side_to_move(),
                    counter,
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
    counters: [[[Move; Square::NUM]; Square::NUM]; Color::NUM],
    killers: [Move; 256],
}

impl OrderingState {
    pub fn new() -> Self {
        OrderingState {
            piece_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Square::NUM]; Color::NUM],
            counters: [[[INVALID_MOVE; Square::NUM]; Square::NUM]; Color::NUM],
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
            if pos.prev != INVALID_MOVE {
                self.counters[stm as usize][pos.prev.from as usize][pos.prev.to as usize] = mv;
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

    fn rank(&self, piece: Piece, mv: Move, stm: Color, counter: Move) -> u32 {
        let (piece_to, _) = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let (from_to, _) = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        let counter_score = match mv == counter {
            true => 30_000_000,
            false => 0,
        };
        piece_to + from_to + counter_score
    }

    fn counter_move(&self, mv: Move, stm: Color) -> Move {
        self.counters[stm as usize][mv.from as usize][mv.to as usize]
    }

    fn killer(&self, ply: u16) -> Move {
        self.killers
            .get(ply as usize)
            .copied()
            .unwrap_or(INVALID_MOVE)
    }
}
