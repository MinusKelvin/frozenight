use cozy_chess::{Board, Move};

use crate::nnue::NnueAccumulator;
use crate::Eval;

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub ply: u16,
    nnue: NnueAccumulator,
}

impl Position {
    pub fn from_root(board: Board) -> Position {
        Position {
            nnue: NnueAccumulator::new(&board),
            board,
            ply: 0,
        }
    }

    pub fn play_move(&self, mv: Move) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        Position {
            board,
            nnue: self.nnue.play_move(&self.board, mv),
            ply: self.ply + 1,
        }
    }

    pub fn null_move(&self) -> Option<Position> {
        Some(Position {
            board: self.board.null_move()?,
            nnue: self.nnue,
            ply: self.ply + 1,
        })
    }

    pub fn static_eval(&self) -> Eval {
        let raw = self.nnue.calculate(self.board.side_to_move());
        let factor = 32 - self.board.halfmove_clock().saturating_sub(70) as i32;
        Eval::new((raw as i32 * factor / 32) as i16)
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
