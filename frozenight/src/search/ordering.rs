use cozy_chess::{Color, Move, Piece, Square};

use crate::position::Position;

use super::see::static_exchange_eval;
use super::{Searcher, INVALID_MOVE};

pub struct MovePicker<'a> {
    pos: &'a Position,
    hashmv: Option<Move>,
    moves: Vec<Move>,
    next: usize,
}

impl<'a> MovePicker<'a> {
    pub fn new(pos: &'a Position, hashmv: Option<Move>) -> Self {
        let mut moves = Vec::with_capacity(64);

        pos.board.generate_moves(|mvs| {
            moves.extend(mvs);
            false
        });

        MovePicker {
            pos,
            hashmv,
            moves,
            next: 0,
        }
    }

    pub fn pick_move(&mut self) -> Option<(usize, Move)> {
        let i = self.next;
        let &mv = self.moves.get(i)?;
        self.next += 1;
        Some((i, mv))
    }

    pub fn yielded(&mut self) -> &[Move] {
        &self.moves[..self.next]
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
