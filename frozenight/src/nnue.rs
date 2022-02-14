use cozy_chess::{BitBoard, Board, Color, Piece, Square};
use rand::prelude::*;

use crate::Eval;

type Vector = [i32; 16];

pub struct Nnue {
    input_layer: [[[Vector; Square::NUM]; Piece::NUM]; Color::NUM],
    input_layer_bias: Vector,
    hidden_layer: Vector,
    hidden_layer_bias: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct NnueAccumulator {
    inputs: [Vector; Color::NUM],

    // active features
    colors: [BitBoard; Color::NUM],
    pieces: [BitBoard; Piece::NUM],
}

impl Nnue {
    pub fn new() -> Nnue {
        Nnue {
            input_layer: [(); 2].map(|_| {
                [(); 6]
                    .map(|_| [(); 64].map(|_| [(); 16].map(|_| thread_rng().gen_range(-128..128))))
            }),
            input_layer_bias: [(); 16].map(|_| thread_rng().gen_range(-128..128)),
            hidden_layer: [(); 16].map(|_| thread_rng().gen_range(-128..128)),
            hidden_layer_bias: thread_rng().gen_range(-128..128),
        }
    }
}

impl NnueAccumulator {
    pub fn new(nn: &Nnue) -> Self {
        NnueAccumulator {
            inputs: [nn.input_layer_bias; Color::NUM],
            colors: [BitBoard::EMPTY; Color::NUM],
            pieces: [BitBoard::EMPTY; Piece::NUM],
        }
    }

    pub fn calculate(&mut self, nn: &Nnue, board: &Board) -> Eval {
        let mut new_colors = [BitBoard::EMPTY; Color::NUM];
        for color in Color::ALL {
            new_colors[color as usize] = board.colors(color);
        }
        let mut new_pieces = [BitBoard::EMPTY; Piece::NUM];
        for piece in Piece::ALL {
            new_pieces[piece as usize] = board.pieces(piece);
        }

        // TODO: there *has* to be a better way of diffing feature sets
        for piece in Piece::ALL {
            for color in Color::ALL {
                let previous = self.colors[color as usize] & self.pieces[piece as usize];
                let new = new_colors[color as usize] & new_pieces[piece as usize];
                for sq in previous & !new {
                    self.inputs[Color::White as usize] = vsub(
                        self.inputs[Color::White as usize],
                        nn.input_layer[color as usize][piece as usize][sq as usize],
                    );
                    self.inputs[Color::Black as usize] = vsub(
                        self.inputs[Color::Black as usize],
                        nn.input_layer[(!color) as usize][piece as usize][sq.flip_rank() as usize],
                    );
                }
                for sq in new & !previous {
                    self.inputs[Color::White as usize] = vadd(
                        self.inputs[Color::White as usize],
                        nn.input_layer[color as usize][piece as usize][sq as usize],
                    );
                    self.inputs[Color::Black as usize] = vadd(
                        self.inputs[Color::Black as usize],
                        nn.input_layer[(!color) as usize][piece as usize][sq.flip_rank() as usize],
                    );
                }
            }
        }

        self.colors = new_colors;
        self.pieces = new_pieces;

        let clipped = clipped_relu(self.inputs[board.side_to_move() as usize]);
        let output = vdot(clipped, nn.hidden_layer);

        Eval::new((nn.hidden_layer_bias + output) as i16)
    }
}

fn vadd(a: Vector, b: Vector) -> Vector {
    let mut result = Vector::default();
    for i in 0..result.len() {
        result[i] = a[i] + b[i];
    }
    result
}

fn vsub(a: Vector, b: Vector) -> Vector {
    let mut result = Vector::default();
    for i in 0..result.len() {
        result[i] = a[i] - b[i];
    }
    result
}

fn clipped_relu(a: Vector) -> Vector {
    a.map(|v| v.clamp(0, 127))
}

fn vdot(a: Vector, b: Vector) -> i32 {
    let mut result = 0;
    for (&a, &b) in a.iter().zip(b.iter()) {
        result += (a * b) / 64;
    }
    result
}
