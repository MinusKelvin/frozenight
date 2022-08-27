use cozy_chess::{bitboard, BitBoard, Board, Color, Piece};

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

pub fn draw_oracle(board: &Board) -> bool {
    let bishops = board.pieces(Piece::Bishop);
    let knights = board.pieces(Piece::Knight);
    let kings = board.pieces(Piece::King);

    // only checking minor piece draws
    if board.occupied() != bishops | knights | kings {
        return false;
    }

    // bishops of same color draw
    if board.occupied().len() - 2 == bishops.len() {
        if bishops.is_subset(CHECKERBOARD) || bishops.is_subset(!CHECKERBOARD) {
            return true;
        }
    }

    match board.occupied().len() {
        0..=3 => true, // KvK, KBvK, KNvK
        4 => {
            let minors = bishops | knights;

            if (minors & board.colors(Color::White)).is_empty()
                || (minors & board.colors(Color::Black)).is_empty()
            {
                // Same color, only a draw if 2 knights and kings are off the edge
                bishops.is_empty() && !(kings & BitBoard::EDGES).is_empty()
            } else {
                // Different colors, these are all draws so long as a king is not in the corner
                (kings & CORNERS).is_empty()
            }
        }
        _ => false,
    }
}
