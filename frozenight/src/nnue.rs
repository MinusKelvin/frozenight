use cozy_chess::{BitBoard, Board, Color, Piece, Square};

use crate::Eval;

type Vector = [i32; 16];

const NUM_FEATURES: usize = Color::NUM * Piece::NUM * Square::NUM;

pub struct Nnue {
    input_layer: [Vector; NUM_FEATURES],
    input_layer_bias: Vector,
    hidden_layer: [i32; 32],
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
        include!("../model.rs")
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
        self.update_features(nn, board);

        let l1_input = bytemuck::cast([
            clipped_relu(self.inputs[board.side_to_move() as usize]),
            clipped_relu(self.inputs[!board.side_to_move() as usize]),
        ]);
        let output = vdot(l1_input, nn.hidden_layer);

        Eval::new((nn.hidden_layer_bias + output) as i16)
    }

    fn update_features(&mut self, nn: &Nnue, board: &Board) {
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
                        nn.input_layer[feature(color, piece, sq)],
                    );
                    self.inputs[Color::Black as usize] = vsub(
                        self.inputs[Color::Black as usize],
                        nn.input_layer[feature(!color, piece, sq.flip_rank())],
                    );
                }
                for sq in new & !previous {
                    self.inputs[Color::White as usize] = vadd(
                        self.inputs[Color::White as usize],
                        nn.input_layer[feature(color, piece, sq)],
                    );
                    self.inputs[Color::Black as usize] = vadd(
                        self.inputs[Color::Black as usize],
                        nn.input_layer[feature(!color, piece, sq.flip_rank())],
                    );
                }
            }
        }

        self.colors = new_colors;
        self.pieces = new_pieces;
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
