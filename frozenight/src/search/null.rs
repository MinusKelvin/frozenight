use crate::position::Position;
use crate::tt::NodeKind;
use crate::Eval;

use super::window::Window;
use super::Searcher;

use cozy_chess::{Move, Piece};

impl Searcher<'_> {
    pub fn visit_null(&mut self, position: &Position, window: Window, depth: i16) -> Option<Eval> {
        self.visit_node(position, window, depth, |this| {
            this.null_search(position, window, depth, None)
        })
    }

    pub fn null_search(
        &mut self,
        position: &Position,
        window: Window,
        depth: i16,
        skip: Option<Move>,
    ) -> Option<Eval> {
        let entry = match skip.is_some() {
            false => self.shared.tt.get(&position),
            true => None,
        };
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
        if depth <= 6 {
            let margin = 250 * depth as i16;
            let rfp_window = Window::null(window.lb() + margin);
            let eval = entry
                .map(|e| e.eval)
                .unwrap_or_else(|| self.qsearch(position, rfp_window));
            if rfp_window.fail_high(eval) {
                return Some(eval);
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
                    if window.fail_high(v) {
                        return Some(v);
                    }
                }
            }
        }

        let mut yielded = Vec::with_capacity(64);

        self.search_moves(
            position,
            entry.map(|e| e.mv),
            skip,
            window,
            depth,
            |this, i, mv, new_pos, window| {
                let mut extension = match () {
                    _ if !new_pos.board.checkers().is_empty() => 1,
                    _ => 0,
                };

                // Singular extension
                if let Some(entry) = entry {
                    if i == 0
                        && extension < 1
                        && entry.depth >= depth - 2
                        && matches!(entry.kind, NodeKind::Exact | NodeKind::LowerBound)
                        && depth >= 7
                    {
                        let singular_window = Window::null(entry.eval - depth * 50);
                        let v =
                            this.null_search(position, singular_window, depth / 2, Some(entry.mv))?;

                        if singular_window.fail_low(v) {
                            extension = 1;
                        }
                    }
                }

                let reduction = match () {
                    _ if extension > 0 => -extension,
                    _ if position.is_capture(mv) => 0,
                    _ if !new_pos.board.checkers().is_empty() => 0,
                    _ => ((2 * depth + i as i16) / 8).min(i as i16),
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
