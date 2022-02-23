use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square};

use crate::Eval;

type Vector = [i32; 16];

const NUM_FEATURES: usize = Color::NUM * Piece::NUM * Square::NUM;

pub struct Nnue {
    input_layer: [Vector; NUM_FEATURES],
    input_layer_bias: Vector,
    hidden_layer: [i32; 32],
    hidden_layer_bias: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NnueAccumulator {
    white: Vector,
    black: Vector,
    side_to_move: Color,
}

impl Nnue {
    pub fn new() -> Nnue {
        include!("../model.rs")
    }
}

impl NnueAccumulator {
    pub fn new(board: &Board, nn: &Nnue) -> Self {
        let mut white = nn.input_layer_bias;
        let mut black = nn.input_layer_bias;
        for p in Piece::ALL {
            for sq in board.pieces(p) {
                let color = match board.colors(Color::White).has(sq) {
                    true => Color::White,
                    false => Color::Black,
                };
                white = vadd(white, nn.input_layer[feature(color, p, sq)]);
                black = vadd(black, nn.input_layer[feature(!color, p, sq.flip_rank())]);
            }
        }
        NnueAccumulator {
            white,
            black,
            side_to_move: board.side_to_move(),
        }
    }

    pub fn calculate(&self, nn: &Nnue) -> Eval {
        let l1_input = clipped_relu(bytemuck::cast(match self.side_to_move {
            Color::White => [self.white, self.black],
            Color::Black => [self.black, self.white],
        }));
        let output = vdot(l1_input, nn.hidden_layer);

        Eval::new((nn.hidden_layer_bias + output) as i16)
    }

    pub fn swap_sides(&self) -> Self {
        NnueAccumulator {
            side_to_move: !self.side_to_move,
            ..*self
        }
    }

    pub fn play_move(&self, nn: &Nnue, board: &Board, mv: Move) -> Self {
        let mut result = self.swap_sides();

        let us = board.side_to_move();
        let moved = board.piece_on(mv.from).unwrap();

        // remove piece on from square
        result.white = vsub(result.white, nn.input_layer[feature(us, moved, mv.from)]);
        result.black = vsub(
            result.black,
            nn.input_layer[feature(!us, moved, mv.from.flip_rank())],
        );

        // remove piece on to square
        if let Some((color, piece)) = board.color_on(mv.to).zip(board.piece_on(mv.to)) {
            result.white = vsub(result.white, nn.input_layer[feature(color, piece, mv.to)]);
            result.black = vsub(
                result.black,
                nn.input_layer[feature(!color, piece, mv.to.flip_rank())],
            )
        }

        // remove EP-captured pawn
        if let Some(ep_file) = board.en_passant() {
            if moved == Piece::Pawn && mv.to == Square::new(ep_file, Rank::Sixth.relative_to(us)) {
                result.white = vsub(
                    result.white,
                    nn.input_layer[feature(
                        !us,
                        Piece::Pawn,
                        Square::new(ep_file, Rank::Fifth.relative_to(us)),
                    )],
                );
                result.black = vsub(
                    result.black,
                    nn.input_layer[feature(
                        us,
                        Piece::Pawn,
                        Square::new(ep_file, Rank::Fifth.relative_to(!us)),
                    )],
                );
            }
        }

        if Some(us) == board.color_on(mv.to) {
            let rank = Rank::First.relative_to(us);
            if mv.from.file() > mv.to.file() {
                // castle queen-side
                result.white = vadd(
                    result.white,
                    nn.input_layer[feature(us, Piece::King, Square::new(File::C, rank))],
                );
                result.white = vadd(
                    result.white,
                    nn.input_layer[feature(us, Piece::Rook, Square::new(File::D, rank))],
                );
                result.black = vadd(
                    result.black,
                    nn.input_layer[feature(!us, Piece::King, Square::new(File::C, rank.flip()))],
                );
                result.black = vadd(
                    result.black,
                    nn.input_layer[feature(!us, Piece::Rook, Square::new(File::D, rank.flip()))],
                );
            } else {
                // castle king-side
                result.white = vadd(
                    result.white,
                    nn.input_layer[feature(us, Piece::King, Square::new(File::G, rank))],
                );
                result.white = vadd(
                    result.white,
                    nn.input_layer[feature(us, Piece::Rook, Square::new(File::F, rank))],
                );
                result.black = vadd(
                    result.black,
                    nn.input_layer[feature(!us, Piece::King, Square::new(File::G, rank.flip()))],
                );
                result.black = vadd(
                    result.black,
                    nn.input_layer[feature(!us, Piece::Rook, Square::new(File::F, rank.flip()))],
                );
            }
        } else {
            let added = mv.promotion.unwrap_or(moved);
            result.white = vadd(result.white, nn.input_layer[feature(us, added, mv.to)]);
            result.black = vadd(
                result.black,
                nn.input_layer[feature(!us, added, mv.to.flip_rank())],
            );
        }

        result
    }
}

fn vadd<const N: usize>(a: [i32; N], b: [i32; N]) -> [i32; N] {
    let mut result = [0; N];
    for i in 0..N {
        result[i] = a[i] + b[i];
    }
    result
}

fn vsub<const N: usize>(a: [i32; N], b: [i32; N]) -> [i32; N] {
    let mut result = [0; N];
    for i in 0..N {
        result[i] = a[i] - b[i];
    }
    result
}

fn clipped_relu<const N: usize>(a: [i32; N]) -> [i32; N] {
    let mut result = [0; N];
    for i in 0..N {
        result[i] = a[i].clamp(0, 127);
    }
    result
}

fn vdot<const N: usize>(a: [i32; N], b: [i32; N]) -> i32 {
    let mut result = 0;
    for i in 0..N {
        result += a[i] * b[i];
    }
    result / 64
}

fn feature(color: Color, piece: Piece, sq: Square) -> usize {
    sq as usize + Square::NUM * (piece as usize + Piece::NUM * color as usize)
}
