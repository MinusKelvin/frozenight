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
        let mut moves = Vec::with_capacity(64);
        let mut underpromotions = vec![];

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

                moves.push((mv, self.state.history.score(position, mv, mvs.piece).0));
            }
            false
        });

        // Iterate scored moves
        while !moves.is_empty() {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }

            if search(self, moves.swap_remove(index).0)? {
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
    capture_piece_to_sq: ColorTable<PieceTable<SquareTable<HistoryCounter>>>,
    piece_to_sq: ColorTable<PieceTable<SquareTable<HistoryCounter>>>,
    from_sq_to_sq: ColorTable<SquareTable<SquareTable<HistoryCounter>>>,
    killers: [Move; 256],
}

impl OrderingState {
    pub fn new() -> Self {
        OrderingState {
            capture_piece_to_sq: Default::default(),
            piece_to_sq: Default::default(),
            from_sq_to_sq: Default::default(),
            killers: [INVALID_MOVE; 256],
        }
    }

    pub fn decay(&mut self) {
        for counter in (&mut self.capture_piece_to_sq)
            .into_iter()
            .flatten()
            .flatten()
        {
            counter.decay(64);
        }
        for counter in (&mut self.piece_to_sq).into_iter().flatten().flatten() {
            counter.decay(64);
        }
        for counter in (&mut self.from_sq_to_sq).into_iter().flatten().flatten() {
            counter.decay(16);
        }
    }

    pub fn caused_cutoff(&mut self, pos: &Position, mv: Move, depth: i16) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if capture {
            self.capture_piece_to_sq[stm][piece][mv.to].increment(depth);
        } else {
            self.piece_to_sq[stm][piece][mv.to].increment(depth);
            self.from_sq_to_sq[stm][mv.from][mv.to].increment(depth);

            if let Some(killer) = self.killers.get_mut(pos.ply as usize) {
                *killer = mv;
            }
        }
    }

    pub fn did_not_cause_cutoff(&mut self, pos: &Position, mv: Move) {
        let stm = pos.board.side_to_move();
        let piece = pos.board.piece_on(mv.from).unwrap();
        let capture = pos.is_capture(mv);

        if capture {
            self.capture_piece_to_sq[stm][piece][mv.to].decrement();
        } else {
            self.piece_to_sq[stm][piece][mv.to].decrement();
            self.from_sq_to_sq[stm][mv.from][mv.to].decrement();
        }
    }

    pub fn score(&self, position: &Position, mv: Move, piece: Piece) -> (i32, i32) {
        let stm = position.board.side_to_move();
        let mut move_score = 0;
        let mut see = 0;

        if position.is_capture(mv) {
            let victim = position.board.piece_on(mv.to).unwrap();
            let mvv_lva = 8 * victim as i32 - piece as i32 + 8;
            see = static_exchange_eval(&position.board, mv);
            move_score += (see + mvv_lva) * 10_000;
            let piece_to = self.capture_piece_to_sq[stm][piece][mv.to].value;
            move_score += piece_to;
        } else {
            let piece_to = self.piece_to_sq[stm][piece][mv.to].value;
            let from_to = self.from_sq_to_sq[stm][mv.from][mv.to].value;
            move_score += (piece_to + from_to) / 2;
        }
        if mv == self.killer(position.ply) {
            move_score += 1_000_000;
        }

        (move_score, see)
    }

    fn killer(&self, ply: u16) -> Move {
        self.killers
            .get(ply as usize)
            .copied()
            .unwrap_or(INVALID_MOVE)
    }
}

#[derive(Copy, Clone, Debug)]
struct HistoryCounter {
    value: i32,
    count: i32,
}

impl HistoryCounter {
    #[inline(always)]
    fn increment(&mut self, depth: i16) {
        self.count += 1;
        let diff = (depth as i32 * 1_000_000 - self.value).max(0);
        self.value += diff / self.count;
    }

    #[inline(always)]
    fn decrement(&mut self) {
        self.count += 1;
        self.value -= self.value / self.count;
    }

    #[inline(always)]
    fn decay(&mut self, factor: i32) {
        self.count = 1.max(self.count / factor);
    }
}

impl Default for HistoryCounter {
    fn default() -> Self {
        Self {
            value: 1_000_000,
            count: 1,
        }
    }
}

macro_rules! tables {
    ($($table:ident: $enum:ty;)*) => {
        $(
            #[derive(Copy, Clone, Debug)]
            struct $table<T>([T; <$enum>::NUM]);

            impl<T, I: Into<$enum>> std::ops::Index<I> for $table<T> {
                type Output = T;

                #[inline(always)]
                fn index(&self, index: I) -> &T {
                    &self.0[index.into() as usize]
                }
            }

            impl<T, I: Into<$enum>> std::ops::IndexMut<I> for $table<T> {
                #[inline(always)]
                fn index_mut(&mut self, index: I) -> &mut T {
                    &mut self.0[index.into() as usize]
                }
            }

            impl<T: Default> Default for $table<T> {
                fn default() -> Self {
                    Self([(); <$enum>::NUM].map(|_| Default::default()))
                }
            }

            impl<'a, T> IntoIterator for &'a mut $table<T> {
                type Item = &'a mut T;
                type IntoIter = std::slice::IterMut<'a, T>;

                #[inline(always)]
                fn into_iter(self) -> Self::IntoIter {
                    self.0.iter_mut()
                }
            }
        )*
    };
}

tables! {
    ColorTable: Color;
    PieceTable: Piece;
    SquareTable: Square;
}
