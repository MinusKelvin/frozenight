use std::cell::Cell;

use cozy_chess::{Board, Move};

use crate::nnue::NnueAccumulator;
use crate::Eval;

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub ply: u16,
    nnue: NnueAccumulator,
    static_eval: Cell<Option<Eval>>,
}

impl Position {
    pub fn from_root(board: Board) -> Position {
        Position {
            nnue: NnueAccumulator::new(&board),
            board,
            ply: 0,
            static_eval: Cell::new(None),
        }
    }

    pub fn play_move(&self, mv: Move) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        Position {
            board,
            nnue: self.nnue.play_move(&self.board, mv),
            ply: self.ply + 1,
            static_eval: Cell::new(None),
        }
    }

    pub fn null_move(&self) -> Option<Position> {
        Some(Position {
            board: self.board.null_move()?,
            nnue: self.nnue,
            ply: self.ply + 1,
            static_eval: Cell::new(None),
        })
    }

    pub fn static_eval(&self) -> Eval {
        match self.static_eval.get() {
            Some(v) => v,
            None => {
                let v = self.nnue.calculate(self.board.side_to_move());
                self.static_eval.set(Some(v));
                v
            }
        }
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
