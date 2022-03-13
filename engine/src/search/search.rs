use arrayvec::ArrayVec;
use cozy_chess::*;

use crate::eval::*;
use super::SearchHandler;
use super::cache::*;
use super::helpers::move_is_quiet;
use super::moves::*;
use super::window::Window;
use super::oracle;
use super::history::HistoryTable;
use super::formulas::*;

#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub nodes: u64,
    pub seldepth: u8
}

#[derive(Debug, Clone)]
pub struct SearcherResult {
    pub mv: Move,
    pub eval: Eval,
    pub stats: SearchStats
}

pub struct SearchSharedState<H> {
    pub handler: H,
    pub history: Vec<u64>,
    pub cache_table: CacheTable
}

pub const KILLER_ENTRIES: usize = 2;
pub(crate) type KillerEntry = ArrayVec<Move, KILLER_ENTRIES>;

pub struct SearchData {
    pub game_history: Vec<u64>,
    pub killers: [KillerEntry; u8::MAX as usize],
    pub history_table: HistoryTable
}

impl SearchData {
    pub fn new(history: Vec<u64>) -> Self {
        const EMPTY_KILLER_ENTRY: KillerEntry = KillerEntry::new_const();
        Self {
            game_history: history.clone(),
            killers: [EMPTY_KILLER_ENTRY; u8::MAX as usize],
            history_table: HistoryTable::new()
        }
    }

    pub fn search<H: SearchHandler>(
        &mut self,
        shared: &mut SearchSharedState<H>,
        board: &Board,
        depth: u8,
        window: Window
    ) -> Result<SearcherResult, ()> {
        let mut searcher = Searcher {
            shared,
            data: self,
            search_result: None,
            stats: SearchStats::default()
        };
        let eval = searcher.search_node(
            Node::Root,
            &board,
            depth,
            0,
            window
        )?;
        Ok(SearcherResult {
            mv: searcher.search_result.unwrap(),
            eval,
            stats: searcher.stats
        })
    }
}

pub struct Searcher<'s, H> {
    pub shared: &'s mut SearchSharedState<H>,
    pub data: &'s mut SearchData,
    pub search_result: Option<Move>,
    pub stats: SearchStats
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Root,
    Pv,
    Normal
}

