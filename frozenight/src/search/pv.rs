use cozy_chess::Move;

use crate::position::Position;
use crate::tt::NodeKind;

use super::window::Window;
use super::{SearchResult, Searcher};

impl Searcher<'_> {
    pub fn pv_search(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
    reverse_leftmost: &[Move],
    ) -> Option<SearchResult> {
        let pv_owner;
        let mut reverse_leftmost = reverse_leftmost;
        let mut remaining_leftmost = &[][..];
        let mut hashmove = None;
        if let Some(entry) = self.shared.tt.get(&position) {
            if entry.depth >= depth {
                match entry.kind {
                    NodeKind::Exact => {
                        if depth < 2 {
                            return Some(SearchResult::mv(entry.eval, entry.mv));
                        }
                    }
                    NodeKind::LowerBound => {
                        if window.fail_high(entry.eval) {
                            return Some(SearchResult::mv(entry.eval, entry.mv));
                        }
                    }
                    NodeKind::UpperBound => {
                        if window.fail_low(entry.eval) {
                            return Some(SearchResult::mv(entry.eval, entry.mv));
                        }
                    }
                }
            }
            let tt_not_good_enough = entry.depth < depth - 2 || entry.kind != NodeKind::Exact;
            if tt_not_good_enough && depth > 3 {
                // internal iterative deepening
                pv_owner = self
                    .pv_search(position, window, depth - 2, reverse_leftmost)?
                    .reverse_pv;
                reverse_leftmost = &pv_owner;
            } else {
                hashmove = Some(entry.mv);
            }
        }

        if let Some((&mv, remain)) = reverse_leftmost.split_last() {
            hashmove = Some(mv);
            remaining_leftmost = remain;
        };

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
                    return Some(-this.visit_pv(
                        &new_pos,
                        -window,
                        depth + extension - 1,
                        remaining_leftmost,
                    )?);
                }

                let reduction = match () {
                    _ if extension > 0 => -extension,
                    _ if position.is_capture(mv) => 0,
                    _ if !new_pos.board.checkers().is_empty() => 0,
                    _ => ((2 * depth + i as i16) / 8).min(i as i16) * 2 / 3,
                };

                let mut v =
                    -this.visit_null(new_pos, -Window::null(window.lb()), depth - reduction - 1)?;

                if window.fail_low(v.eval) {
                    return Some(v);
                }

                if reduction > 0 {
                    v = -this.visit_null(new_pos, -Window::null(window.lb()), depth - 1)?;
                    if window.fail_low(v.eval) {
                        return Some(v);
                    }
                }

                if window.fail_high(v.eval) {
                    // null window search search returned a lower bound that exceeds beta,
                    // so there's no need to re-search
                    return Some(v);
                }

                Some(-this.visit_pv(new_pos, -window, depth + extension - 1, &v.reverse_pv)?)
            },
        )
    }

    fn visit_pv(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
        reverse_leftmost: &[Move],
    ) -> Option<SearchResult> {
        self.visit_node(position, window, depth, |this| {
            this.pv_search(position, window, depth, reverse_leftmost)
        })
    }
}
