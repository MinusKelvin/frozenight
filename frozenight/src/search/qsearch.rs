use std::sync::atomic::Ordering;

use cozy_chess::{get_king_moves, BitBoard, Move, Piece, Rank};

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::window::Window;
use super::{Searcher, INVALID_MOVE};

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [u8; 12] = [16, 8, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1];

impl Searcher<'_> {
    pub fn qsearch(&mut self, position: &Position, window: Window) -> Eval {
        self.qsearch_impl(position, window, 0)
    }

    fn qsearch_impl(&mut self, position: &Position, orig_window: Window, qply: u16) -> Eval {
        self.stats
            .selective_depth
            .fetch_max(position.ply, Ordering::Relaxed);
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

        let in_check = !position.board.checkers().is_empty();
        let us = position.board.side_to_move();
        let king = position.board.king(us);

        let permitted;
        let mut window = orig_window;
        let mut best;
        let mut best_mv = INVALID_MOVE;
        let do_for;

        if in_check {
            best = -Eval::MATE.add_time(position.ply);
            permitted = BitBoard::FULL;
            do_for = BitBoard::FULL;
        } else {
            best = position.static_eval(&self.shared.nnue);
            permitted = position.board.colors(!us);
            do_for = !king.bitboard();
        }

        if window.fail_high(best) {
            return best;
        }
        window.raise_lb(best);

        if let Some(entry) = self.shared.tt.get(position) {
            match entry.kind {
                _ if entry.depth < -(qply as i16) => {}
                NodeKind::Exact => return entry.eval,
                NodeKind::LowerBound => {
                    if window.fail_high(entry.eval) {
                        return entry.eval;
                    }
                }
                NodeKind::UpperBound => {
                    if window.fail_low(entry.eval) {
                        return entry.eval;
                    }
                }
            }
        }

        let mut moves = Vec::with_capacity(16);
        let mut had_moves = false;
        position.board.generate_moves_for(do_for, |mut mvs| {
            if !(mvs.piece == Piece::Pawn && mvs.from.rank() == Rank::Seventh.relative_to(us)) {
                mvs.to &= permitted;
            }
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
            );
            if window.fail_high(v) {
                self.shared.tt.store(
                    &position,
                    TableEntry {
                        mv,
                        eval: v,
                        depth: -(qply as i16),
                        kind: NodeKind::LowerBound,
                    },
                );
                return v;
            }
            window.raise_lb(v);
            if v > best {
                best = v;
                best_mv = mv;
            }

            i += 1;
        }

        if best_mv != INVALID_MOVE {
            self.shared.tt.store(
                &position,
                TableEntry {
                    mv: best_mv,
                    eval: best,
                    depth: -(qply as i16),
                    kind: match orig_window.fail_low(best) {
                        true => NodeKind::UpperBound,
                        false => NodeKind::Exact,
                    },
                },
            );
        }

        best
    }
}
