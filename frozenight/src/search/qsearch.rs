use std::sync::atomic::Ordering;

use cozy_chess::Move;

use crate::Eval;
use crate::position::Position;

use super::Searcher;
use super::negamax::SearchType;
use super::window::Window;

impl Searcher<'_> {
    pub(crate) fn qsearch(
        &mut self,
        st: impl SearchType,
        pos: &Position,
        mut window: Window,
    ) -> Option<(Eval, Option<Move>)> {
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

        let mut best = pos.static_eval();
        let mut best_mv = None;
        let mut raised_alpha = false;

        if window.fail_high(best) {
            return Some((best, None));
        }
        window.raise_lb(best);

        let mut moves = Vec::with_capacity(64);
        pos.board.generate_moves(|mut mvs| {
            mvs.to &= pos.board.colors(!pos.board.side_to_move());
            moves.extend(mvs);
            false
        });

        for mv in moves {
            let new_pos = &pos.play_move(mv, self.tt);

            let v = -self.qsearch(st, new_pos, -window)?.0;

            if v > best {
                best = v;
                best_mv = Some(mv);
            }

            if window.fail_high(v) {
                break;
            }

            raised_alpha |= window.raise_lb(v);
        }

        Some((best, best_mv))
    }
}
