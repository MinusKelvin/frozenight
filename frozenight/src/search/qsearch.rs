use cozy_chess::{Piece, Board};

use crate::Eval;

use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];

impl Searcher {
    pub fn qsearch(&mut self, board: &Board, mut alpha: Eval, beta: Eval, ply_index: u16) -> Eval {
        self.stats.selective_depth = self.stats.selective_depth.max(ply_index);
        self.stats.nodes += 1;

        let mut best = self.nnue.calculate(&self.shared.nnue, board);

        if best > alpha {
            alpha = best;
            if alpha >= beta {
                return alpha;
            }
        }

        let capture_squares = board.colors(!board.side_to_move());
        let mut moves = Vec::with_capacity(16);
        board.generate_moves(|mut mvs| {
            mvs.to &= capture_squares;
            for mv in mvs {
                let attacker = PIECE_ORDINALS[mvs.piece as usize];
                let victim = PIECE_ORDINALS[board.piece_on(mv.to).unwrap() as usize] * 4;
                moves.push((mv, victim - attacker));
            }
            false
        });

        while !moves.is_empty() {
            let mut index = 0;
            for i in 1..moves.len() {
                if moves[i].1 > moves[index].1 {
                    index = i;
                }
            }
            let mv = moves.swap_remove(index).0;

            let mut new_board = board.clone();
            new_board.play_unchecked(mv);
            let v = -self.qsearch(&new_board, -beta, -alpha, ply_index + 1);
            if v > best {
                best = v;
            }
            if v > alpha {
                alpha = v;
                if v >= beta {
                    break;
                }
            }
        }

        best
    }
}