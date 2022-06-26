use crate::position::Position;
use crate::tt::NodeKind;
use crate::Eval;

use super::ordering::MAX_HISTORY_SCORE;
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

        // mate distance pruning
        let mate_score = Eval::MATE.add_time(position.ply);
        if window.fail_low(mate_score) {
            return Some(mate_score);
        }

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
            let sliders = position.board.pieces(Piece::Rook)
                | position.board.pieces(Piece::Bishop)
                | position.board.pieces(Piece::Queen);
            if !(sliders & position.board.colors(position.board.side_to_move())).is_empty() {
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
        }

        self.search_moves(
            position,
            hashmove,
            window,
            depth,
            |this, i, mv, new_pos, window| {
                let mut reduction = 0;

                let tactical =
                    position.is_capture(mv) || !new_pos.board.checkers().is_empty() || i == 0;

                if !tactical {
                    reduction = ((2 * depth + i as i16) / 8).min(i as i16);

                    const HISTORY_THRESHOLD: u32 = (MAX_HISTORY_SCORE as f64 * 0.05) as u32;
                    if depth > 3
                        && this.state.history.score(&position.board, mv) < HISTORY_THRESHOLD
                    {
                        reduction += 1;
                    }
                }

                if depth - reduction - 1 < 0 {
                    return Some(-Eval::MATE);
                }

                let mut v = -this.visit_null(new_pos, -window, depth - reduction - 1)?;

                if window.fail_high(v) && reduction > 0 {
                    v = -this.visit_null(new_pos, -window, depth - 1)?;
                }

                if window.fail_high(v) {
                    return Some(v);
                }

                if !position.is_capture(mv) {
                    this.state.history.did_not_cause_cutoff(&position.board, mv);
                }

                Some(v)
            },
        )
        .map(|(e, _)| e)
    }
}
