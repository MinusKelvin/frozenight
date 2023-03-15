use std::sync::atomic::Ordering;

use cozy_chess::Move;

use crate::position::Position;
use crate::tt::{NodeKind, TableEntry};
use crate::Eval;

use super::negamax::SearchType;
use super::oracle::oracle;
use super::see::static_exchange_eval;
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

        let tt = self.tt.get(pos);
        if let Some(tt) = tt {
            let bound_allows_cutoff = match tt.kind {
                NodeKind::Exact => true,
                NodeKind::LowerBound => window.fail_high(tt.eval),
                NodeKind::UpperBound => window.fail_low(tt.eval),
            };
            if bound_allows_cutoff {
                return Some((tt.eval, Some(tt.mv)));
            }
        }

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
                let see = static_exchange_eval(&pos.board, mv);
                if see >= 0 {
                    moves.push((
                        mv,
                        (Some(mv) == tt.map(|tt| tt.mv)) as i16 * 1000
                            + pos.board.piece_on(mv.to).unwrap() as i16 * 8
                            - mvs.piece as i16,
                    ));
                }
            }
            false
        });

        while let Some((i, &(mv, _))) = moves
            .iter()
            .enumerate()
            .max_by_key(|&(_, &(_, score))| score)
        {
            moves.swap_remove(i);

            let new_pos = &pos.play_move(mv, self.tt);

            let v;
            if let Some(known) = oracle(&new_pos.board) {
                v = known;
            } else {
                v = -self.qsearch(st, new_pos, -window)?.0;
            }

            if v > best {
                best = v;
                best_mv = Some(mv);
            }

            if window.fail_high(v) {
                break;
            }

            raised_alpha |= window.raise_lb(v);
        }

        if let Some(best_mv) = best_mv {
            self.tt.store(
                pos,
                TableEntry {
                    mv: best_mv,
                    eval: best,
                    depth: 0,
                    kind: match () {
                        _ if window.fail_high(best) => NodeKind::LowerBound,
                        _ if raised_alpha => NodeKind::Exact,
                        _ => NodeKind::UpperBound,
                    },
                },
            );
        }

        Some((best, best_mv))
    }
}
