use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};

use crate::search::INVALID_MOVE;
use crate::SearchInfo;

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

struct SoftDeadlines {
    consistent: Instant,
    inconsistent: Instant,
}

pub(crate) struct TimeManager {
    hard_deadline: Option<Instant>,
    soft_deadline: Option<SoftDeadlines>,
    consistent_move: Option<Move>,
    one_reply: bool,
}

impl TimeManager {
    pub fn new(board: &Board, time: TimeConstraint) -> Self {
        let now = Instant::now();
        if time.use_all_time {
            TimeManager {
                one_reply: false,
                hard_deadline: time
                    .clock
                    .map(|clock| now + clock.saturating_sub(time.overhead)),
                soft_deadline: None,
                consistent_move: None,
            }
        } else {
            let mtg = time.moves_to_go.unwrap_or(45) + 5;
            TimeManager {
                one_reply: time.clock.is_some() && one_reply(board),
                hard_deadline: time
                    .clock
                    .map(|clock| now + (clock / 2).saturating_sub(time.overhead)),
                soft_deadline: time.clock.map(|clock| {
                    let noinc = clock.saturating_sub(time.increment);
                    let consistent = noinc / (mtg * 2) + time.increment / 4;
                    let inconsistent = noinc * 3 / (mtg * 2) + time.increment / 2;

                    let adjust = |d: Duration| {
                        now + d
                            .saturating_sub(time.overhead)
                            .max(Duration::from_millis(1))
                    };

                    SoftDeadlines {
                        consistent: adjust(consistent),
                        inconsistent: adjust(inconsistent),
                    }
                }),
                consistent_move: None,
            }
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.hard_deadline
    }

    pub fn update(&mut self, info: &SearchInfo) -> ControlFlow<()> {
        if *self.consistent_move.get_or_insert(info.best_move) != info.best_move {
            self.consistent_move = Some(INVALID_MOVE);
        }
        match &self.soft_deadline {
            _ if self.one_reply => ControlFlow::Break(()),
            None => ControlFlow::Continue(()),
            Some(deadlines) => {
                let deadline = match self.consistent_move == Some(info.best_move) {
                    true => deadlines.consistent,
                    false => deadlines.inconsistent,
                };
                if Instant::now() < deadline {
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
