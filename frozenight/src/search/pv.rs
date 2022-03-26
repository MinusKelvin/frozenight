use cozy_chess::Move;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::ordering::MoveOrdering;
use super::window::Window;
use super::Searcher;

impl Searcher<'_> {
    pub fn pv_search(
        &mut self,
        position: &Position,
        mut window: Window,
        depth: i16,
    ) -> Option<(Eval, Move)> {
        let hashmove = match self.shared.tt.get(&position) {
            None => None,
            Some(entry) => {
                if entry.depth >= depth {
                    match entry.kind {
                        NodeKind::Exact => return Some((entry.eval, entry.mv)),
                        NodeKind::LowerBound => {
                            if window.fail_high(entry.eval) {
                                return Some((entry.eval, entry.mv));
                            }
                        }
                        NodeKind::UpperBound => {
                            if window.fail_low(entry.eval) {
                                return Some((entry.eval, entry.mv));
                            }
                        }
                    }
                }
                let tt_not_good_enough = entry.depth < depth - 2 || entry.kind != NodeKind::Exact;
                if tt_not_good_enough && depth > 3 {
                    // internal iterative deepening
                    Some(self.pv_search(position, window, depth - 2)?.1)
                } else {
                    Some(entry.mv)
                }
            }
        };

        let mut moves = MoveOrdering::new(&position.board, hashmove, *self.killer(position.ply));

        let (_, mut best_move) = moves.next(&mut self.state.history).unwrap();
        let mut best_score = -self.visit_pv(
            &position.play_move(&self.shared.nnue, best_move),
            -window,
            depth - 1,
        )?;
        if window.fail_high(best_score) {
            self.failed_high(position, depth, best_score, best_move);
            return Some((best_score, best_move));
        }
        let mut raised_alpha = window.raise_lb(best_score);

        while let Some((i, mv)) = moves.next(&mut self.state.history) {
            let new_pos = &position.play_move(&self.shared.nnue, mv);

            let reduction = match () {
                _ if position.is_capture(mv) => 0,
                _ if !new_pos.board.checkers().is_empty() => 0,
                _ => ((depth + i as i16) / 12).min(i as i16 / 2),
            };

            let mut v =
                -self.visit_null(new_pos, -Window::null(window.lb()), depth - reduction - 1)?;

            if window.fail_low(v) {
                if v > best_score {
                    best_score = v;
                    best_move = mv;
                }
                continue;
            }

            if reduction > 0 {
                v = -self.visit_null(new_pos, -Window::null(window.lb()), depth - 1)?;
                if window.fail_low(v) {
                    if v > best_score {
                        best_score = v;
                        best_move = mv;
                    }
                    continue;
                }
            }

            if window.fail_high(v) {
                // null window search search returned a lower bound that exceeds beta,
                // so there's no need to re-search
                self.failed_high(position, depth, v, mv);
                return Some((v, mv));
            }

            v = -self.visit_pv(new_pos, -window, depth - 1)?;

            if window.fail_high(v) {
                self.failed_high(position, depth, v, mv);
                return Some((v, mv));
            }

            if window.raise_lb(v) {
                best_move = mv;
                best_score = v;
                raised_alpha = true;
            }
        }

        if raised_alpha {
            self.shared.tt.store(
                &position,
                TableEntry {
                    mv: best_move,
                    eval: best_score,
                    depth,
                    kind: NodeKind::Exact,
                },
            );
        } else {
            self.failed_low(position, depth, best_score, best_move);
        }

        Some((best_score, best_move))
    }

    fn visit_pv(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        self.visit_node(position, window, depth, |this| {
            this.pv_search(position, window, depth)
                .map(|(eval, _)| eval)
        })
    }
}
