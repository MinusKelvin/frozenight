use crate::position::Position;
use crate::search::INVALID_MOVE;
use crate::tt::NodeKind;
use crate::Eval;

use super::ordering::MoveOrdering;
use super::window::Window;
use super::Searcher;

impl Searcher<'_> {
    pub fn visit_null(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        self.visit_node(position, window, depth, |this| {
            this.null_search(position, window, depth)
        })
    }

    fn null_search(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        let hashmove = match self.shared.tt.get(&position) {
            None => None,
            Some(entry) => {
                match entry.kind {
                    _ if entry.depth < depth => {}
                    NodeKind::Exact => return Some(entry.eval),
                    NodeKind::LowerBound => {
                        if window.fail_high(entry.eval) {
                            return Some(entry.eval);
                        }
                    }
                    NodeKind::UpperBound => {
                        if window.fail_low(entry.eval) {
                            return Some(entry.eval);
                        }
                    }
                }
                Some(entry.mv)
            }
        };

        // reverse futility pruning... but with qsearch
        if depth <= 6 {
            let margin = 250 * depth as i16;
            let rfp_window = Window::null(window.lb() + margin);
            let eval = self.qsearch(position, rfp_window);
            if rfp_window.fail_high(eval) {
                return Some(eval);
            }
        }

        // null move pruning
        if depth >= 4 {
            if let Some(nm) = position.null_move() {
                let reduction = match () {
                    _ if depth > 6 => 4,
                    _ => 3,
                };
                let v = -self.visit_null(&nm, -window, depth - reduction - 1)?;
                if window.fail_high(v) {
                    return Some(v);
                }
            }
        }

        let mut best_score = -Eval::MATE;
        let mut best_move = INVALID_MOVE;

        let mut moves = MoveOrdering::new(&position.board, hashmove, *self.killer(position.ply));

        while let Some((i, mv)) = moves.next(&mut self.state.history) {
            let new_pos = &position.play_move(&self.shared.nnue, mv);

            let reduction = match () {
                _ if position.is_capture(mv) => 0,
                _ if !new_pos.board.checkers().is_empty() => 0,
                _ if i < 1 => 0,
                _ => (depth + i as i16) / 8,
            };

            let mut v = -self.visit_null(new_pos, -window, depth - reduction - 1)?;

            if window.fail_high(v) && reduction > 0 {
                v = -self.visit_null(new_pos, -window, depth - 1)?;
            }

            if window.fail_high(v) {
                self.failed_high(position, depth, v, mv);
                return Some(v);
            }

            if !position.is_capture(mv) {
                self.state.history.did_not_cause_cutoff(&position.board, mv);
            }

            if v > best_score {
                best_score = v;
                best_move = mv;
            }
        }

        self.failed_low(position, depth, best_score, best_move);

        Some(best_score)
    }
}
