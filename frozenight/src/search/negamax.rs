use std::sync::atomic::Ordering;
use std::time::Instant;

use cozy_chess::Move;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::ordering::{MovePicker, MoveScore};
use super::params::*;
use super::window::Window;
use super::{estimate_nodes_to_deadline, Searcher};

impl Searcher<'_> {
    pub(crate) fn negamax(
        &mut self,
        search: impl SearchType,
        pos: &Position,
        mut window: Window,
        depth: i16,
    ) -> Option<(Eval, Option<Move>)> {
        if depth <= 0 {
            return self.qsearch(search, pos, window);
        }

        let n = self.stats.nodes.fetch_add(1, Ordering::Relaxed);
        if self.allow_abort {
            if n >= self.node_limit || self.abort.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(deadline) = self.deadline {
                if n >= self.next_deadline_check {
                    let to_deadline = deadline.checked_duration_since(Instant::now())?;
                    self.next_deadline_check = n + estimate_nodes_to_deadline(to_deadline);
                }
            }
        }

        let tt = self.tt.get(pos);
        let mut hashmv = None;
        if let Some(tt) = tt {
            hashmv = Some(tt.mv);
            let bound_allows_cutoff = match tt.kind {
                NodeKind::Exact => true,
                NodeKind::LowerBound => window.fail_high(tt.eval),
                NodeKind::UpperBound => window.fail_low(tt.eval),
            };
            if tt.depth >= depth && bound_allows_cutoff {
                return Some((tt.eval, Some(tt.mv)));
            }
        }

        if search.pv() && tt.map_or(true, |tt| tt.kind != NodeKind::Exact) {
            hashmv = self.negamax(search, pos, window, depth - 2)?.1;
        }

        let eval = tt
            .map(|tt| tt.eval)
            .filter(|e| !e.is_conclusive())
            .unwrap_or_else(|| pos.static_eval());

        if !search.pv()
            && depth <= RFP_MAX_DEPTH.get()
            && Window::null(window.ub() + depth * RFP_MARGIN.get()).fail_high(eval)
        {
            return Some((eval, None));
        }

        if !search.pv()
            && pos.board.checkers().is_empty()
            && depth >= NMP_MIN_DEPTH.get()
            && window.fail_high(eval)
        {
            let new_pos = &pos.null_move(self.tt).unwrap();
            let reduction = fp_mul(depth, NMP_DEPTH_FACTOR.get()) + NMP_BASE_REDUCTION.get();
            let zw = Window::null(window.ub() - 1);

            self.state.move_stack[pos.ply as usize] = None;
            let v = -self
                .negamax(ZeroWidth, new_pos, -zw, depth - reduction - 1)?
                .0;

            if zw.fail_high(v) {
                return Some((v, None));
            }
        }

        let mut move_picker = MovePicker::new(pos, hashmv);
        let mut best = -Eval::MATE.add_time(pos.ply);
        let mut best_mv = None;
        let mut raised_alpha = false;

        while let Some((i, mv, score)) = move_picker.pick_move(&self.state) {
            let new_pos = &pos.play_move(mv, self.tt);

            let mut v;

            if self.is_repetition(&new_pos.board) {
                v = Eval::DRAW;
            } else {
                self.push_repetition(&new_pos.board);
                self.state.move_stack[pos.ply as usize] =
                    Some((pos.board.piece_on(mv.from).unwrap(), mv.to));

                let ext = !new_pos.board.checkers().is_empty() as i16;

                if i == 0 {
                    v = -self.negamax(search, new_pos, -window, depth + ext - 1)?.0;
                } else {
                    let mut reduction = base_lmr(i, depth);

                    reduction += (i as i16 > LMR_EXTRA.get()) as i16;

                    if ext > 0 {
                        reduction = 0;
                    }

                    let zw = Window::null(window.lb());
                    v = -self
                        .negamax(ZeroWidth, new_pos, -zw, depth + ext - reduction - 1)?
                        .0;

                    if reduction > 0 && zw.fail_high(v) {
                        v = -self.negamax(ZeroWidth, new_pos, -zw, depth - 1)?.0;
                    }

                    if window.inside(v) {
                        v = -self.negamax(search, new_pos, -window, depth + ext - 1)?.0;
                    }
                }

                self.pop_repetition();
            }

            if v > best {
                best = v;
                best_mv = Some(mv);
            }

            if window.fail_high(v) {
                self.update_history(move_picker, mv, depth);
                break;
            }

            raised_alpha |= window.raise_lb(v);
        }

        if best_mv.is_none() && pos.board.checkers().is_empty() {
            return Some((Eval::DRAW, best_mv));
        }

        if let Some(best_mv) = best_mv {
            self.tt.store(
                pos,
                TableEntry {
                    mv: best_mv,
                    eval: best,
                    depth,
                    kind: match () {
                        _ if window.fail_high(best) => NodeKind::LowerBound,
                        _ if raised_alpha => NodeKind::Exact,
                        _ => NodeKind::UpperBound,
                    },
                },
            );
        }

        return Some((best, best_mv));
    }
}

pub trait SearchType: Copy {
    fn pv(&self) -> bool;
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pv;

impl SearchType for Pv {
    fn pv(&self) -> bool {
        true
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ZeroWidth;

impl SearchType for ZeroWidth {
    fn pv(&self) -> bool {
        false
    }
}
