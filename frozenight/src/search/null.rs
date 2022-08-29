use crate::position::Position;
use crate::tt::NodeKind;
use crate::Eval;

use super::window::Window;
use super::{Searcher, SearchResult};

use cozy_chess::Piece;

impl Searcher<'_> {
    pub fn visit_null(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
    ) -> Option<SearchResult> {
        self.visit_node(position, window, depth, |this| {
            this.null_search(position, window, depth)
        })
    }

    fn null_search(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
    ) -> Option<SearchResult> {
        let entry = self.shared.tt.get(&position);
        if let Some(entry) = entry {
            match entry.kind {
                _ if entry.depth < depth => {}
                NodeKind::Exact => return Some(SearchResult::mv(entry.eval, entry.mv)),
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
        };

        // mate distance pruning
        let mate_score = Eval::MATE.add_time(position.ply);
        if window.fail_low(mate_score) {
            return Some(SearchResult::eval(mate_score));
        }

        // reverse futility pruning... but with qsearch
        if depth <= 6 {
            let margin = 250 * depth as i16;
            let rfp_window = Window::null(window.lb() + margin);
            let eval = entry
                .map(|e| e.eval)
                .unwrap_or_else(|| self.qsearch(position, rfp_window));
            if rfp_window.fail_high(eval) {
                return Some(SearchResult::eval(eval));
            }
        }

        // null move pruning
        if depth >= 4 {
            let sliders = position.board.pieces(Piece::Rook)
                | position.board.pieces(Piece::Bishop)
                | position.board.pieces(Piece::Queen);
            if !(sliders & position.board.colors(position.board.side_to_move())).is_empty() {
                if let Some(nm) = position.null_move() {
                    let reduction = depth / 2;
                    let v = -self.visit_null(&nm, -window, depth - reduction - 1)?;
                    if window.fail_high(v.eval) {
                        return Some(SearchResult::eval(v.eval));
                    }
                }
            }
        }

        let mut yielded = Vec::with_capacity(64);

        self.search_moves(
            position,
            entry.map(|e| e.mv),
            window,
            depth,
            |this, i, mv, new_pos, window| {
                let extension = match () {
                    _ if !new_pos.board.checkers().is_empty() => 1,
                    _ => 0,
                };

                let reduction = match () {
                    _ if extension > 0 => -extension,
                    _ if position.is_capture(mv) => 0,
                    _ if !new_pos.board.checkers().is_empty() => 0,
                    _ => ((2 * depth + i as i16) / 8).min(i as i16),
                };

                if window.lb() >= -Eval::MAX_INCONCLUSIVE && depth - reduction - 1 < 0 {
                    return Some(SearchResult::eval(-Eval::MATE));
                }

                let mut v = -this.visit_null(new_pos, -window, depth - reduction - 1)?;

                if window.fail_high(v.eval) && reduction > 0 {
                    v = -this.visit_null(new_pos, -window, depth - 1)?;
                }

                if window.fail_high(v.eval) {
                    for &mv in &yielded {
                        this.state.history.did_not_cause_cutoff(position, mv);
                    }
                    return Some(v);
                }

                yielded.push(mv);

                Some(v)
            },
        )
    }
}
