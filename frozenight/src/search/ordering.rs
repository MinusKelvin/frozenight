use cozy_chess::{Board, Move, Piece, Square};

pub struct MoveOrdering<'a> {
    board: &'a Board,
    stage: MoveOrderingStage,
    hashmove: Option<Move>,
    killer: Move,
    captures: Vec<(Move, i8)>,
    yielded_quiets: usize,
    quiets: Vec<(Move, i32)>,
    underpromotions: Vec<Move>,
}

#[derive(Clone, Copy)]
enum MoveOrderingStage {
    Hashmove,
    GenerateMoves,
    Captures,
    Quiets,
    Underpromotions,
}

const PIECE_ORDINALS: [i8; Piece::NUM] = [0, 1, 1, 2, 3, 4];

impl<'a> MoveOrdering<'a> {
    pub fn new(board: &'a Board, hashmove: Option<Move>, killer: Move) -> Self {
        MoveOrdering {
            board,
            stage: match hashmove {
                Some(_) => MoveOrderingStage::Hashmove,
                None => MoveOrderingStage::GenerateMoves,
            },
            hashmove,
            killer,
            yielded_quiets: 0,
            captures: vec![],
            quiets: vec![],
            underpromotions: vec![],
        }
    }

    pub fn yielded_quiets(&self) -> impl Iterator<Item = Move> + '_ {
        self.quiets[..self.yielded_quiets].iter().map(|&(mv, _)| mv)
    }

    pub fn next(&mut self, history: &HistoryTable) -> Option<Move> {
        match self.stage {
            MoveOrderingStage::Hashmove => self.hashmove(),
            MoveOrderingStage::GenerateMoves => self.generate_moves(history),
            MoveOrderingStage::Captures => self.captures(),
            MoveOrderingStage::Quiets => self.quiets(),
            MoveOrderingStage::Underpromotions => self.underpromotions(),
        }
    }

    fn hashmove(&mut self) -> Option<Move> {
        self.stage = MoveOrderingStage::GenerateMoves;
        self.hashmove
    }

    fn generate_moves(&mut self, history: &HistoryTable) -> Option<Move> {
        self.stage = MoveOrderingStage::Captures;
        self.captures.reserve(16);
        self.quiets.reserve(64);
        self.board.generate_moves(|mvs| {
            for mv in mvs {
                if Some(mv) == self.hashmove {
                    continue;
                }
                if matches!(
                    mv.promotion,
                    Some(Piece::Knight | Piece::Bishop | Piece::Rook)
                ) {
                    self.underpromotions.push(mv);
                    continue;
                }

                match self.board.piece_on(mv.to) {
                    Some(victim) => {
                        let attacker = PIECE_ORDINALS[mvs.piece as usize];
                        let victim = PIECE_ORDINALS[victim as usize] * 4;
                        self.captures.push((mv, victim - attacker));
                    }
                    _ if mv == self.killer => {
                        // Killer is legal; give it the same rank as PxP
                        self.captures.push((mv, 0));
                    }
                    _ => {
                        self.quiets.push((mv, history.rank(mvs.piece, mv)));
                    }
                }
            }
            false
        });
        self.captures()
    }

    fn captures(&mut self) -> Option<Move> {
        if self.captures.is_empty() {
            self.stage = MoveOrderingStage::Quiets;
            return self.quiets();
        }

        let mut index = 0;
        for i in 1..self.captures.len() {
            if self.captures[i].1 > self.captures[index].1 {
                index = i;
            }
        }

        Some(self.captures.swap_remove(index).0)
    }

    fn quiets(&mut self) -> Option<Move> {
        if self.yielded_quiets == self.quiets.len() {
            self.stage = MoveOrderingStage::Underpromotions;
            return self.underpromotions();
        }

        let mut index = self.yielded_quiets;
        for i in index + 1..self.quiets.len() {
            if self.quiets[i].1 > self.quiets[index].1 {
                index = i;
            }
        }

        self.quiets.swap(self.yielded_quiets, index);
        let r = Some(self.quiets[self.yielded_quiets].0);
        self.yielded_quiets += 1;
        r
    }

    fn underpromotions(&mut self) -> Option<Move> {
        self.underpromotions.pop()
    }
}

pub struct HistoryTable {
    to_sq: [[(i32, i32); Square::NUM]; Piece::NUM],
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            to_sq: [[(0, 0); Square::NUM]; Piece::NUM],
        }
    }

    pub fn caused_cutoff(&mut self, piece: Piece, mv: Move) {
        let (average, total) = &mut self.to_sq[piece as usize][mv.to as usize];
        let diff = 2_000_000_000 - *average;
        *total += 1;
        *average += diff / *total;
    }

    pub fn did_not_cause_cutoff(&mut self, piece: Piece, mv: Move) {
        let (average, total) = &mut self.to_sq[piece as usize][mv.to as usize];
        *total += 1;
        *average -= *average / *total;
    }

    fn rank(&self, piece: Piece, mv: Move) -> i32 {
        let (average, _) = self.to_sq[piece as usize][mv.to as usize];
        average
    }
}
