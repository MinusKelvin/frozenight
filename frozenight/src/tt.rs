use std::sync::atomic::{AtomicU8, Ordering};

use cozy_chess::{Board, Move, Piece, Square};

use crate::position::Position;
use crate::Eval;

pub struct TranspositionTable {
    entries: Box<[TtEntry]>,
    search_number: AtomicU8,
}

const ENTRIES_PER_MB: usize = 1024 * 1024 / std::mem::size_of::<TtEntry>();

impl TranspositionTable {
    pub fn new(hash_mb: usize) -> Self {
        TranspositionTable {
            entries: (0..hash_mb * ENTRIES_PER_MB)
                .map(|_| TtEntry::default())
                .collect(),
            search_number: AtomicU8::default(),
        }
    }

    pub fn get_move(&self, board: &Board) -> Option<Move> {
        let index = board.hash() as usize % self.entries.len();
        self.entries[index]
            .lock()
            .find(board.hash())
            .and_then(|data| data.unmarshall_move(board))
    }

    pub fn get(&self, position: &Position) -> Option<TableEntry> {
        let index = position.board.hash() as usize % self.entries.len();
        let data = *self.entries[index].lock().find(position.board.hash())?;

        let mv = data.unmarshall_move(&position.board)?;

        Some(TableEntry {
            mv,
            kind: data.kind,
            eval: data.eval.add_time(position.ply),
            depth: data.depth,
        })
    }

    pub fn store(&self, position: &Position, data: TableEntry) {
        let index = position.board.hash() as usize % self.entries.len();
        let mut bucket = self.entries[index].lock();

        let age = self.search_number.load(Ordering::Relaxed);
        let entry = match bucket.replace(position.board.hash(), &data, age) {
            Some(v) => v,
            None => return,
        };

        let promo = match data.mv.promotion {
            None => 0,
            Some(Piece::Knight) => 1,
            Some(Piece::Bishop) => 2,
            Some(Piece::Rook) => 3,
            Some(Piece::Queen) => 4,
            _ => unreachable!(),
        };

        *entry = TtData {
            upper_hash: (position.board.hash() >> 32) as u32,
            mv: data.mv.from as u16 | (data.mv.to as u16) << 6 | promo << 12,
            eval: data.eval.sub_time(position.ply),
            depth: data.depth,
            kind: data.kind,
            age,
        };
    }

    pub fn increment_age(&self, by: u8) {
        self.search_number.fetch_add(by, Ordering::Relaxed);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TableEntry {
    pub mv: Move,
    pub eval: Eval,
    pub depth: i16,
    pub kind: NodeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Exact,
    LowerBound,
    UpperBound,
}

type TtEntry = parking_lot::Mutex<TtBucket>;

#[derive(Default)]
struct TtBucket([TtData; 5]);

#[derive(Copy, Clone)]
struct TtData {
    upper_hash: u32,
    mv: u16,
    eval: Eval,
    depth: i16,
    kind: NodeKind,
    age: u8,
}

impl TtData {
    fn unmarshall_move(&self, board: &Board) -> Option<Move> {
        let mv = Move {
            from: Square::index(self.mv as usize & 0x3F),
            to: Square::index(self.mv as usize >> 6 & 0x3F),
            promotion: match self.mv as usize >> 12 {
                0 => None,
                1 => Some(Piece::Knight),
                2 => Some(Piece::Bishop),
                3 => Some(Piece::Rook),
                4 => Some(Piece::Queen),
                _ => return None, // invalid
            },
        };

        board.is_legal(mv).then(|| mv)
    }
}

impl TtBucket {
    fn find(&mut self, hash: u64) -> Option<&mut TtData> {
        let upper_hash = (hash >> 32) as u32;
        self.0.iter_mut().find(|data| data.upper_hash == upper_hash)
    }

    fn replace(&mut self, hash: u64, data: &TableEntry, age: u8) -> Option<&mut TtData> {
        let upper_hash = (hash >> 32) as u32;

        let entry = self
            .0
            .iter_mut()
            .min_by_key(|entry| {
                if entry.upper_hash == upper_hash {
                    return i16::MIN;
                }
                let age_score = match age.wrapping_sub(entry.age) {
                    0 | 1 => 0,
                    v => v as i16 * -64,
                };
                let depth_score = entry.depth * 2;
                let node_type_score = match entry.kind {
                    NodeKind::Exact => 5,
                    _ => 0,
                };
                age_score + depth_score + node_type_score
            })
            .unwrap();

        if entry.upper_hash != upper_hash
            || data.kind == NodeKind::Exact
            || data.depth >= entry.depth
            || age.wrapping_sub(entry.age) >= 2
        {
            Some(entry)
        } else {
            None
        }
    }
}

impl Default for TtData {
    fn default() -> Self {
        Self {
            upper_hash: 0,
            mv: 0,
            eval: Eval::DRAW,
            depth: 0,
            kind: NodeKind::Exact,
            age: 0,
        }
    }
}
