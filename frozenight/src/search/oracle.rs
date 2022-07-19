use cozy_chess::{Board, Piece};

use crate::Eval;

pub fn oracle(board: &Board) -> Option<Eval> {
    match board.occupied().popcnt() {
        0..=2 => Some(Eval::DRAW),
        3 => {
            if !(board.pieces(Piece::Knight) | board.pieces(Piece::Bishop)).is_empty() {
                Some(Eval::DRAW)
            } else {
                None
            }
        }
        _ => None,
    }
}
