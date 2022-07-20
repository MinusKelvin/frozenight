use cozy_chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves, BitBoard,
    Board, Color, Move, Piece, Rank, Square,
};

const VALUES: [i32; Piece::NUM] = [100, 300, 325, 500, 900, 0];

pub fn static_exchange_eval(board: &Board, capture: Move) -> i32 {
    let capture_gain = board.piece_on(capture.to).map_or(0, |p| VALUES[p as usize]);
    let promotion_gain = capture
        .promotion
        .map_or(0, |p| VALUES[p as usize] - VALUES[Piece::Pawn as usize]);
    capture_gain + promotion_gain
        - see_impl(
            board,
            !board.side_to_move(),
            capture.to,
            capture
                .promotion
                .unwrap_or_else(|| board.piece_on(capture.from).unwrap()),
            capture.from.bitboard(),
        )
}

fn see_impl(board: &Board, stm: Color, sq: Square, to_capture: Piece, moved: BitBoard) -> i32 {
    let movable = board.colors(stm) & !moved;
    let gain = VALUES[to_capture as usize];

    let is_promo_sq = sq.rank() == Rank::First.relative_to(stm);

    let eval = |from: Square, mut piece: Piece| {
        if to_capture == Piece::King {
            return 999999;
        }
        let mut gain = gain;
        if piece == Piece::Pawn && is_promo_sq {
            gain += VALUES[Piece::Queen as usize] - VALUES[Piece::Pawn as usize];
            piece = Piece::Queen;
        }
        0i32.max(gain - see_impl(board, !stm, sq, piece, moved | from.bitboard()))
    };

    if !is_promo_sq {
        if let Some(sq) =
            (get_pawn_attacks(sq, !stm) & board.pieces(Piece::Pawn) & movable).next_square()
        {
            return eval(sq, Piece::Pawn);
        }
    }

    let knight_attacks = get_knight_moves(sq);
    if let Some(sq) = (knight_attacks & board.pieces(Piece::Knight) & movable).next_square() {
        return eval(sq, Piece::Knight);
    }

    let bishop_attacks = get_bishop_moves(sq, board.occupied() & !moved);
    if let Some(sq) = (bishop_attacks & board.pieces(Piece::Bishop) & movable).next_square() {
        return eval(sq, Piece::Bishop);
    }

    let rook_attacks = get_rook_moves(sq, board.occupied() & !moved);
    if let Some(sq) = (rook_attacks & board.pieces(Piece::Rook) & movable).next_square() {
        return eval(sq, Piece::Rook);
    }

    if is_promo_sq {
        if let Some(sq) =
            (get_pawn_attacks(sq, !stm) & board.pieces(Piece::Pawn) & movable).next_square()
        {
            return eval(sq, Piece::Pawn);
        }
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
