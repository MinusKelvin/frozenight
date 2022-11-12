use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use cozy_chess::Board;

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

pub(crate) struct TimeManager {
    soft_deadline: Option<Instant>,
    hard_deadline: Option<Instant>,
    one_reply: bool,
}

impl TimeManager {
    pub fn new(board: &Board, time: TimeConstraint) -> Self {
        let now = Instant::now();
        TimeManager {
            one_reply: !time.use_all_time && time.clock.is_some() && one_reply(board),
            hard_deadline: time
                .clock
                .map(|clock| now + (clock / 2).saturating_sub(time.overhead)),
            soft_deadline: time
                .clock
                .map(|clock| {
                    if time.use_all_time {
                        return clock;
                    }

                    let mtg = time.moves_to_go.unwrap_or(45) + 5;

                    clock.saturating_sub(time.increment) / mtg + time.increment / 2
                })
                .map(|amt| {
                    now + amt
                        .saturating_sub(time.overhead)
                        .max(Duration::from_millis(1))
                }),
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.hard_deadline
    }

    pub fn update(&mut self, _info: &SearchInfo) -> ControlFlow<()> {
        match self.soft_deadline {
            _ if self.one_reply => ControlFlow::Break(()),
            None => ControlFlow::Continue(()),
            Some(deadline) => {
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
