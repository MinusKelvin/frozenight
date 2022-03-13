use cozy_chess::*;
use serde::{Serialize, Deserialize};

// CITE: Mobility evaluation.
// https://www.chessprogramming.org/Mobility
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Mobility<E> {
    pub pawn: [E; 5],
    pub knight: [E; 9],
    pub bishop: [E; 14],
    pub rook: [E; 15],
    pub queen: [E; 28],
    pub king: [E; 9]
}

impl<E> Mobility<E> {
    pub fn get(&self, piece: Piece) -> &[E] {
        match piece {
            Piece::Pawn => &self.pawn,
            Piece::Knight => &self.knight,
            Piece::Bishop => &self.bishop,
            Piece::Rook => &self.rook,
            Piece::Queen => &self.queen,
            Piece::King => &self.king
        }
    }

    pub fn get_mut(&mut self, piece: Piece) -> &mut [E] {
        match piece {
            Piece::Pawn => &mut self.pawn,
            Piece::Knight => &mut self.knight,
            Piece::Bishop => &mut self.bishop,
            Piece::Rook => &mut self.rook,
            Piece::Queen => &mut self.queen,
            Piece::King => &mut self.king
        }
    }
}
