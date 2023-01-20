use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square};

use crate::Eval;

const NUM_FEATURES: usize = Color::NUM * Piece::NUM * Square::NUM;
const L1_SIZE: usize = 768;
const BUCKETS: usize = 16;

static NETWORK: Nnue = include!(concat!(env!("OUT_DIR"), "/model.rs"));

struct Nnue {
    input_layer: [[i16; L1_SIZE]; NUM_FEATURES],
    input_layer_bias: [i16; L1_SIZE],
    hidden_layer: [[i8; L1_SIZE * 2]; BUCKETS],
    hidden_layer_bias: [i32; BUCKETS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NnueAccumulator {
    white: [i16; L1_SIZE],
    black: [i16; L1_SIZE],
    material: usize,
}

impl NnueAccumulator {
    pub fn new(board: &Board) -> Self {
        let mut white = NETWORK.input_layer_bias;
        let mut black = NETWORK.input_layer_bias;
        for p in Piece::ALL {
            for sq in board.pieces(p) {
                let color = match board.colors(Color::White).has(sq) {
                    true => Color::White,
                    false => Color::Black,
                };
                vadd(&mut white, &NETWORK.input_layer[feature(color, p, sq)]);
                vadd(
                    &mut black,
                    &NETWORK.input_layer[feature(!color, p, sq.flip_rank())],
                );
            }
        }
        NnueAccumulator {
            white,
            black,
            material: board.pieces(Piece::Pawn).len() as usize
                + 3 * board.pieces(Piece::Bishop).len() as usize
                + 3 * board.pieces(Piece::Knight).len() as usize
                + 5 * board.pieces(Piece::Rook).len() as usize
                + 8 * board.pieces(Piece::Queen).len() as usize,
        }
    }

    pub fn calculate(&self, stm: Color) -> Eval {
        let bucket = (self.material * BUCKETS / 76).min(BUCKETS - 1);
        let mut output = NETWORK.hidden_layer_bias[bucket] * 127;
        let (first, second) = match stm {
            Color::White => (&self.white, &self.black),
            Color::Black => (&self.black, &self.white),
        };
        for i in 0..first.len() {
            output += activate(first[i]) * NETWORK.hidden_layer[bucket][i] as i32;
        }
        for i in 0..second.len() {
            output += activate(second[i]) * NETWORK.hidden_layer[bucket][i + first.len()] as i32;
        }

        Eval::new((output / 127 / 8) as i16)
    }

    pub fn play_move(&self, board: &Board, mv: Move) -> Self {
        let mut result = *self;

        let us = board.side_to_move();
        let moved = board.piece_on(mv.from).unwrap();

        if board.colors(!us).has(mv.to) {
            result.material -= match board.piece_on(mv.to) {
                Some(Piece::Pawn) => 1,
                Some(Piece::Bishop) => 3,
                Some(Piece::Knight) => 3,
                Some(Piece::Rook) => 5,
                Some(Piece::Queen) => 8,
                _ => unreachable!(),
            };
        }

        match mv.promotion {
            Some(Piece::Bishop) => result.material += 2,
            Some(Piece::Knight) => result.material += 2,
            Some(Piece::Rook) => result.material += 4,
            Some(Piece::Queen) => result.material += 7,
            _ => {}
        }

        // remove piece on from square
        vsub(
            &mut result.white,
            &NETWORK.input_layer[feature(us, moved, mv.from)],
        );
        vsub(
            &mut result.black,
            &NETWORK.input_layer[feature(!us, moved, mv.from.flip_rank())],
        );

        // remove piece on to square
        if let Some((color, piece)) = board.color_on(mv.to).zip(board.piece_on(mv.to)) {
            vsub(
                &mut result.white,
                &NETWORK.input_layer[feature(color, piece, mv.to)],
            );
            vsub(
                &mut result.black,
                &NETWORK.input_layer[feature(!color, piece, mv.to.flip_rank())],
            )
        }

        // remove EP-captured pawn
        if let Some(ep_file) = board.en_passant() {
            if moved == Piece::Pawn && mv.to == Square::new(ep_file, Rank::Sixth.relative_to(us)) {
                vsub(
                    &mut result.white,
                    &NETWORK.input_layer[feature(
                        !us,
                        Piece::Pawn,
                        Square::new(ep_file, Rank::Fifth.relative_to(us)),
                    )],
                );
                vsub(
                    &mut result.black,
                    &NETWORK.input_layer[feature(
                        us,
                        Piece::Pawn,
                        Square::new(ep_file, Rank::Fifth.relative_to(!us)),
                    )],
                );
                result.material -= 1;
            }
        }

        if Some(us) == board.color_on(mv.to) {
            let rank = Rank::First.relative_to(us);
            if mv.from.file() > mv.to.file() {
                // castle queen-side
                vadd(
                    &mut result.white,
                    &NETWORK.input_layer[feature(us, Piece::King, Square::new(File::C, rank))],
                );
                vadd(
                    &mut result.white,
                    &NETWORK.input_layer[feature(us, Piece::Rook, Square::new(File::D, rank))],
                );
                vadd(
                    &mut result.black,
                    &NETWORK.input_layer
                        [feature(!us, Piece::King, Square::new(File::C, rank.flip()))],
                );
                vadd(
                    &mut result.black,
                    &NETWORK.input_layer
                        [feature(!us, Piece::Rook, Square::new(File::D, rank.flip()))],
                );
            } else {
                // castle king-side
                vadd(
                    &mut result.white,
                    &NETWORK.input_layer[feature(us, Piece::King, Square::new(File::G, rank))],
                );
                vadd(
                    &mut result.white,
                    &NETWORK.input_layer[feature(us, Piece::Rook, Square::new(File::F, rank))],
                );
                vadd(
                    &mut result.black,
                    &NETWORK.input_layer
                        [feature(!us, Piece::King, Square::new(File::G, rank.flip()))],
                );
                vadd(
                    &mut result.black,
                    &NETWORK.input_layer
                        [feature(!us, Piece::Rook, Square::new(File::F, rank.flip()))],
                );
            }
        } else {
            let added = mv.promotion.unwrap_or(moved);
            vadd(
                &mut result.white,
                &NETWORK.input_layer[feature(us, added, mv.to)],
            );
            vadd(
                &mut result.black,
                &NETWORK.input_layer[feature(!us, added, mv.to.flip_rank())],
            );
        }

        result
    }
}

fn activate(v: i16) -> i32 {
    let v = v as i32;
    let v = v.clamp(0, 127);
    v * v
}

fn vadd<const N: usize>(a: &mut [i16; N], b: &[i16; N]) {
    a.iter_mut().zip(b.iter()).for_each(|(a, &b)| *a += b);
}

fn vsub<const N: usize>(a: &mut [i16; N], b: &[i16; N]) {
    a.iter_mut().zip(b.iter()).for_each(|(a, &b)| *a -= b);
}

fn feature(color: Color, piece: Piece, sq: Square) -> usize {
    sq as usize + Square::NUM * (piece as usize + Piece::NUM * color as usize)
}
