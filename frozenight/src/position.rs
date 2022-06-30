use cozy_chess::{Board, Move};

use crate::nnue::{Nnue, NnueAccumulator};
use crate::Eval;
use crate::search::INVALID_MOVE;

#[derive(Clone)]
pub struct Position {
    pub board: Board,
    pub ply: u16,
    pub prev_move: Move,
    nnue: NnueAccumulator,
}

impl Position {
    pub fn from_root(board: Board, nn: &Nnue) -> Position {
        Position {
            nnue: NnueAccumulator::new(&board, nn),
            board,
            ply: 0,
            prev_move: INVALID_MOVE,
        }
    }

    pub fn play_move(&self, nn: &Nnue, mv: Move) -> Position {
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        Position {
            board,
            nnue: self.nnue.play_move(nn, &self.board, mv),
            ply: self.ply + 1,
            prev_move: mv,
        }
    }

    pub fn null_move(&self) -> Option<Position> {
        Some(Position {
            board: self.board.null_move()?,
            nnue: self.nnue,
            ply: self.ply + 1,
            prev_move: INVALID_MOVE,
        })
    }

    pub fn static_eval(&self, nn: &Nnue) -> Eval {
        self.nnue.calculate(nn, self.board.side_to_move())
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}
