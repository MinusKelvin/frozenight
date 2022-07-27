use cozy_chess::{bitboard, BitBoard, Board, Color, Piece};

use crate::Eval;

const CHECKERBOARD: BitBoard = bitboard! {
    X . X . X . X .
    . X . X . X . X
    X . X . X . X .
    . X . X . X . X
    X . X . X . X .
    . X . X . X . X
    X . X . X . X .
    . X . X . X . X
};

const CORNERS: BitBoard = bitboard! {
    X . . . . . . X
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    . . . . . . . .
    X . . . . . . X
};

pub fn oracle(board: &Board) -> Option<Eval> {
    let bishops = board.pieces(Piece::Bishop);
    let knights = board.pieces(Piece::Knight);
    let kings = board.pieces(Piece::King);

    // only checking minor piece draws
    if board.occupied() != bishops | knights | kings {
        return None;
    }

    // bishops of same color draw
    if board.occupied().len() - 2 == bishops.len() {
        if bishops.is_subset(CHECKERBOARD) || bishops.is_subset(!CHECKERBOARD) {
            return Some(Eval::DRAW);
        }
    }

    match board.occupied().len() {
        0..=3 => Some(Eval::DRAW), // KvK, KBvK, KNvK
        4 => {
            let minors = bishops | knights;

            if (minors & board.colors(Color::White)).is_empty()
                || (minors & board.colors(Color::Black)).is_empty()
            {
                // Same color, only a draw if 2 knights and kings are off the edge
                if bishops.is_empty() && !(kings & BitBoard::EDGES).is_empty() {
                    Some(Eval::DRAW)
                } else {
                    None
                }
            } else {
                // Different colors, these are all draws so long as a king is not in the corner
                if (kings & CORNERS).is_empty() {
                    Some(Eval::DRAW)
                } else {
                    None
                }
            }
        }
        _ => None,
    }
}
