use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};

use crate::search::INVALID_MOVE;
use crate::{Eval, SearchInfo};

#[derive(Copy, Clone, Debug)]
pub struct TimeConstraint {
    pub nodes: u64,
    pub depth: i16,
    pub clock: Option<Duration>,
    pub increment: Duration,
    pub overhead: Duration,
    pub moves_to_go: Option<u32>,
    pub use_all_time: bool,
}

impl TimeConstraint {
    pub const INFINITE: TimeConstraint = TimeConstraint {
        nodes: u64::MAX,
        depth: i16::MAX,
        clock: None,
        increment: Duration::ZERO,
        overhead: Duration::ZERO,
        moves_to_go: None,
        use_all_time: true,
    };
}

pub(crate) struct TimeManager {
    board: Board,
    soft_deadline: Option<Instant>,
    hard_deadline: Option<Instant>,
    soft_deadline_increment: Duration,
    quiet_bonus_decay: u32,
    one_reply: bool,
    prev_best_move: Move,
    prev_best_eval: Eval,
}

impl TimeManager {
    pub fn new(board: &Board, time: TimeConstraint) -> Self {
        let now = Instant::now();
        let clock = match time.clock {
            Some(v) => v,
            None => {
                return TimeManager {
                    board: board.clone(),
                    one_reply: false,
                    hard_deadline: None,
                    soft_deadline: None,
                    soft_deadline_increment: Duration::ZERO,
                    prev_best_eval: Eval::DRAW,
                    prev_best_move: INVALID_MOVE,
                    quiet_bonus_decay: 1
                }
            }
        };

        if time.use_all_time {
            return TimeManager {
                board: board.clone(),
                one_reply: false,
                hard_deadline: Some(now + clock.saturating_sub(time.overhead)),
                soft_deadline: None,
                soft_deadline_increment: Duration::ZERO,
                prev_best_eval: Eval::DRAW,
                prev_best_move: INVALID_MOVE,
                quiet_bonus_decay: 1,
            };
        }

        let mtg = time.moves_to_go.unwrap_or(40).clamp(1, 40);

        let hard_deadline = match mtg {
            0..=10 => clock * 20 / (20 - mtg),
            _ => clock / 2,
        }
        .saturating_sub(time.overhead);

        let soft_deadline_increment = clock / 100;
        let initial_soft_deadline = (clock.saturating_sub(time.increment) / (2 * mtg + 5))
            .saturating_add(time.increment)
            .saturating_sub(time.overhead);

        TimeManager {
            board: board.clone(),
            one_reply: one_reply(board),
            hard_deadline: Some(now + hard_deadline),
            soft_deadline: Some(now + initial_soft_deadline),
            soft_deadline_increment,
            prev_best_eval: Eval::DRAW,
            prev_best_move: INVALID_MOVE,
            quiet_bonus_decay: 1,
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.hard_deadline
    }

    pub fn update(&mut self, info: &SearchInfo) -> ControlFlow<()> {
        match self.soft_deadline {
            _ if self.one_reply => ControlFlow::Break(()),
            None => ControlFlow::Continue(()),
            Some(ref mut deadline) => {
                let now = Instant::now();

                let capture =
                    self.board.color_on(info.best_move.to) == Some(!self.board.side_to_move());
                let check = {
                    let mut b = self.board.clone();
                    b.play_unchecked(info.best_move);
                    !b.checkers().is_empty()
                };

                // extend time if best move is quiet
                if !capture && !check {
                    *deadline += self.soft_deadline_increment / self.quiet_bonus_decay;
                    self.quiet_bonus_decay += 1;
                }

                self.prev_best_eval = info.eval;
                self.prev_best_move = info.best_move;

                if now < *deadline {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            }
        }
    }
}

fn one_reply(board: &Board) -> bool {
    let mut moves = 0;
    board.generate_moves(|mvs| {
        moves += mvs.len();
        moves > 1
    });
    moves == 1
}
