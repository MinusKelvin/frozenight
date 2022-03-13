use cozy_chess::*;

pub fn move_is_quiet(mv: Move, board: &Board) -> bool {
    let color = board.side_to_move();
    let mut capture_squares = board.colors(!color);
    if let Some(ep) = board.en_passant() {
        capture_squares |= Square::new(ep, Rank::Third.relative_to(!color)).bitboard();
    }
    !capture_squares.has(mv.to) && mv.promotion.is_none()
}
