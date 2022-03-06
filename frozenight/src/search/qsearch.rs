use cozy_chess::{BitBoard, Board, Move, Piece};

use crate::position::Position;
use crate::Eval;

use super::window::Window;
use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [usize; 12] = [16, 8, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1];

struct MoveOrdering<'a> {
    board: &'a Board,
    permitted: BitBoard,
    moves: Vec<(Move, i8)>,
    done_king: bool,
    had_moves: bool,
}

impl<'a> MoveOrdering<'a> {
    fn new(board: &'a Board) -> Self {
        let mut this = MoveOrdering {
            board,
            permitted: match board.checkers().is_empty() {
                true => board.colors(!board.side_to_move()),
                false => BitBoard::FULL,
            },
            moves: Vec::with_capacity(16),
            done_king: false,
            had_moves: false,
        };
        this.gen_moves(!board.king(board.side_to_move()).bitboard());
        this
    }

    fn gen_moves(&mut self, mask: BitBoard) {
        self.board.generate_moves_for(mask, |mut mvs| {
            mvs.to &= self.permitted;
            self.had_moves = true;
            for mv in mvs {
                match self.board.piece_on(mv.to) {
                    Some(victim) => {
                        let attacker = PIECE_ORDINALS[mvs.piece as usize];
                        let victim = PIECE_ORDINALS[victim as usize] * 4;
                        self.moves.push((mv, victim - attacker));
                    }
                    None => self.moves.push((mv, 0)),
                }
            }
            false
        });
    }
}

impl Iterator for MoveOrdering<'_> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if self.moves.is_empty() {
            if !self.done_king {
                self.gen_moves(self.board.king(self.board.side_to_move()).bitboard());
                self.done_king = true;
            }
            if self.moves.is_empty() {
                return None;
            }
        }

        let mut index = 0;
        for i in 1..self.moves.len() {
            if self.moves[i].1 > self.moves[index].1 {
                index = i;
            }
        }
        Some(self.moves.swap_remove(index).0)
    }
}

impl Searcher {
    pub fn qsearch(&mut self, position: &Position, window: Window) -> Eval {
        self.qsearch_impl(position, window, 0)
    }

    fn qsearch_impl(&mut self, position: &Position, mut window: Window, qply: u16) -> Eval {
        self.stats.selective_depth = self.stats.selective_depth.max(position.ply);
        self.stats.nodes += 1;

        let in_check = !position.board.checkers().is_empty();

        let mut best;

        if in_check {
            best = -Eval::MATE.add_time(position.ply);
        } else {
            best = position.static_eval(&self.shared.nnue);
        }

        if window.fail_high(best) {
            return best;
        }
        window.raise_lb(best);

        let mut moves = MoveOrdering::new(&position.board);
        let iter = (&mut moves).take(match in_check {
            true => 100,
            false => BREADTH_LIMIT.get(qply as usize).copied().unwrap_or(0),
        });
        for mv in iter {
            let v = -self.qsearch_impl(
                &position.play_move(&self.shared.nnue, mv),
                -window,
                qply + 1,
            );
            if window.fail_high(v) {
                return v;
            }
            window.raise_lb(v);
            if v > best {
                best = v;
            }
        }

        if !moves.had_moves && !in_check {
            return Eval::DRAW;
        }

        best
    }
}
