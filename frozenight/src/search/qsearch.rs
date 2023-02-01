use std::sync::atomic::Ordering;

use cozy_chess::{get_king_moves, BitBoard, Move, Piece, Rank};

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::see::static_exchange_eval;
use super::window::Window;
use super::{Searcher, INVALID_MOVE};

impl Searcher<'_> {
    pub fn qsearch(&mut self, position: &Position, orig_window: Window) -> Eval {
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

        let in_check = !position.board.checkers().is_empty();
        let us = position.board.side_to_move();
        let king = position.board.king(us);

        let permitted;
        let mut best;
        let mut best_mv = INVALID_MOVE;
        let mut window = orig_window;
        let do_for;

        if in_check {
            best = -Eval::MATE.add_time(position.ply);
            permitted = BitBoard::FULL;
            do_for = BitBoard::FULL;
        } else {
            best = position.static_eval();
            permitted = position.board.colors(!us);
            do_for = !king.bitboard();
        }

        if window.fail_high(best) {
            return best;
        }
        window.raise_lb(best);

        if let Some(entry) = self.shared.tt.get(position) {
            match entry.kind {
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
            let promo =
                mvs.piece == Piece::Pawn && mvs.from.rank() == Rank::Seventh.relative_to(us);
            if !promo {
                mvs.to &= permitted;
            }
            had_moves = true;
            for to in mvs.to {
                let mv = Move {
                    from: mvs.from,
                    to,
                    promotion: promo.then(|| Piece::Queen),
                };
                if position.is_capture(mv) {
                    let victim = position.board.piece_on(mv.to).unwrap();
                    let mvv_lva = 8 * victim as i32 - mvs.piece as i32 + 8;
                    let see = static_exchange_eval(&position.board, mv);
                    if see >= 0 || in_check {
                        moves.push((mv, see + mvv_lva));
                    }
                } else {
                    moves.push((mv, 0))
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
                    if position.board.occupied().has(mv.to) {
                        let see = static_exchange_eval(&position.board, mv);
                        if see >= 0 {
                            moves.push((mv, see));
                        }
                    } else {
                        moves.push((mv, 0))
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

        while !moves.is_empty() {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let v = -self.qsearch(&position.play_move(mv, &self.shared.tt), -window);
            if window.fail_high(v) {
                self.shared.tt.store(
                    position,
                    TableEntry {
                        mv,
                        eval: v,
                        depth: 0,
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
        }

        if best_mv != INVALID_MOVE {
            self.shared.tt.store(
                position,
                TableEntry {
                    mv: best_mv,
                    eval: best,
                    kind: if window.fail_low(best) {
                        NodeKind::UpperBound
                    } else {
                        NodeKind::Exact
                    },
                    depth: 0,
                },
            );
        }

        best
    }
}
