use cozy_chess::Move;

use crate::position::Position;
use crate::tt::NodeKind;
use crate::Eval;

use super::params::*;
use super::window::Window;
use super::Searcher;

impl Searcher<'_> {
    pub fn pv_search(
        &mut self,
        position: &Position,
        window: Window,
        new_pv: bool,
        mut depth: i16,
    ) -> Option<(Eval, Move)> {
        let mut hashmove = match self.shared.tt.get(position) {
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
                if entry.kind == NodeKind::Exact && !new_pv {
                    if position.ply % 4 == 0 && position.ply as i16 * 2 < depth {
                        depth += 1;
                    }
                }
                Some(entry.mv)
            }
        };

        if new_pv && depth > 3 {
            hashmove = Some(self.pv_search(position, window, true, depth - 2)?.1);
        }

        self.search_moves(
            position,
            hashmove,
            window,
            depth,
            |this, i, mv, new_pos, window| {
                let extension = match () {
                    _ if !new_pos.board.checkers().is_empty() => 1,
                    _ => 0,
                };

                if i == 0 {
                    // First move; search as PV node
                    return Some(-this.visit_pv(new_pos, -window, false, depth + extension - 1)?);
                }

                let reduction = match () {
                    _ if extension > 0 => -extension,
                    _ if position.is_capture(mv) => 0,
                    _ if !new_pos.board.checkers().is_empty() => 0,
                    _ => pv_lmr(depth, i),
                };

                let mut v =
                    -this.visit_null(new_pos, -Window::null(window.lb()), depth - reduction - 1)?;

                if window.fail_low(v) {
                    return Some(v);
                }

                if reduction > 0 {
                    v = -this.visit_null(new_pos, -Window::null(window.lb()), depth - 1)?;
                    if window.fail_low(v) {
                        return Some(v);
                    }
                }

                if window.fail_high(v) {
                    // null window search search returned a lower bound that exceeds beta,
                    // so there's no need to re-search
                    return Some(v);
                }

                Some(-this.visit_pv(new_pos, -window, true, depth + extension - 1)?)
            },
        )
    }

    fn visit_pv(
        &mut self,
        position: &Position,
        window: Window,
        new_pv: bool,
        depth: i16,
    ) -> Option<Eval> {
        self.visit_node(position, window, depth, |this| {
            this.pv_search(position, window, new_pv, depth)
                .map(|(eval, _)| eval)
        })
    }
}
