use cozy_chess::Board;

use crate::Eval;

use super::Searcher;

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
        board.generate_moves(|mut mvs| {
            mvs.to &= capture_squares;
            for mv in mvs {
                let mut new_board = board.clone();
                new_board.play_unchecked(mv);
                let v = -self.qsearch(&new_board, -beta, -alpha, ply_index + 1);
                if v > best {
                    best = v;
                }
                if v > alpha {
                    alpha = v;
                    if v >= beta {
                        return true;
                    }
                }
            }
            false
        });

        best
    }
}