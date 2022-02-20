use std::sync::atomic::{AtomicU64, Ordering};

use bytemuck::{Pod, Zeroable};
use cozy_chess::{Board, Move, Piece, Square};

use crate::Eval;

pub struct TranspositionTable {
    entries: Box<[TtEntry]>,
}

const ENTRIES_PER_MB: usize = 1024 * 1024/ std::mem::size_of::<TtEntry>();

impl TranspositionTable {
    pub fn new(hash_mb: usize) -> Self {
        TranspositionTable {
            entries: (0..hash_mb * ENTRIES_PER_MB)
                .map(|_| TtEntry::default())
                .collect(),
        }
    }

    pub fn get(&self, board: &Board) -> Option<TableEntry> {
        let index = board.hash() as usize % self.entries.len();
        let data = self.entries[index].data.load(Ordering::Relaxed);
        let hxd = self.entries[index].hash.load(Ordering::Relaxed);
        if hxd ^ data != board.hash() {
            return None;
        }
        // marshal between usable type and stored data
        // also validates the data
        let data: TtData = bytemuck::cast(data);
        Some(TableEntry {
            mv: Move {
                from: Square::index(data.mv as usize & 0x3F),
                to: Square::index(data.mv as usize >> 6 & 0x3F),
                promotion: match data.mv as usize >> 12 {
                    0 => None,
                    1 => Some(Piece::Knight),
                    2 => Some(Piece::Bishop),
                    3 => Some(Piece::Rook),
                    4 => Some(Piece::Queen),
                    _ => return None, // invalid
                },
            },
            kind: match data.kind {
                0 => NodeKind::Exact,
                1 => NodeKind::LowerBound,
                2 => NodeKind::UpperBound,
                _ => return None, // invalid
            },
            eval: data.eval,
            search_depth: data.depth,
        })
    }

    pub fn store(&self, board: &Board, data: TableEntry) {
        let index = board.hash() as usize % self.entries.len();
        let promo = match data.mv.promotion {
            None => 0,
            Some(Piece::Knight) => 1,
            Some(Piece::Bishop) => 2,
            Some(Piece::Rook) => 3,
            Some(Piece::Queen) => 4,
            _ => unreachable!(),
        };
        let data = bytemuck::cast(TtData {
            mv: data.mv.from as u16 | (data.mv.to as u16) << 6 | promo << 12,
            eval: data.eval,
            depth: data.search_depth,
            kind: data.kind as u8,
            _padding: 0,
        });
        self.entries[index].data.store(data, Ordering::Relaxed);
        self.entries[index].hash.store(board.hash() ^ data, Ordering::Relaxed);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TableEntry {
    pub mv: Move,
    pub eval: Eval,
    pub search_depth: u16,
    pub kind: NodeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Default)]
struct TtEntry {
    hash: AtomicU64,
    data: AtomicU64,
}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct TtData {
    mv: u16,
    eval: Eval,
    depth: u16,
    kind: u8,
    _padding: u8,
}
