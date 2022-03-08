use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use cozy_chess::{Board, Color, GameStatus, Move};
use cozy_syzygy::{Tablebase, Wdl};
use frozenight::{Eval, Frozenight};
use rand::prelude::*;
use structopt::StructOpt;

mod games;

#[derive(StructOpt)]
struct Options {
    #[structopt(short = "p", long, default_value = "1")]
    concurrency: usize,
    #[structopt(short = "s", long)]
    syzygy_path: Option<PathBuf>,
    #[structopt(short = "c", long, default_value = "10000000")]
    count: usize,
    #[structopt(short = "d", long, default_value = "6")]
    depth: u16,

    /// One of `wdl`
    kind: Kind,
}

enum Kind {
    Wdl,
}

impl FromStr for Kind {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, std::io::Error> {
        Ok(match s {
            "wdl" => Kind::Wdl,
            _ => return Err(std::io::ErrorKind::Other.into()),
        })
    }
}

fn main() {
    let options = Options::from_args();

    match options.kind {
        Kind::Wdl => games::generate_games(&options),
    }
}

struct Sample {
    board: Board,
    mv: Move,
    eval: Eval,
}

fn play_game(depth: u16, tb: Option<&Tablebase>) -> (Vec<Sample>, Option<Color>) {
    let mut board = Board::default();
    let mut history = HashMap::<_, u8>::new();
    let mut game = vec![];
    for _ in 0..8 {
        let mut moves = vec![];
        board.generate_moves(|mvs| {
            moves.extend(mvs);
            false
        });
        if moves.is_empty() {
            return (vec![], None);
        }
        let entry = history.entry(board.clone()).or_default();
        *entry += 1;
        if *entry >= 3 {
            return (vec![], None);
        }
        let mv = *moves.choose(&mut thread_rng()).unwrap();
        board.play_unchecked(mv);
        game.push(Sample {
            mv,
            eval: Eval::DRAW,
            board: board.clone(),
        });
    }
    if board.status() != GameStatus::Ongoing {
        return (vec![], None);
    }

    let mut engine = Frozenight::new(64);

    let winner = loop {
        match board.status() {
            GameStatus::Won => break Some(!board.side_to_move()),
            GameStatus::Drawn => break None,
            GameStatus::Ongoing => {}
        }

        let entry = history.entry(board.clone()).or_default();
        *entry += 1;
        if *entry >= 3 {
            break None;
        }

        let mut moves = game.iter().map(|s| s.mv);
        engine.set_position(Board::default(), |_| moves.next());

        let (eval, mv) = engine.search_synchronous(None, depth, |_, _, _, _, _| {});

        game.push(Sample {
            mv,
            eval,
            board: board.clone(),
        });
        board.play(mv);

        if let Some(tb) = tb {
            if board.occupied().popcnt() <= tb.max_pieces() as u32 {
                match tb.probe_wdl(&board) {
                    Some((Wdl::Win, _)) => break Some(board.side_to_move()),
                    Some((Wdl::Loss, _)) => break Some(!board.side_to_move()),
                    Some(_) => break None,
                    None => {}
                }
            }
        }
    };

    (game.into_iter().skip(8).collect(), winner)
}