impl<H: SearchHandler> Searcher<'_, H> {
    // CITE: The base of this engine is built on principal variation search.
    // https://www.chessprogramming.org/Principal_Variation_Search
    pub fn search_node(
        &mut self,
        node: Node,
        board: &Board,
        mut depth: u8,
        ply_index: u8,
        mut window: Window
    ) -> Result<Eval, ()> {
        self.data.game_history.push(board.hash());
        let result = (|| {
            self.stats.seldepth = self.stats.seldepth.max(ply_index);

            let in_check = !board.checkers().is_empty();

            if in_check {
                // CITE: Check extensions.
                // https://www.chessprogramming.org/Check_Extensions
                depth += 1;
            }

            if depth == 0 {
                if node != Node::Root && self.repetitions(&board) > 1 {
                    return Ok(Eval::DRAW);
                }
                // We are allowed to search in this node as qsearch doesn't track history
                return Ok(self.quiescence(board, ply_index, window));
            }

            self.stats.nodes += 1;

            let init_window = window;

            if self.shared.handler.stop_search() {
                return Err(());
            }

            if node != Node::Root && self.repetitions(&board) > 0 {
                return Ok(Eval::DRAW);
            }
            match board.status() {
                GameStatus::Won => return Ok(Eval::mated_in(ply_index)),
                GameStatus::Drawn => return Ok(Eval::DRAW),
                GameStatus::Ongoing => {}
            }
            if node != Node::Root {
                if let Some(eval) = oracle::oracle(&board) {
                    return Ok(eval);
                }
            }

            let mut pv_move = None;
            let cache_entry = self.shared.cache_table.get(&board, ply_index);
            if let Some(entry) = cache_entry {
                pv_move = Some(entry.best_move);
                if !matches!(node, Node::Root | Node::Pv) && entry.depth >= depth {
                    match entry.kind {
                        TableEntryKind::Exact => return Ok(entry.eval),
                        TableEntryKind::LowerBound => window.narrow_alpha(entry.eval),
                        TableEntryKind::UpperBound => window.narrow_beta(entry.eval),
                    }
                    if window.empty() {
                        return Ok(entry.eval);
                    }
                }
            }

            let static_eval = cache_entry
                .and_then(|e| {
                    if e.eval.as_cp().is_some() {
                        Some(e.eval)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| evaluate(board));

            if !matches!(node, Node::Root | Node::Pv) {
                // CITE: Reverse futility pruning.
                // https://www.chessprogramming.org/Reverse_Futility_Pruning
                if let Some(margin) = reverse_futility_margin(depth) {
                    let eval_estimate = static_eval.saturating_sub(margin);
                    if eval_estimate >= window.beta {
                        return Ok(eval_estimate);
                    }
                }
            }

            let our_pieces = board.colors(board.side_to_move());
            let sliding_pieces =
                board.pieces(Piece::Rook) |
                board.pieces(Piece::Bishop) |
                board.pieces(Piece::Queen);

            let mut best_move = None;
            let mut best_eval = Eval::MIN;
            // CITE: Null move pruning.
            // The idea for doing it only when static_eval >= beta was
            // first suggested to me by the Black Marlin author.
            // https://www.chessprogramming.org/Null_Move_Pruning
            let do_nmp = static_eval >= window.beta
                && !(our_pieces & sliding_pieces).is_empty();
            if node != Node::Root && do_nmp {
                if let Some(child) = board.null_move() {
                    let mut window = window.null_window_beta();
                    let reduction = nmp_calculate_reduction(static_eval, window);
                    let eval = -self.search_node(
                        Node::Normal,
                        &child,
                        (depth - 1).saturating_sub(reduction),
                        ply_index + 1,
                        -window
                    )?;
                    window.narrow_alpha(eval);
                    if window.empty() {
                        //TODO This might not bet correct since we can return a false mate score.
                        //Not quite sure what to do in that case though.
                        //NMP operates on the assumption that some other move works too anyway though.
                        return Ok(eval);
                    }
                }
            }
            let mut moves = MoveList::new(
                board,
                pv_move,
                self.data.killers[ply_index as usize].clone()
            );

            // CITE: Futility pruning.
            // This implementation is also based on extended futility pruning.
            // https://www.chessprogramming.org/Futility_Pruning
            let futile = if let Some(margin) = futility_margin(depth) {
                let max_eval = static_eval.saturating_add(margin);
                max_eval <= window.alpha
            } else {
                false
            };
            let mut quiets_to_check = lmp_quiets_to_check(depth);
            while let Some((i, (mv, move_score))) = moves.pick(self) {
                // CITE: Late move pruning.
                // We check only a certain number of quiets per node given some depth.
                // This was suggested to me by the Black Marlin author.
                if let MoveScore::Quiet(_) = move_score {
                    if quiets_to_check > 0 {
                        quiets_to_check -= 1;
                    } else {
                        continue;
                    }
                }
                let mut child = board.clone();
                child.play_unchecked(mv);
                let gives_check = !child.checkers().is_empty();
                let quiet = move_is_quiet(mv, &board);

                if best_move.is_some() && futile && quiet && !in_check && !gives_check {
                    continue;
                }

                let mut child_node_type = if i == 0 {
                    Node::Pv
                } else {
                    Node::Normal
                };
                let mut child_window = if child_node_type == Node::Pv {
                    window
                } else {
                    window.null_window_alpha()
                };
                let mut reduction = 0;
                // CITE: Late move reductions.
                // https://www.chessprogramming.org/Late_Move_Reductions
                if depth >= LMR_MIN_DEPTH && quiet && !in_check && !gives_check {
                    let history = self.data.history_table.get(board, mv);
                    reduction += lmr_calculate_reduction(i, depth, history);
                }
                let mut eval = -self.search_node(
                    child_node_type,
                    &child,
                    (depth - 1).saturating_sub(reduction),
                    ply_index + 1,
                    -child_window
                )?;
                if (child_window != window || reduction > 0) && window.contains(eval) {
                    child_window = window;
                    child_node_type = Node::Pv;
                    eval = -self.search_node(
                        child_node_type,
                        &child,
                        depth - 1,
                        ply_index + 1,
                        -child_window
                    )?;
                }

                if eval > best_eval {
                    best_eval = eval;
                    best_move = Some(mv);
                }

                window.narrow_alpha(eval);
                if window.empty() {
                    if move_is_quiet(mv, &board) {
                        // CITE: Killer moves.
                        // https://www.chessprogramming.org/Killer_Heuristic
                        let killers = &mut self.data.killers[ply_index as usize];
                        if killers.is_full() {
                            killers.remove(0);
                        }
                        killers.push(mv);
                        // CITE: History heuristic.
                        // https://www.chessprogramming.org/History_Heuristic
                        self.data.history_table.update(board, mv, depth, true);
                    }
                    // CITE: We additionally punish the history of quiet moves that don't produce cutoffs.
                    // Suggested by the Black Marlin author and additionally observed in MadChess.
                    for &(prev_mv, _) in moves.yielded() {
                        if prev_mv != mv && move_is_quiet(prev_mv, &board) {
                            self.data.history_table.update(board, prev_mv, depth, false);
                        }
                    }
                    break;
                }
            }
            let best_move = best_move.unwrap();

            self.shared.cache_table.set(&board, ply_index, TableEntry {
                kind: match best_eval {
                    // No move was capable of raising alpha.
                    // The actual value might be worse than this.
                    _ if best_eval <= init_window.alpha => TableEntryKind::UpperBound,
                    // The move was too good and this is a cut node.
                    // The value might be even better if it were not cut off.
                    _ if best_eval >= window.beta => TableEntryKind::LowerBound,
                    // It's in the window. This is an exact value.
                    _ => TableEntryKind::Exact
                },
                eval: best_eval,
                depth,
                best_move
            });

            if node == Node::Root {
                self.search_result = Some(best_move);
            }

            Ok(best_eval)
        })();
        self.data.game_history.pop();
        result
    }

    // CITE: Quiescence search.
    // https://www.chessprogramming.org/Quiescence_Search
    fn quiescence(
        &mut self,
        board: &Board,
        ply_index: u8,
        mut window: Window
    ) -> Eval {
        //TODO track history and repetitions in quiescence? This seems to lose Elo though...
        let result = (|| {
            self.stats.nodes += 1;

            match board.status() {
                GameStatus::Won => return Eval::mated_in(ply_index),
                GameStatus::Drawn => return Eval::DRAW,
                GameStatus::Ongoing => {}
            }
            if let Some(eval) = oracle::oracle(board) {
                return eval;
            }

            if let Some(entry) = self.shared.cache_table.get(board, ply_index) {
                match entry.kind {
                    TableEntryKind::Exact => return entry.eval,
                    TableEntryKind::LowerBound => window.narrow_alpha(entry.eval),
                    TableEntryKind::UpperBound => window.narrow_beta(entry.eval),
                }
                if window.empty() {
                    return entry.eval;
                }
            }

            let mut best_eval = evaluate(board);
            window.narrow_alpha(best_eval);
            if window.empty() {
                return best_eval;
            }

            let mut move_list = QSearchMoveList::new(board);
            while let Some((_, (mv, _))) = move_list.pick() {
                let mut child = board.clone();
                child.play_unchecked(mv);
                let eval = -self.quiescence(
                    &child,
                    ply_index + 1,
                    -window
                );

                if eval > best_eval {
                    best_eval = eval;
                }

                window.narrow_alpha(eval);
                if window.empty() {
                    break;
                }
            }

            best_eval
        })();
        result
    }

    fn repetitions(&self, board: &Board) -> usize {
        self.data.game_history.iter()
            .rev()
            .take(board.halfmove_clock() as usize + 1)
            .step_by(2) // Every second ply so it's our turn
            .skip(1) // Skip our board
            .filter(|&&hash| hash == board.hash())
            .count()
    }
}
