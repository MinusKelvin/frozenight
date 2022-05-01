use std::sync::atomic::Ordering;

use cozy_chess::{get_king_moves, BitBoard, Board, Move, Piece};

use crate::position::Position;
use crate::Eval;

use super::window::Window;
use super::Searcher;

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];
const BREADTH_LIMIT: [u8; 12] = [16, 8, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1];

struct QsOrdering<'a> {
    board: &'a Board,
    hashmove: Option<Move>,
    moves: Vec<(Move, i8)>,
    has_generated_moves: bool,
    stalemate: bool,
}

impl<'a> QsOrdering<'a> {
    fn new(pos: &'a Position, hashmove: Move) -> Self {
        QsOrdering {
            hashmove: (pos.board.is_legal(hashmove) && pos.is_capture(hashmove)).then(|| hashmove),
            board: &pos.board,
            moves: vec![],
            has_generated_moves: false,
            stalemate: false,
        }
    }
}

impl Iterator for QsOrdering<'_> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if let Some(hashmove) = self.hashmove.take() {
            return Some(hashmove);
        }

        if !self.has_generated_moves {
            self.has_generated_moves = true;

            let king = self.board.king(self.board.side_to_move());
            let in_check = !self.board.checkers().is_empty();

            let permitted;
            let do_for;
            if in_check {
                permitted = BitBoard::FULL;
                do_for = BitBoard::FULL;
            } else {
                permitted = self.board.colors(!self.board.side_to_move());
                do_for = !king.bitboard();
            }

            let mut had_moves = false;
            self.board.generate_moves_for(do_for, |mut mvs| {
                mvs.to &= permitted;
                had_moves = true;
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

            if !in_check {
                for to in get_king_moves(king) & permitted {
                    let mv = Move {
                        from: king,
                        to,
                        promotion: None,
                    };
                    if self.board.is_legal(mv) {
                        had_moves = true;
                        match self.board.piece_on(to) {
                            Some(victim) => {
                                let attacker = PIECE_ORDINALS[Piece::King as usize];
                                let victim = PIECE_ORDINALS[victim as usize] * 4;
                                self.moves.push((mv, victim - attacker));
                            }
                            None => self.moves.push((mv, 0)),
                        }
                    }
                }
                if !had_moves {
                    for to in get_king_moves(king) & !permitted {
                        let mv = Move {
                            from: king,
                            to,
                            promotion: None,
                        };
                        if self.board.is_legal(mv) {
                            had_moves = true;
                            break;
                        }
                    }
                }

                if !had_moves {
                    self.stalemate = true;
                }
            }
        }

        if self.moves.is_empty() {
            return None;
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

impl Searcher<'_> {
    pub fn qsearch(&mut self, position: &Position, window: Window) -> Eval {
        self.qsearch_impl(position, window, 0)
    }

    fn qsearch_impl(&mut self, position: &Position, mut window: Window, qply: u16) -> Eval {
        self.stats
            .selective_depth
            .fetch_max(position.ply, Ordering::Relaxed);
        self.stats.nodes.fetch_add(1, Ordering::Relaxed);

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

        let mini_idx =
            (position.board.hash() % self.state.qsearch_ordering_tt.len() as u64) as usize;

        let mut moves = QsOrdering::new(position, self.state.qsearch_ordering_tt[mini_idx]);

        let limit = match in_check {
            true => 100,
            false => BREADTH_LIMIT.get(qply as usize).copied().unwrap_or(0),
        };
        for mv in (&mut moves).take(limit as usize) {
            let v = -self.qsearch_impl(
                &position.play_move(&self.shared.nnue, mv),
                -window,
                qply + 1,
            );
            if window.fail_high(v) {
                self.state.qsearch_ordering_tt[mini_idx] = mv;
                return v;
            }
            window.raise_lb(v);
            if v > best {
                best = v;
            }
        }

        if moves.stalemate {
            return Eval::DRAW;
        }

        best
    }
}
