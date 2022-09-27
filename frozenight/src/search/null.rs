use crate::position::Position;
use crate::tt::NodeKind;
use crate::Eval;

use super::params::*;
use super::window::Window;
use super::Searcher;

use cozy_chess::Piece;

impl Searcher<'_> {
    pub fn visit_null(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        self.visit_node(position, window, depth, |this| {
            this.null_search(position, window, depth)
        })
    }

    fn null_search(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        let entry = self.shared.tt.get(position);
        if let Some(entry) = entry {
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
        };

        // mate distance pruning
        let mate_score = Eval::MATE.add_time(position.ply);
        if window.fail_low(mate_score) {
            return Some(mate_score);
        }

        // reverse futility pruning... but with qsearch
        if depth <= RFP_MAX_DEPTH.get() {
            let rfp_window = Window::null(window.lb() + rfp_margin(depth));
            let eval = entry
                .map(|e| e.eval)
                .unwrap_or_else(|| self.qsearch(position, rfp_window));
            if rfp_window.fail_high(eval) {
                return Some(eval);
            }
        }

        // null move pruning
        let our_sliders = (position.board.pieces(Piece::Rook)
            | position.board.pieces(Piece::Bishop)
            | position.board.pieces(Piece::Queen))
            & position.board.colors(position.board.side_to_move());
        let do_nmp = depth >= NMP_MIN_DEPTH.get()
            && !our_sliders.is_empty()
            && window.fail_high(position.static_eval());
        if do_nmp {
            if let Some(nm) = position.null_move() {
                let reduction = nmp_reduction(depth);
                let v = -self.visit_null(&nm, -window, depth - reduction - 1)?;
                if window.fail_high(v) {
                    return Some(v);
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
                    _ if position.is_capture(mv) => null_lmr(depth, i) / 4,
                    _ if !new_pos.board.checkers().is_empty() => 0,
                    _ => null_lmr(depth, i),
                };

                if window.lb() >= -Eval::MAX_INCONCLUSIVE && depth - reduction - 1 < 0 {
                    return Some(-Eval::MATE);
                }

                let mut v = -this.visit_null(new_pos, -window, depth - reduction - 1)?;

                if window.fail_high(v) && reduction > 0 {
                    v = -this.visit_null(new_pos, -window, depth - 1)?;
                }

                if window.fail_high(v) {
                    for &mv in &yielded {
                        this.state.history.did_not_cause_cutoff(position, mv);
                    }
                    return Some(v);
                }

                yielded.push(mv);

                Some(v)
            },
        )
        .map(|(e, _)| e)
    }
}
