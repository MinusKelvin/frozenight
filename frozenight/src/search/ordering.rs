use cozy_chess::{Color, Move, Piece, Square};

use crate::position::Position;

use super::see::static_exchange_eval;
use super::{Searcher, INVALID_MOVE};

pub struct MovePicker<'a> {
    pos: &'a Position,
    hashmv: Option<Move>,
    moves: Vec<(Move, MoveScore)>,
    next: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoveScore {
    Quiet,
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

    pub fn pick_move(&mut self) -> Option<(usize, Move)> {
        let i = self.next;
        match self.hashmv {
            Some(mv) if i == 0 => {
                self.next += 1;
                return Some((i, mv));
            }
            _ if self.moves.is_empty() => {
                if let Some(mv) = self.hashmv {
                    self.moves.push((mv, MoveScore::Hash));
                }
                let capture_targets = self.pos.board.colors(!self.pos.board.side_to_move());
                self.pos.board.generate_moves(|mvs| {
                    for mv in mvs {
                        let score = match () {
                            _ if Some(mv) == self.hashmv => continue,
                            _ if capture_targets.has(mv.to) => MoveScore::Capture(
                                self.pos.board.piece_on(mv.to).unwrap() as i16 * 8
                                    - mvs.piece as i16,
                            ),
                            _ => MoveScore::Quiet,
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
        Some((i, mv))
    }

    pub fn yielded(&mut self) -> impl Iterator<Item = Move> + '_ {
        self.moves[..self.next].iter().map(|&(mv, _)| mv)
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
