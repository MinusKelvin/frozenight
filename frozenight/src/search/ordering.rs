use cozy_chess::{Board, Color, Move, Piece, Square};

pub struct MoveOrdering<'a> {
    board: &'a Board,
    stage: MoveOrderingStage,
    hashmove: Option<Move>,
    killer: Move,
    captures: Vec<(Move, i8)>,
    quiets: Vec<(Move, Piece)>,
    underpromotions: Vec<Move>,
    count: usize,
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
            captures: vec![],
            quiets: vec![],
            underpromotions: vec![],
            count: 0,
        }
    }

    pub fn next(&mut self, history: &HistoryTable) -> Option<(usize, Move)> {
        match self.stage {
            MoveOrderingStage::Hashmove => self.hashmove(),
            MoveOrderingStage::GenerateMoves => self.generate_moves(history),
            MoveOrderingStage::Captures => self.captures(history),
            MoveOrderingStage::Quiets => self.quiets(history),
            MoveOrderingStage::Underpromotions => self.underpromotions(),
        }
        .map(|mv| {
            let count = self.count;
            self.count += 1;
            (count, mv)
        })
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
                    _ if mv.promotion.is_some() => {
                        // Give promotions the same rank as PxP
                        self.captures.push((mv, 0));
                    }
                    _ => {
                        self.quiets.push((mv, mvs.piece));
                    }
                }
            }
            false
        });
        self.captures(history)
    }

    fn captures(&mut self, history: &HistoryTable) -> Option<Move> {
        if self.captures.is_empty() {
            self.stage = MoveOrderingStage::Quiets;
            return self.quiets(history);
        }

        let mut index = 0;
        for i in 1..self.captures.len() {
            if self.captures[i].1 > self.captures[index].1 {
                index = i;
            }
        }

        Some(self.captures.swap_remove(index).0)
    }

    fn quiets(&mut self, history: &HistoryTable) -> Option<Move> {
        if self.quiets.is_empty() {
            self.stage = MoveOrderingStage::Underpromotions;
            return self.underpromotions();
        }

        let mut index = 0;
        let mut rank = history.rank(
            self.quiets[0].1,
            self.quiets[0].0,
            self.board.side_to_move(),
        );
        for i in 1..self.quiets.len() {
            let r = history.rank(
                self.quiets[i].1,
                self.quiets[i].0,
                self.board.side_to_move(),
            );
            if r > rank {
                index = i;
                rank = r;
            }
        }

        Some(self.quiets.swap_remove(index).0)
    }

    fn underpromotions(&mut self) -> Option<Move> {
        self.underpromotions.pop()
    }
}

pub struct HistoryTable {
    piece_to_sq: [[[(u32, u32); Square::NUM]; Piece::NUM]; Color::NUM],
    from_sq_to_sq: [[[(u32, u32); Square::NUM]; Square::NUM]; Color::NUM],
}

impl HistoryTable {
    pub fn new() -> Self {
        HistoryTable {
            piece_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Piece::NUM]; Color::NUM],
            from_sq_to_sq: [[[(1_000_000_000, 0); Square::NUM]; Square::NUM]; Color::NUM],
        }
    }

    pub fn decay(&mut self) {
        for (_, total) in self.piece_to_sq.iter_mut().flatten().flatten() {
            *total /= 64;
        }
        for (_, total) in self.from_sq_to_sq.iter_mut().flatten().flatten() {
            *total /= 16;
        }
    }

    pub fn caused_cutoff(&mut self, board: &Board, mv: Move) {
        let stm = board.side_to_move();
        let piece = board.piece_on(mv.from).unwrap();
        let (piece_to, total) = &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let diff = 2_000_000_000 - *piece_to;
        *total += 1;
        *piece_to += diff / *total;
        let (from_to, total) =
            &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        let diff = 2_000_000_000 - *from_to;
        *total += 1;
        *from_to += diff / *total;
    }

    pub fn did_not_cause_cutoff(&mut self, board: &Board, mv: Move) {
        let stm = board.side_to_move();
        let piece = board.piece_on(mv.from).unwrap();
        let (piece_to, total) = &mut self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        *total += 1;
        *piece_to -= *piece_to / *total;
        let (from_to, total) =
            &mut self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        *total += 1;
        *from_to -= *from_to / *total;
    }

    fn rank(&self, piece: Piece, mv: Move, stm: Color) -> u32 {
        let (piece_to, _) = self.piece_to_sq[stm as usize][piece as usize][mv.to as usize];
        let (from_to, _) = self.from_sq_to_sq[stm as usize][mv.from as usize][mv.to as usize];
        piece_to + from_to
    }
}
