use std::cell::Cell;

use cozy_chess::{Board, Move};

use crate::nnue::NnueAccumulator;
use crate::Eval;

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub ply: u16,
    nnue: NnueAccumulator,
    prev_evals: [Eval; 2],
    eval: Cell<Option<Eval>>,
}

impl Position {
    pub fn from_root(board: Board) -> Position {
        Position {
            nnue: NnueAccumulator::new(&board),
            board,
            ply: 0,
            prev_evals: [Eval::DRAW; 2],
            eval: Cell::new(None),
        }
    }

    pub fn play_move(&self, mv: Move) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        let prev_eval = self.static_eval();
        Position {
            board,
            nnue: self.nnue.play_move(&self.board, mv),
            ply: self.ply + 1,
            prev_evals: [prev_eval, self.prev_evals[0]],
            eval: Cell::new(None),
        }
    }

    pub fn null_move(&self) -> Option<Position> {
        let prev_eval = self.static_eval();
        Some(Position {
            board: self.board.null_move()?,
            nnue: self.nnue,
            ply: self.ply + 1,
            prev_evals: [prev_eval, self.prev_evals[0]],
            eval: Cell::new(None),
        })
    }

    pub fn static_eval(&self) -> Eval {
        match self.eval.get() {
            Some(v) => v,
            None => {
                let v = self.nnue.calculate(self.board.side_to_move());
                self.eval.set(Some(v));
                v
            }
        }
    }

    pub fn improvement(&self) -> i16 {
        self.static_eval().raw() - self.prev_evals[1].raw()
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
