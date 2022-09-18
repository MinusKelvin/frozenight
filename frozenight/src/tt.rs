use std::sync::atomic::{AtomicU64, Ordering};

use bytemuck::{Pod, Zeroable};
use cozy_chess::{Board, Move, Piece, Square};

use crate::position::Position;
use crate::Eval;

pub struct TranspositionTable {
    entries: Box<[TtEntry]>,
    search_number: u8,
}

const ENTRIES_PER_MB: usize = 1024 * 1024 / std::mem::size_of::<TtEntry>();

impl TranspositionTable {
    pub fn new(hash_mb: usize) -> Self {
        assert!(hash_mb > 0);
        TranspositionTable {
            entries: (0..hash_mb * ENTRIES_PER_MB)
                .map(|_| TtEntry::default())
                .collect(),
            search_number: 2,
        }
    }

    fn entry(&self, hash: u64) -> &TtEntry {
        unsafe {
            // SAFETY: This is a fixed-point multiply of `self.entries.len()` by hash/2^64.
            //         Since `hash` is in 0..1 and does not include 1, the result cannot overflow
            //         and also cannot exceed `self.entries.len()` and therefore is in-bounds.
            let index = hash as u128 * self.entries.len() as u128 >> 64;
            self.entries.get_unchecked(index as usize)
        }
    }

    pub fn get_move(&self, board: &Board) -> Option<Move> {
        let entry = self.entry(board.hash());
        let data = entry.data.load(Ordering::Relaxed);
        let hxd = entry.hash.load(Ordering::Relaxed);
        if hxd ^ data != board.hash() {
            return None;
        }
        let data: TtData = bytemuck::cast(data);
        data.unmarshall_move(board)
    }

    pub fn get(&self, position: &Position) -> Option<TableEntry> {
        let entry = self.entry(position.board.hash());
        let data = entry.data.load(Ordering::Relaxed);
        let hxd = entry.hash.load(Ordering::Relaxed);
        if hxd ^ data != position.board.hash() {
            return None;
        }
        // marshal between usable type and stored data
        // also validates the data
        let data: TtData = bytemuck::cast(data);

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
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            _mm_prefetch(
                self.entry(board.hash()) as *const _ as *const _,
                _MM_HINT_T0,
            );
        }
    }

    pub fn store(&self, position: &Position, data: TableEntry) {
        let entry = self.entry(position.board.hash());

        let old_data = entry.data.load(Ordering::Relaxed);
        let old_hash = entry.hash.load(Ordering::Relaxed) ^ old_data;
        let old_data: TtData = bytemuck::cast(old_data);

        let mut replace = false;
        // always replace existing position data with PV data
        replace |= old_hash == position.board.hash() && data.kind == NodeKind::Exact;
        // prefer deeper data
        replace |= data.depth >= old_data.depth;
        // prefer replacing stale data
        replace |= self.search_number.wrapping_sub(old_data.age) >= 2;

        if !replace {
            return;
        }

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
            age: self.search_number,
        });
        entry.data.store(data, Ordering::Relaxed);
        entry
            .hash
            .store(position.board.hash() ^ data, Ordering::Relaxed);
    }

    pub fn increment_age(&mut self, by: u8) {
        self.search_number = self.search_number.wrapping_add(by);
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
struct TtEntry {
    hash: AtomicU64,
    data: AtomicU64,
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
