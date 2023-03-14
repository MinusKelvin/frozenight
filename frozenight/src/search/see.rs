use cozy_chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    Board, Move, Piece, Square,
};

const VALUES: [i16; Piece::NUM] = [500, 1500, 1625, 2500, 4500, 30000];

pub fn static_exchange_eval(board: &Board, capture: Move) -> i16 {
    let occupied = board.occupied() & !capture.from.bitboard();
    VALUES[board.piece_on(capture.to).unwrap() as usize]
        - see_impl(
            board,
            capture.to,
            board.piece_on(capture.from).unwrap(),
            occupied,
        )
}

fn see_impl(board: &Board, sq: Square, mut piece: Piece, mut occupied: BitBoard) -> i16 {
    let mut stm = board.side_to_move();
    let mut bishop_attacks = get_bishop_moves(sq, occupied);
    let mut rook_attacks = get_rook_moves(sq, occupied);

    let mut gains = [0; 32];
    let mut index = 0;
    for i in 0..32 {
        index = i;

        stm = !stm;
        let movable = board.colors(stm) & occupied;

        if let Some(from) =
            (get_pawn_attacks(sq, !stm) & board.pieces(Piece::Pawn) & movable).next_square()
        {
            // Pawn
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::Pawn;
            occupied &= !from.bitboard();
            bishop_attacks = get_bishop_moves(sq, occupied);
        } else if let Some(from) =
            (get_knight_moves(sq) & board.pieces(Piece::Knight) & movable).next_square()
        {
            // Knight
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::Knight;
            occupied &= !from.bitboard();
        } else if let Some(from) =
            (bishop_attacks & board.pieces(Piece::Bishop) & movable).next_square()
        {
            // Bishop
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::Bishop;
            occupied &= !from.bitboard();
            bishop_attacks = get_bishop_moves(sq, occupied);
        } else if let Some(from) =
            (rook_attacks & board.pieces(Piece::Rook) & movable).next_square()
        {
            // Rook
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::Rook;
            occupied &= !from.bitboard();
            rook_attacks = get_rook_moves(sq, occupied);
        } else if let Some(from) =
            ((rook_attacks | bishop_attacks) & board.pieces(Piece::Queen) & movable).next_square()
        {
            // Queen
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::Queen;
            occupied &= !from.bitboard();
            if bishop_attacks.has(from) {
                bishop_attacks = get_bishop_moves(sq, occupied);
            } else {
                rook_attacks = get_rook_moves(sq, occupied);
            }
        } else if get_king_moves(sq).has(board.king(stm)) {
            // King
            gains[i] = VALUES[piece as usize];
            if piece == Piece::King {
                break;
            }
            piece = Piece::King;
            occupied &= !board.king(stm).bitboard();
        } else {
            // No more captures possible
            break;
        }
    }

    let mut value = 0;
    for i in (0..=index).rev() {
        value = 0.max(gains[i] - value);
    }

    value
}
