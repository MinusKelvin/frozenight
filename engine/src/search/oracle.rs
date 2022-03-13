use cozy_chess::*;

use crate::eval::*;

// CITE: Oracle. This is more specifically an interior node recognizer.
// https://www.chessprogramming.org/Oracle
// https://www.chessprogramming.org/Interior_Node_Recognizer
pub fn oracle(board: &Board) -> Option<Eval> {
    let all_pieces = board.occupied();
    let white_pieces = board.colors(Color::White);
    let bishops = board.pieces(Piece::Bishop);
    let knights = board.pieces(Piece::Knight);
    let kings = board.pieces(Piece::King);

    match all_pieces.popcnt() {
        2 => Some(Eval::DRAW),
        3 => {
            //KBvK and KNvK is always a draw
            if !(bishops | knights).is_empty() {
                Some(Eval::DRAW)
            } else {
                None
            }
        }
        4 => {
            const DARK_SQUARES: BitBoard = bitboard! {
                . X . X . X . X
                X . X . X . X .
                . X . X . X . X
                X . X . X . X .
                . X . X . X . X
                X . X . X . X .
                . X . X . X . X
                X . X . X . X .
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
            let one_piece_each = white_pieces.popcnt() == 2;

            //KNvKN KNNvk. Always a draw except for a few positions that are mate in one.
            //All of those positions have a king on an edge and are incredibly rare,
            //so we just do a quick check for edge kings before returning a draw.
            if knights.popcnt() == 2 && (kings & BitBoard::EDGES).is_empty() {
                return Some(Eval::DRAW);
            }
            if bishops.popcnt() == 2 {
                if (bishops & DARK_SQUARES).popcnt() != 1 {
                    //Both bishops are on the same color square
                    return Some(Eval::DRAW);
                }
                if one_piece_each && (kings & CORNERS).is_empty() {
                    //Opposite color bishops. Check the corners
                    //since there's technically one checkmate.
                    return Some(Eval::DRAW);
                }
            }
            if knights.popcnt() == 1 && bishops.popcnt() == 1 {
                if one_piece_each && (kings & CORNERS).is_empty() {
                    //Check the corners since there's technically one checkmate.
                    return Some(Eval::DRAW);
                }
            }
            None
        }
        _ => None
    }
}
