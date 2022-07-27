use cozy_chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    Board, Color, Move, Piece, Square,
};

const VALUES: [i32; Piece::NUM] = [100, 300, 325, 500, 900, 0];

pub fn static_exchange_eval(board: &Board, capture: Move) -> i32 {
    let occupied = board.occupied() & !capture.from.bitboard();
    VALUES[board.piece_on(capture.to).unwrap() as usize]
        - see_impl(
            board,
            !board.side_to_move(),
            capture.to,
            board.piece_on(capture.from).unwrap(),
            occupied,
            get_bishop_moves(capture.to, occupied),
            get_rook_moves(capture.to, occupied),
        )
}

fn see_impl(
    board: &Board,
    stm: Color,
    sq: Square,
    to_capture: Piece,
    occupied: BitBoard,
    bishop_attacks: BitBoard,
    rook_attacks: BitBoard,
) -> i32 {
    let movable = board.colors(stm) & occupied;
    let gain = VALUES[to_capture as usize];

    let eval = |from: Square, piece: Piece| {
        if to_capture == Piece::King {
            return 999999;
        }
        let occupied = occupied & !from.bitboard();
        let bishop_attacks = match bishop_attacks.has(from) {
            true => get_bishop_moves(sq, occupied),
            false => bishop_attacks,
        };
        let rook_attacks = match rook_attacks.has(from) {
            true => get_rook_moves(sq, occupied),
            false => rook_attacks,
        };
        0i32.max(
            gain - see_impl(
                board,
                !stm,
                sq,
                piece,
                occupied,
                bishop_attacks,
                rook_attacks,
            ),
        )
    };

    if let Some(sq) =
        (get_pawn_attacks(sq, !stm) & board.pieces(Piece::Pawn) & movable).next_square()
    {
        return eval(sq, Piece::Pawn);
    }

    let knight_attacks = get_knight_moves(sq);
    if let Some(sq) = (knight_attacks & board.pieces(Piece::Knight) & movable).next_square() {
        return eval(sq, Piece::Knight);
    }

    if let Some(sq) = (bishop_attacks & board.pieces(Piece::Bishop) & movable).next_square() {
        return eval(sq, Piece::Bishop);
    }

    if let Some(sq) = (rook_attacks & board.pieces(Piece::Rook) & movable).next_square() {
        return eval(sq, Piece::Rook);
    }

    let queen_attacks = bishop_attacks | rook_attacks;
    if let Some(sq) = (queen_attacks & board.pieces(Piece::Queen) & movable).next_square() {
        return eval(sq, Piece::Queen);
    }

    if get_king_moves(sq).has(board.king(stm)) {
        return eval(sq, Piece::King);
    }

    0
}
