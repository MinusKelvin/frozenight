use cozy_chess::*;
use arrayvec::ArrayVec;

use crate::eval::*;

// CITE: Static exchange evaluation.
// https://www.chessprogramming.org/Static_Exchange_Evaluation
pub fn static_exchange_evaluation(board: &Board, capture: Move) -> Eval {
    fn get_both_pawn_attacks(sq: Square) -> BitBoard {
        get_pawn_attacks(sq, Color::White) | get_pawn_attacks(sq, Color::Black)
    }

    macro_rules! pieces {
        ($($piece:ident)|+) => {
            ($(board.pieces(Piece::$piece))|*)
        }
    }

    let sq = capture.to;
    let mut attacker_sq = capture.from;
    let mut victim = board.piece_on(sq).unwrap();
    let mut attacker = board.piece_on(attacker_sq).unwrap();
    let mut color = board.side_to_move();
    let mut blockers = board.occupied();
    let mut attackers =
        get_king_moves(sq)                   & pieces!(King)           |
        get_knight_moves(sq)                 & pieces!(Knight)         |
        get_rook_moves(sq, blockers)         & pieces!(Rook | Queen)   |
        get_bishop_moves(sq, blockers)       & pieces!(Bishop | Queen) |
        get_both_pawn_attacks(sq) & blockers & pieces!(Pawn);

    //32 pieces max on a legal chess board.
    let mut captures = ArrayVec::<Eval, 32>::new();
    'exchange: loop {
        //"Capture" victim
        captures.push(Eval::cp(*PIECE_VALUES.get(victim)));

        //"Move" attacker to target square
        let attacker_bitboard = attacker_sq.bitboard();
        blockers ^= attacker_bitboard;
        attackers ^= attacker_bitboard;

        //Add new exposed sliding pieces
        if matches!(attacker, Piece::Rook | Piece::Queen) {
            attackers |= get_rook_moves(sq, blockers) & blockers & pieces!(Rook | Queen);
        }
        if matches!(attacker, Piece::Pawn | Piece::Bishop | Piece::Queen) {
            attackers |= get_bishop_moves(sq, blockers) & blockers & pieces!(Bishop | Queen);
        }

        //Swap sides
        color = !color;

        //Try to fetch a new attacker
        for &new_attacker in &Piece::ALL {
            let mut attackers = attackers &
                board.pieces(new_attacker) &
                board.colors(color);
            if let Some(sq) = attackers.next() {
                if victim == Piece::King {
                    //Oops! Our last capture with our king was illegal since this piece is defended.
                    captures.pop();
                    break;
                }

                //New attacker, the old attacker is now the victim
                victim = attacker;
                attacker = new_attacker;
                attacker_sq = sq;
                continue 'exchange;
            }
        }

        //No attacker could be found, calculate final result.
        while captures.len() > 1 {
            //First capture is forced, but all others can be ignored.
            let forced = captures.len() == 2;
            let their_eval = captures.pop().unwrap();
            let our_capture = captures.last_mut().unwrap();
            *our_capture -= their_eval;
            if !forced && *our_capture < Eval::ZERO {
                //Choose not to make the capture.
                *our_capture = Eval::ZERO;
            }
        }
        return captures.pop().unwrap();
    }
}
