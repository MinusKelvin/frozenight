use std::sync::atomic::Ordering;

use cozy_chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_rook_moves, BitBoard, Move, Piece,
};

use crate::position::Position;
use crate::Eval;

use super::window::Window;
use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [u8; 12] = [16, 8, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1];

impl Searcher<'_> {
    pub fn qsearch(&mut self, position: &Position, window: Window) -> Eval {
        self.qsearch_impl(position, window, 0, 0)
    }

    fn qsearch_impl(
        &mut self,
        position: &Position,
        mut window: Window,
        qply: u16,
        quiets_played: u16,
    ) -> Eval {
        self.stats
            .selective_depth
            .fetch_max(position.ply, Ordering::Relaxed);
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

        let in_check = !position.board.checkers().is_empty();
        let us = position.board.side_to_move();
        let king = position.board.king(us);

        let permitted;
        let mut best;
        let do_for;
        let bishop_checks;
        let rook_checks;
        let knight_checks;

        if in_check {
            best = -Eval::MATE.add_time(position.ply);
            bishop_checks = BitBoard::EMPTY;
            rook_checks = BitBoard::EMPTY;
            knight_checks = BitBoard::EMPTY;
            permitted = BitBoard::FULL;
            do_for = BitBoard::FULL;
        } else {
            best = position.static_eval(&self.shared.nnue);
            permitted = position.board.colors(!us);
            do_for = !king.bitboard();
            if quiets_played < 1 {
                let their_king = position.board.king(!us);
                let occupied = position.board.occupied();
                bishop_checks = get_bishop_moves(their_king, occupied);
                rook_checks = get_rook_moves(their_king, occupied);
                knight_checks = get_knight_moves(their_king);
            } else {
                bishop_checks = BitBoard::EMPTY;
                rook_checks = BitBoard::EMPTY;
                knight_checks = BitBoard::EMPTY;
            }
        }

        if window.fail_high(best) {
            return best;
        }
        window.raise_lb(best);

        let mut moves = Vec::with_capacity(16);
        let mut had_moves = false;
        position.board.generate_moves_for(do_for, |mut mvs| {
            let permitted = match mvs.piece {
                Piece::Rook => permitted | rook_checks,
                Piece::Bishop => permitted | bishop_checks,
                Piece::Queen => permitted | rook_checks | bishop_checks,
                Piece::Knight => permitted | knight_checks,
                _ => permitted,
            };
            mvs.to &= permitted;
            had_moves = true;
            for mv in mvs {
                match position.board.piece_on(mv.to) {
                    Some(victim) => {
                        let attacker = PIECE_ORDINALS[mvs.piece as usize];
                        let victim = PIECE_ORDINALS[victim as usize] * 4;
                        moves.push((mv, victim - attacker));
                    }
                    None => moves.push((mv, 0)),
                }
            }
            false
        });

        if !in_check {
            for to in get_king_moves(king) & permitted {
                let mv = Move {
                    from: king,
                    to,
                    promotion: None,
                };
                if position.board.is_legal(mv) {
                    had_moves = true;
                    match position.board.piece_on(to) {
                        Some(victim) => {
                            let attacker = PIECE_ORDINALS[Piece::King as usize];
                            let victim = PIECE_ORDINALS[victim as usize] * 4;
                            moves.push((mv, victim - attacker));
                        }
                        None => moves.push((mv, 0)),
                    }
                }
            }
            if !had_moves {
                for to in get_king_moves(king) & !permitted {
                    let mv = Move {
                        from: king,
                        to,
                        promotion: None,
                    };
                    if position.board.is_legal(mv) {
                        had_moves = true;
                        break;
                    }
                }
            }

            if !had_moves {
                return Eval::DRAW;
            }
        }

        let mut i = 0;
        let limit = match in_check {
            true => 100,
            false => BREADTH_LIMIT.get(qply as usize).copied().unwrap_or(0),
        };
        while !moves.is_empty() && i < limit {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let v = -self.qsearch_impl(
                &position.play_move(&self.shared.nnue, mv),
                -window,
                qply + 1,
                quiets_played + !position.board.colors(!us).has(mv.to) as u16
            );
            if window.fail_high(v) {
                return v;
            }
            window.raise_lb(v);
            if v > best {
                best = v;
            }

            i += 1;
        }

        best
    }
}
