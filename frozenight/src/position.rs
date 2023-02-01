use std::cell::Cell;

use cozy_chess::{Board, Move};

use crate::nnue::NnueAccumulator;
use crate::Eval;
use crate::tt::TranspositionTable;

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
            eval: Cell::default(),
        }
    }

    pub fn play_move(&self, mv: Move, tt: &TranspositionTable) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        tt.prefetch(&board);
        Position {
            board,
            nnue: self.nnue.play_move(&self.board, mv),
            ply: self.ply + 1,
            eval: Cell::default(),
        }
    }

    pub fn null_move(&self, tt: &TranspositionTable) -> Option<Position> {
        self.board.null_move().map(|board| {
            tt.prefetch(&board);
            Position {
                board,
                nnue: self.nnue,
                ply: self.ply + 1,
                eval: Cell::default(),
            }
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

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
