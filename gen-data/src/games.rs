use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{stdout, BufWriter, Write};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cozy_chess::{Board, Color, GameStatus, Piece};
use cozy_syzygy::{Tablebase, Wdl};
use frozenight::{Frozenight, TimeConstraint};
use marlinformat::PackedBoard;
use rand::prelude::*;
use structopt::StructOpt;

use crate::CommonOptions;

#[derive(StructOpt)]
pub(crate) struct Options {
    #[structopt(short = "o", long)]
    output: PathBuf,

    #[structopt(short = "n", long)]
    nodes: Option<u64>,
    #[structopt(short = "d", long)]
    depth: Option<i16>,

    #[structopt(parse(try_from_str = crate::parse_filter_underscore))]
    positions: usize,

    #[structopt(long)]
    frc: bool,
    #[structopt(long)]
    dfrc: bool,

    #[structopt(short = "r", long, default_value = "0.0")]
    random_move: f64,
}

impl Options {
    pub(crate) fn run(self, opt: CommonOptions) {
        let mut error = false;

        if self.frc && self.dfrc {
            eprintln!("Only one of --frc and --dfrc can be specified");
            error = true;
        }

        if self.nodes.is_some() == self.depth.is_some() {
            eprintln!("Exactly one of --nodes and --depth must be specified.");
            error = true;
        }

        if !(0.0..=1.0).contains(&self.random_move) {
            eprintln!("Random move probability must be between 0 and 1 inclusive");
            error = true;
        }

        if error {
            return;
        }

        let tb = opt.syzygy();

        let output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.output)
            .unwrap();
        let output = Mutex::new(BufWriter::new(output));

        let game_counter = Arc::new(AtomicUsize::new(0));
        let start = Instant::now();

        opt.parallel(
            || Frozenight::new(64),
            |engine| {
                let boards = self.play_game(engine, &tb);

                let games = game_counter.fetch_add(boards.len(), Ordering::SeqCst);
                if games >= self.positions {
                    return ControlFlow::Break(());
                }

                output
                    .lock()
                    .map(|mut output| output.write_all(bytemuck::cast_slice(&boards)))
                    .unwrap()
                    .unwrap();

                let total = games + boards.len();
                let completion = total as f64 / self.positions as f64;
                let time = start.elapsed().as_secs_f64();
                let eta = time / completion - time;
                print!(
                    "\r\x1b[K{:>6.2}% complete. {:.0} positions/sec. ETA: {} minutes",
                    completion * 100.0,
                    total as f64 / time,
                    eta as i64 / 60,
                );
                stdout().flush().unwrap();

                ControlFlow::Continue(())
            },
        );
        println!();
    }

    fn generate_starting_position(&self) -> Board {
        let mut board = match () {
            _ if self.frc => Board::chess960_startpos(thread_rng().gen_range(0..960)),
            _ if self.dfrc => Board::double_chess960_startpos(
                thread_rng().gen_range(0..960),
                thread_rng().gen_range(0..960),
            ),
            _ => Board::default(),
        };
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

    fn play_game(&self, engine: &mut Frozenight, tb: &Tablebase) -> Vec<PackedBoard> {
        let start_pos = self.generate_starting_position();
        let mut repetitions = HashSet::new();
        let mut game = vec![];

        engine.new_game();
        let mut board = start_pos.clone();

        let mut outcome = None;
        loop {
            match board.status() {
                GameStatus::Won => {
                    outcome.get_or_insert(match board.side_to_move() {
                        Color::White => 0,
                        Color::Black => 2,
                    });
                    break;
                }
                GameStatus::Drawn => {
                    outcome.get_or_insert(match board.side_to_move() {
                        Color::White => 0,
                        Color::Black => 2,
                    });
                    break;
                }
                GameStatus::Ongoing => {}
            }

            if board.occupied().len() == 2
                || board.occupied().len() == 3
                    && !(board.pieces(Piece::Bishop) | board.pieces(Piece::Knight)).is_empty()
            {
                outcome.get_or_insert(match board.side_to_move() {
                    Color::White => 0,
                    Color::Black => 2,
                });
                break;
            }

            if !repetitions.insert(board.hash()) {
                outcome.get_or_insert(match board.side_to_move() {
                    Color::White => 0,
                    Color::Black => 2,
                });
                break;
            }

            let tb_outcome = match board.occupied().len() <= tb.max_pieces() {
                true => match tb.probe_wdl(&board) {
                    Some((Wdl::Win, _)) => Some(match board.side_to_move() {
                        Color::White => 2,
                        Color::Black => 0,
                    }),
                    Some((Wdl::Loss, _)) => Some(match board.side_to_move() {
                        Color::White => 0,
                        Color::Black => 2,
                    }),
                    Some(_) => Some(1),
                    None => None,
                },
                false => None,
            };

            if tb_outcome.is_some() && outcome.is_none() {
                outcome = tb_outcome;
            }

            let mv = if thread_rng().gen_bool(self.random_move) {
                let mut moves = vec![];
                board.generate_moves(|mvs| {
                    moves.extend(mvs);
                    false
                });
                *moves.choose(&mut thread_rng()).unwrap()
            } else {
                engine.set_position(board.clone());

                engine
                    .search(
                        TimeConstraint {
                            nodes: self.nodes.unwrap_or(u64::MAX),
                            depth: self.depth.unwrap_or(250),
                            ..TimeConstraint::INFINITE
                        },
                        |_| {},
                    )
                    .best_move
            };

            game.push((mv, tb_outcome));
            board.play(mv);
        }

        let outcome = outcome.unwrap();

        game.into_iter()
            .scan(start_pos, |board, (mv, tb_outcome)| {
                let value = PackedBoard::pack(&board, 0, tb_outcome.unwrap_or(outcome), 0);
                board.play(mv);
                Some(value)
            })
            .collect()
    }
}
