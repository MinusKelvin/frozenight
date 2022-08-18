use std::sync::atomic::Ordering;

use cozy_chess::{Piece, Rank};

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::see::static_exchange_eval;
use super::window::Window;
use super::{Searcher, INVALID_MOVE};

impl Searcher<'_> {
    pub fn qsearch(&mut self, position: &Position, orig_window: Window) -> Eval {
        self.stats
            .selective_depth
            .fetch_max(position.ply, Ordering::Relaxed);
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

        let in_check = !position.board.checkers().is_empty();
        let us = position.board.side_to_move();

        let permitted = position.board.colors(!us);
        let mut best = position.static_eval(&self.shared.nnue);
        let mut best_mv = INVALID_MOVE;
        let mut window = orig_window;

        if !in_check {
            if window.fail_high(best) {
                return best;
            }
            window.raise_lb(best);
        }

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
        position.board.generate_moves(|mut mvs| {
            if !(mvs.piece == Piece::Pawn && mvs.from.rank() == Rank::Seventh.relative_to(us)) {
                mvs.to &= permitted;
            }
            had_moves = true;
            for mv in mvs {
                if position.board.occupied().has(mv.to) {
                    let see = static_exchange_eval(&position.board, mv);
                    if see >= 0 || in_check {
                        moves.push((mv, see));
                    }
                } else {
                    moves.push((mv, 0))
                }
            }
            false
        });

        if !had_moves {
            return match in_check {
                true => -Eval::MATE.add_time(position.ply),
                false => Eval::DRAW,
            };
        }

        if in_check {
            if window.fail_high(best) {
                return best;
            }
            window.raise_lb(best);
        }

        while !moves.is_empty() {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let v = -self.qsearch(&position.play_move(&self.shared.nnue, mv), -window);
            if window.fail_high(v) {
                self.shared.tt.store(
                    &position,
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
                &position,
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
