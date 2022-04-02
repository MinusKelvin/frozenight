use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{stdout, BufWriter, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cozy_chess::{Board, Color, GameStatus, Move};
use cozy_syzygy::{Tablebase, Wdl};
use frozenight::Frozenight;
use rand::prelude::*;
use structopt::StructOpt;

use crate::CommonOptions;

#[derive(StructOpt)]
pub(crate) struct Options {
    #[structopt(short = "n", long, default_value = "10000")]
    nodes: u64,
    #[structopt(default_value = "10_000_000", parse(try_from_str = crate::parse_filter_underscore))]
    positions: usize,
}

impl Options {
    pub(crate) fn run(self, opt: CommonOptions) {
        let output = OpenOptions::new()
            .create(true)
            .append(true)
            .open("games.dat")
            .unwrap();
        let output = Mutex::new(BufWriter::new(output));

        let mut tb = Tablebase::new();
        for path in opt.syzygy_path {
            let _ = tb.add_directory(path);
        }
        if tb.max_pieces() > 2 {
            println!("Using tablebase with {} men", tb.max_pieces());
        }

        let game_counter = Arc::new(AtomicUsize::new(0));
        let start = Instant::now();

        crossbeam_utils::thread::scope(|s| {
            for _ in 0..opt.concurrency {
                s.spawn(|_| while !crate::ABORT.load(Ordering::SeqCst) {
                    let (start_pos, mvs, winner) = self.play_game(&tb);

                    output.lock().map(|mut output| {
                        write!(output, "{start_pos:#}\t").unwrap();
                        write!(output, "{}", mvs.first().unwrap()).unwrap();
                        for mv in &mvs[1..] {
                            write!(output, " {mv}").unwrap();
                        }
                        writeln!(output, "\t{}", match winner {
                            Some(Color::White) => "1-0",
                            Some(Color::Black) => "0-1",
                            None => "1/2-1/2"
                        }).unwrap();
                    }).unwrap();

                    let total = mvs.len() + game_counter.fetch_add(mvs.len(), Ordering::SeqCst);
                    let completion = total as f64 / self.positions as f64;
                    let time = start.elapsed().as_secs_f64();
                    let eta = time / completion - time;
                    print!(
                        "\r\x1b[K{:>6.2}% complete. {:.0} positions/sec. Estimated time remaining: {} minutes",
                        completion * 100.0,
                        total as f64 / time,
                        eta as i64 / 60,
                    );
                    stdout().flush().unwrap();
                    if total >= self.positions {
                        break;
                    }
                });
            }
        })
        .unwrap();
        println!();
    }

    fn generate_starting_position(&self) -> Board {
        let mut board = Board::default();
        for _ in 0..8 {
            let mut moves = vec![];
            board.generate_moves(|mvs| {
                moves.extend(mvs);
                false
            });
            if moves.is_empty() {
                return self.generate_starting_position();
            }
            let mv = *moves.choose(&mut thread_rng()).unwrap();
            board.play_unchecked(mv);
        }
        if board.status() != GameStatus::Ongoing {
            return self.generate_starting_position();
        }
        board
    }

    fn play_game(&self, tb: &Tablebase) -> (Board, Vec<Move>, Option<Color>) {
        let start_pos = self.generate_starting_position();
        let mut repetitions = HashMap::<_, u8>::new();
        let mut game = vec![];

        let mut engine = Frozenight::new(64);
        let mut board = start_pos.clone();

        let winner = loop {
            match board.status() {
                GameStatus::Won => break Some(!board.side_to_move()),
                GameStatus::Drawn => break None,
                GameStatus::Ongoing => {}
            }

            let entry = repetitions.entry(board.hash()).or_default();
            *entry += 1;
            if *entry >= 3 {
                break None;
            }

            let mut moves = game.iter().copied();
            engine.set_position(start_pos.clone(), |_| moves.next());

            let (_, mv) = engine.search_synchronous(None, 250, self.nodes, |_, _, _, _, _| {});

            game.push(mv);
            board.play(mv);

            if board.occupied().popcnt() <= tb.max_pieces() as u32 {
                match tb.probe_wdl(&board) {
                    Some((Wdl::Win, _)) => break Some(board.side_to_move()),
                    Some((Wdl::Loss, _)) => break Some(!board.side_to_move()),
                    Some(_) => break None,
                    None => {}
                }
            }
        };

        (start_pos, game, winner)
    }
}
