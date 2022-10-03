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
        self.nnue.calculate(self.board.side_to_move())
    }

    pub fn is_capture(&self, mv: Move) -> bool {
        self.board.colors(!self.board.side_to_move()).has(mv.to)
    }
}

pub trait BoardExt {
    fn halfmove_hash(&self) -> u64;
}

const HALFMOVE_KEYS: [u64; 25] = {
    let mut result = [0; 25];
    let mut i = 0;
    let mut state = 0xeb84926524cc094cef4d44c49559c4feu128 | 1;
    while i < 25 {
        state = state.wrapping_mul(0x2360ED051FC65DA44385DF649FCCF645);
        let rot = (state >> 122) as u32;
        let xsl = (state >> 64) as u64 ^ state as u64;
        result[i] = xsl.rotate_right(rot);
        i += 1;
    }
    result
};

impl BoardExt for Board {
    fn halfmove_hash(&self) -> u64 {
        let hash = self.hash();
        if self.halfmove_clock() <= 75 {
            hash
        } else {
            hash ^ HALFMOVE_KEYS[self.halfmove_clock().min(100) as usize - 76]
        }
    }
}
