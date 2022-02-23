use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use cozy_chess::{
    get_bishop_rays, get_knight_moves, get_rook_rays, Board, Move, Piece, Rank, Square,
};
use once_cell::sync::Lazy;

use crate::position::Position;
use crate::{Eval, INVALID_MOVE};

pub struct TranspositionTable {
    entries: Box<[TtEntry]>,
}

const ENTRIES_PER_MB: usize = 1024 * 1024 / std::mem::size_of::<TtEntry>();

impl TranspositionTable {
    pub fn new(hash_mb: usize) -> Self {
        TranspositionTable {
            entries: (0..hash_mb * ENTRIES_PER_MB)
                .map(|_| TtEntry::default())
                .collect(),
        }
    }

    pub fn get_move(&self, board: &Board) -> Option<Move> {
        let entry = &self.entries[board.hash() as usize % self.entries.len()];
        entry.get_move(board)
    }

    pub fn get_eval(&self, position: &Position) -> Option<TableEntry> {
        let hash = position.board.hash();
        let entry = &self.entries[hash as usize % self.entries.len()];
        entry.get_eval(hash, position.ply)
    }

    pub fn store(&self, position: &Position, data: TableEntry, mv: Move) {
        let hash = position.board.hash();
        let entry = &self.entries[hash as usize % self.entries.len()];
        entry.set_eval(hash, position.ply, data);
        entry.set_move(hash, mv)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TableEntry {
    pub eval: Eval,
    pub depth: u16,
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
    /// Layout:
    /// `HHHHHHHH_HHHHHHHH_HHHHHHHH_HHHHHHHH_HHHHHHKK_DDDDDDDD_EEEEEEEE_EEEEEEEE`
    /// - `H`: Hash (top 38 bits)
    /// - `K`: Node kind (0 = exact, 1 = lb, 2 = ub)
    /// - `D`: Depth
    /// - `E`: Eval
    eval: AtomicU64,
    /// Layout:
    /// `HHHH_HMMM_MMMM_MMMM`
    /// - `H`: Hash (top 5 bits)
    /// - `M`: index into move table
    moves: [AtomicU16; 4],
}

impl TtEntry {
    fn get_move(&self, board: &Board) -> Option<Move> {
        let hash = board.hash();
        // top 2 bits of hash are used as the index into the subarray
        let mv_data = self.moves[(hash >> 62) as usize].load(Ordering::Relaxed);
        // shift down so that the next 5 bits are in the same spot as they are in the move data
        let check = (hash >> 46) as u16 & MV_HASH_MASK;
        (check == mv_data & MV_HASH_MASK)
            .then(|| MOVES_TABLES.0[(mv_data & MV_DATA_MASK) as usize])
            .filter(|&mv| board.is_legal(mv))
    }

    fn set_move(&self, hash: u64, mv: Move) {
        let mut mv_data = 0;
        mv_data |= MOVES_TABLES.1[mv.from as usize][mv.to as usize]
            + mv.promotion.map_or(0, |piece| piece as u16);
        mv_data |= (hash >> 46) as u16 & MV_HASH_MASK;
        self.moves[(hash >> 62) as usize].store(mv_data, Ordering::Relaxed);
    }

    fn get_eval(&self, hash: u64, ply: u16) -> Option<TableEntry> {
        let eval_data = self.eval.load(Ordering::Relaxed);
        (eval_data & HASH_MASK == hash & HASH_MASK).then(|| {
            let eval: Eval = bytemuck::cast(((eval_data & EVAL_MASK) >> EVAL_OFFSET) as u16);
            let eval = eval.add_time(ply);
            let depth = ((eval_data & DEPTH_MASK) >> DEPTH_OFFSET) as u16;
            let kind = match (eval_data & NODE_KIND_MASK) >> NODE_KIND_OFFSET {
                0 => NodeKind::Exact,
                1 => NodeKind::LowerBound,
                2 => NodeKind::UpperBound,
                _ => unreachable!(),
            };
            TableEntry { eval, depth, kind }
        })
    }

    fn set_eval(&self, hash: u64, ply: u16, entry: TableEntry) {
        let mut data = 0;
        data |= hash & HASH_MASK;
        data |= (entry.kind as u64) << NODE_KIND_OFFSET;
        data |= (entry.depth.min(255) as u64) << DEPTH_OFFSET;
        data |= bytemuck::cast::<_, u16>(entry.eval.sub_time(ply)) as u64;
        self.eval.store(data, Ordering::Relaxed);
    }
}

const EVAL_OFFSET: usize = 0;
const DEPTH_OFFSET: usize = 16;
const NODE_KIND_OFFSET: usize = 24;
const HASH_OFFSET: usize = 26;

const EVAL_MASK: u64 = (1 << DEPTH_OFFSET) - (1 << 0);
const DEPTH_MASK: u64 = (1 << NODE_KIND_OFFSET) - (1 << DEPTH_OFFSET);
const NODE_KIND_MASK: u64 = (1 << HASH_OFFSET) - (1 << NODE_KIND_OFFSET);
const HASH_MASK: u64 = !((1 << HASH_OFFSET) - 1);

const MV_DATA_MASK: u16 = (1 << 11) - 1;
const MV_HASH_MASK: u16 = !MV_DATA_MASK;

static MOVES_TABLES: Lazy<([Move; 2048], [[u16; Square::NUM]; Square::NUM])> = Lazy::new(|| {
    let mut idx_to_move = [INVALID_MOVE; 2048];
    let mut mv_to_idx = [[2047; Square::NUM]; Square::NUM];

    let mut idx = 0;
    let mut put = |mv: Move| {
        idx_to_move[idx as usize] = mv;
        mv_to_idx[mv.from as usize][mv.to as usize] = idx;
        idx += 1;
    };

    for from in Square::ALL {
        let possible_moves = get_knight_moves(from) | get_bishop_rays(from) | get_rook_rays(from);

        for to in possible_moves {
            put(Move {
                from,
                to,
                promotion: None,
            });
            if (from.rank() == Rank::Second && to.rank() == Rank::First
                || from.rank() == Rank::Seventh && to.rank() == Rank::Eighth)
                && (from.file() as i8 - to.file() as i8).abs() < 2
            {
                for promo in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
                    put(Move {
                        from,
                        to,
                        promotion: Some(promo),
                    });
                }
            }
        }
    }

    (idx_to_move, mv_to_idx)
});
