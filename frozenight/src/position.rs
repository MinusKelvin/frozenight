use std::cell::Cell;

use cozy_chess::{Board, Move};

use crate::nnue::NnueAccumulator;
use crate::Eval;

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub ply: u16,
    nnue: NnueAccumulator,
    eval: Cell<Option<Eval>>,
}

impl Position {
    pub fn from_root(board: Board) -> Position {
        Position {
            nnue: NnueAccumulator::new(&board),
            board,
            ply: 0,
            eval: Cell::new(None),
        }
    }

    pub fn play_move(&self, mv: Move) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        Position {
            board,
            nnue: self.nnue.play_move(&self.board, mv),
            ply: self.ply + 1,
            eval: Cell::new(None),
        }
    }

    pub fn null_move(&self) -> Option<Position> {
        Some(Position {
            board: self.board.null_move()?,
            nnue: self.nnue,
            ply: self.ply + 1,
            eval: Cell::new(None),
        })
    }

    pub fn static_eval(&self) -> Eval {
        self.eval.get().unwrap_or_else(|| {
            let v = self.nnue.calculate(self.board.side_to_move());
            self.eval.set(Some(v));
            v
        })
    }

    pub fn cached_static_eval(&self) -> Option<Eval> {
        self.eval.get()
    }

    pub fn restore_static_eval(&self, to: Eval) {
        self.eval.set(Some(to));
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
