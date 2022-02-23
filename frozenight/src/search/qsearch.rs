use cozy_chess::{BitBoard, Piece};

use crate::position::Position;
use crate::Eval;

use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [u8; 10] = [16, 8, 5, 4, 4, 3, 3, 2, 2, 2];

impl Searcher {
    pub fn qsearch(&mut self, position: &Position, alpha: Eval, beta: Eval) -> Eval {
        self.qsearch_impl(position, alpha, beta, 0)
    }

    fn qsearch_impl(&mut self, position: &Position, mut alpha: Eval, beta: Eval, qply: u16) -> Eval {
        self.stats.selective_depth = self.stats.selective_depth.max(position.ply);
        self.stats.nodes += 1;

        let in_check = !position.board.checkers().is_empty();

        let permitted;
        let mut best;

        if in_check {
            best = -Eval::MATE.add_time(position.ply);
            permitted = BitBoard::FULL;
        } else {
            best = position.static_eval(&self.shared.nnue);
            permitted = position.board.colors(!position.board.side_to_move());
        }

        if best > alpha {
            alpha = best;
            if alpha >= beta {
                return alpha;
            }
        }

        let mut moves = Vec::with_capacity(16);
        let mut had_moves = false;
        position.board.generate_moves(|mut mvs| {
            mvs.to &= permitted;
            had_moves = true;
            for mv in mvs {
                match position.board.piece_on(mv.to) {
                    Some(victim) => {
                        let attacker = PIECE_ORDINALS[mvs.piece as usize];
                        let victim = PIECE_ORDINALS[victim as usize] * 4;
                        moves.push((mv, victim - attacker));
                    }
                    None => moves.push((mv, 0)),
                }
            }
            false
        });

        if !had_moves && !in_check {
            return Eval::DRAW;
        }

        let mut i = 0;
        let limit = match in_check {
            true => 100,
            false => BREADTH_LIMIT.get(qply as usize).copied().unwrap_or(1),
        };
        while !moves.is_empty() && i < limit {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let v = -self.qsearch_impl(
                &position.play_move(&self.shared.nnue, mv),
                -beta,
                -alpha,
                qply + 1,
            );
            if v > best {
                best = v;
            }
            if v > alpha {
                alpha = v;
                if v >= beta {
                    break;
                }
            }

            i += 1;
        }

        best
    }
}
