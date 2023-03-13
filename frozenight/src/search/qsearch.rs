use std::sync::atomic::Ordering;

use cozy_chess::Move;

use crate::position::Position;
use crate::Eval;

use super::negamax::SearchType;
use super::window::Window;
use super::Searcher;

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
            for mv in mvs {
                moves.push((
                    mv,
                    pos.board.piece_on(mv.to).unwrap() as i16 * 8 - mvs.piece as i16,
                ));
            }
            false
        });

        while let Some((i, &(mv, score))) = moves
            .iter()
            .enumerate()
            .max_by_key(|&(_, &(_, score))| score)
        {
            moves.swap_remove(i);

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
