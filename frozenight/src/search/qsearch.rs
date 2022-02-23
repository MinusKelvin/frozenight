use cozy_chess::{BitBoard, Board, Piece};

use crate::nnue::NnueAccumulator;
use crate::Eval;

use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [u8; 12] = [16, 8, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1];

impl Searcher {
    pub fn qsearch(&mut self, nnue: &NnueAccumulator, board: &Board, alpha: Eval, beta: Eval, ply_index: u16) -> Eval {
        self.qsearch_impl(
            nnue,
            board,
            alpha,
            beta,
            ply_index,
            0,
        )
    }

    fn qsearch_impl(
        &mut self,
        nnue: &NnueAccumulator,
        board: &Board,
        mut alpha: Eval,
        beta: Eval,
        ply_index: u16,
        qply: u16,
    ) -> Eval {
        self.stats.selective_depth = self.stats.selective_depth.max(ply_index);
        self.stats.nodes += 1;

        let in_check = !board.checkers().is_empty();

        let permitted;
        let mut best;

        if in_check {
            best = -Eval::MATE.add_time(ply_index);
            permitted = BitBoard::FULL;
        } else {
            best = nnue.calculate(&self.shared.nnue);
            permitted = board.colors(!board.side_to_move());
        }

        if best > alpha {
            alpha = best;
            if alpha >= beta {
                return alpha;
            }
        }

        let mut moves = Vec::with_capacity(16);
        let mut had_moves = false;
        board.generate_moves(|mut mvs| {
            mvs.to &= permitted;
            had_moves = true;
            for mv in mvs {
                match board.piece_on(mv.to) {
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
            false => BREADTH_LIMIT.get(qply as usize).copied().unwrap_or(0),
        };
        while !moves.is_empty() && i < limit {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let mut new_board = board.clone();
            new_board.play_unchecked(mv);
            let v = -self.qsearch_impl(
                &nnue.play_move(&self.shared.nnue, board, mv),
                &new_board,
                -beta,
                -alpha,
                ply_index + 1,
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
