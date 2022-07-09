use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use bytemuck::{Pod, Zeroable};
use cozy_chess::{Board, Move, Piece, Square};

use crate::position::Position;
use crate::Eval;

pub struct TranspositionTable {
    entries: Box<[TtBucket]>,
    search_number: AtomicU8,
}

const ENTRIES_PER_MB: usize = 1024 * 1024 / std::mem::size_of::<TtBucket>();

impl TranspositionTable {
    pub fn new(hash_mb: usize) -> Self {
        TranspositionTable {
            entries: (0..hash_mb * ENTRIES_PER_MB)
                .map(|_| TtBucket::default())
                .collect(),
            search_number: AtomicU8::default(),
        }
    }

    pub fn get_move(&self, board: &Board) -> Option<Move> {
        let bucket = &self.entries[board.hash() as usize % self.entries.len()];
        bucket.find(board.hash())?.unmarshall_move(board)
    }

    pub fn get(&self, position: &Position) -> Option<TableEntry> {
        let bucket = &self.entries[position.board.hash() as usize % self.entries.len()];
        let data = bucket.find(position.board.hash())?;

        // marshal between usable type and stored data
        // also validates the data
        let kind = match data.kind {
            0 => NodeKind::Exact,
            1 => NodeKind::LowerBound,
            2 => NodeKind::UpperBound,
            _ => return None, // invalid
        };

        let mv = data.unmarshall_move(&position.board)?;

        Some(TableEntry {
            mv,
            kind,
            eval: data.eval.add_time(position.ply),
            depth: data.depth,
        })
    }

    pub fn prefetch(&self, board: &Board) {
        let index = board.hash() as usize % self.entries.len();
        let entry = &self.entries[index];
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            _mm_prefetch(entry as *const _ as *const _, _MM_HINT_T0);
        }
    }

    pub fn store(&self, position: &Position, data: TableEntry) {
        let bucket = &self.entries[position.board.hash() as usize % self.entries.len()];

        let age = self.search_number.load(Ordering::Relaxed);
        let entry = match bucket.replace(position.board.hash(), &data, age) {
            Some(v) => v,
            None => return,
        };

        let upper_hash = (position.board.hash() >> 32) as u32;
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
            eval: data.eval.sub_time(position.ply),
            depth: data.depth,
            kind: data.kind as u8,
            age,
        });
        entry.data_ptr.store(data, Ordering::Relaxed);
        entry
            .hash_ptr
            .store(upper_hash ^ data as u32, Ordering::Relaxed);
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

#[derive(Default)]
struct TtBucket {
    data: [AtomicU64; 5],
    hash: [AtomicU32; 5],
}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct TtData {
    mv: u16,
    eval: Eval,
    depth: i16,
    kind: u8,
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

struct Entry<'a> {
    data_ptr: &'a AtomicU64,
    hash_ptr: &'a AtomicU32,
    data: TtData,
    upper_hash: u32,
}

impl TtBucket {
    fn entries(&self) -> impl Iterator<Item = Entry> + '_ {
        self.data
            .iter()
            .zip(self.hash.iter())
            .map(|(data_ptr, hash_ptr)| {
                let data = data_ptr.load(Ordering::Relaxed);
                let hash = hash_ptr.load(Ordering::Relaxed);
                let upper_hash = hash ^ data as u32;
                Entry {
                    data_ptr,
                    hash_ptr,
                    data: bytemuck::cast(data),
                    upper_hash,
                }
            })
    }

    fn find(&self, hash: u64) -> Option<TtData> {
        let upper_hash = (hash >> 32) as u32;
        for entry in self.entries() {
            if entry.upper_hash == upper_hash {
                return Some(entry.data);
            }
        }
        None
    }

    fn replace(&self, hash: u64, data: &TableEntry, age: u8) -> Option<Entry> {
        let upper_hash = (hash >> 32) as u32;

        let entry = self
            .entries()
            .min_by_key(|entry| {
                if entry.upper_hash == upper_hash {
                    return i16::MIN;
                }
                let age_score = match age.wrapping_sub(entry.data.age) {
                    0 | 1 => 0,
                    v => v as i16 * -64,
                };
                let depth_score = entry.data.depth * 2;
                const NODE_KIND_EXACT: u8 = NodeKind::Exact as u8;
                let node_type_score = match entry.data.kind {
                    NODE_KIND_EXACT => 5,
                    _ => 0,
                };
                age_score + depth_score + node_type_score
            })
            .unwrap();

        if entry.upper_hash != upper_hash
            || data.kind == NodeKind::Exact
            || data.depth >= entry.data.depth
            || age.wrapping_sub(entry.data.age) >= 2
        {
            Some(entry)
        } else {
            None
        }
    }
}
