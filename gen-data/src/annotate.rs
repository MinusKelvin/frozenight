use std::fs::File;
use std::io::{stdout, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use bytemuck::Zeroable;
use cozy_chess::Color;
use frozenight::{Frozenight, TimeConstraint};
use marlinformat::PackedBoard;
use structopt::StructOpt;

use crate::CommonOptions;

#[derive(StructOpt)]
pub(crate) struct Options {
    input: PathBuf,
    #[structopt(short = "o", long)]
    output: PathBuf,

    #[structopt(short = "n", long)]
    nodes: Option<u64>,
    #[structopt(short = "d", long)]
    depth: Option<i16>,
}

impl Options {
    pub fn run(self, opt: CommonOptions) {
        if self.nodes.is_some() == self.depth.is_some() {
            eprintln!("Exactly one of --nodes and --depth must be specified.");
            return;
        }

        let start = Instant::now();
        let games = AtomicUsize::new(0);

        let mut input = File::open(self.input).unwrap();
        let total_positions =
            input.seek(SeekFrom::End(0)).unwrap() / std::mem::size_of::<PackedBoard>() as u64;
        input.seek(SeekFrom::Start(0)).unwrap();
        let input = Mutex::new(BufReader::new(input));
        let next = |boards: &mut Vec<_>| {
            let mut data = input.lock().unwrap();
            boards.clear();
            for _ in 0..64 {
                let mut board = PackedBoard::zeroed();
                if data.read_exact(bytemuck::bytes_of_mut(&mut board)).is_ok() {
                    boards.push(board);
                };
            }
        };

        let output = Mutex::new(BufWriter::new(
            File::options()
                .create_new(true)
                .write(true)
                .open(self.output)
                .unwrap(),
        ));

        opt.parallel(
            || (Vec::with_capacity(64), Frozenight::new(64)),
            |(boards, engine)| {
                next(boards);
                if boards.is_empty() {
                    return ControlFlow::Break(());
                }

                for packed in &mut *boards {
                    let (board, _, wdl, _) = packed.unpack().unwrap();

                    engine.new_game();
                    engine.set_position(board.clone());
                    let info = engine.search(
                        TimeConstraint {
                            nodes: self.nodes.unwrap_or(u64::MAX),
                            depth: self.depth.unwrap_or(250),
                            ..TimeConstraint::INFINITE
                        },
                        |_| {},
                    );

                    let white_eval = match board.side_to_move() {
                        Color::White => info.eval,
                        Color::Black => -info.eval,
                    };

                    let capture = board.colors(!board.side_to_move()).has(info.best_move.to);
                    let in_check = !board.checkers().is_empty();
                    let gives_check = {
                        let mut b = board.clone();
                        b.play_unchecked(info.best_move);
                        !b.checkers().is_empty()
                    };

                    let extra = capture as u8 | (in_check as u8) << 1 | (gives_check as u8) << 2;

                    *packed = PackedBoard::pack(&board, white_eval.raw(), wdl, extra);
                }

                output
                    .lock()
                    .map(|mut file| file.write_all(bytemuck::cast_slice(&boards)))
                    .unwrap()
                    .unwrap();

                let completed = games.fetch_add(boards.len(), Ordering::Relaxed) + boards.len();
                let completion = completed as f64 / total_positions as f64;
                let time = start.elapsed().as_secs_f64();
                let eta = time / completion - time;
                print!(
                    "\r\x1b[K{:>6.2}% complete. ETA: {} minutes",
                    completion * 100.0,
                    eta as i64 / 60,
                );
                stdout().flush().unwrap();

                ControlFlow::Continue(())
            },
        );

        println!();
    }
}
