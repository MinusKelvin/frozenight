use cozy_chess::Move;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::ordering::{BREAK, CONTINUE};
use super::window::Window;
use super::{Searcher, INVALID_MOVE};

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
                        NodeKind::Exact => {
                            if depth < 2 {
                                return Some((entry.eval, entry.mv));
                            }
                        }
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

        let mut best_move = INVALID_MOVE;
        let mut best_score = -Eval::MATE;
        let mut raised_alpha = false;
        let mut cutoff = false;
        let mut i = 0;

        self.visit_moves(position, hashmove, |this, mv| {
            let tmp = i;
            i += 1;
            let i = tmp;
            let new_pos = &position.play_move(&this.shared.nnue, mv);

            if best_move == INVALID_MOVE {
                // First move; search as PV node
                best_move = mv;
                best_score = -this.visit_pv(&new_pos, -window, depth - 1)?;
                if window.fail_high(best_score) {
                    this.failed_high(position, depth, best_score, best_move);
                    cutoff = true;
                    return Some(BREAK);
                }
                raised_alpha = window.raise_lb(best_score);
                return Some(CONTINUE);
            }

            let reduction = match () {
                _ if position.is_capture(mv) => 0,
                _ if !new_pos.board.checkers().is_empty() => 0,
                _ => ((2 * depth + i as i16) / 8).min(i as i16) * 2 / 3,
            };

            let mut v =
                -this.visit_null(new_pos, -Window::null(window.lb()), depth - reduction - 1)?;

            if window.fail_low(v) {
                if v > best_score {
                    best_score = v;
                    best_move = mv;
                }
                return Some(CONTINUE);
            }

            if reduction > 0 {
                v = -this.visit_null(new_pos, -Window::null(window.lb()), depth - 1)?;
                if window.fail_low(v) {
                    if v > best_score {
                        best_score = v;
                        best_move = mv;
                    }
                    return Some(CONTINUE);
                }
            }

            if window.fail_high(v) {
                // null window search search returned a lower bound that exceeds beta,
                // so there's no need to re-search
                this.failed_high(position, depth, v, mv);
                best_move = mv;
                best_score = v;
                cutoff = true;
                return Some(BREAK);
            }

            v = -this.visit_pv(new_pos, -window, depth - 1)?;

            if window.fail_high(v) {
                this.failed_high(position, depth, v, mv);
                best_move = mv;
                best_score = v;
                cutoff = true;
                return Some(BREAK);
            }

            if window.raise_lb(v) {
                best_move = mv;
                best_score = v;
                raised_alpha = true;
            }

            Some(CONTINUE)
        })?;

        if !cutoff {
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
