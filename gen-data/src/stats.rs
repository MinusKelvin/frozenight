use std::{
    fs::File,
    io::{BufReader, Read},
    ops::ControlFlow,
    path::PathBuf,
    sync::Mutex,
};

use bytemuck::Zeroable;
use cozy_chess::{Color, Piece};
use marlinformat::PackedBoard;
use structopt::StructOpt;

use crate::CommonOptions;

#[derive(StructOpt)]
pub struct Options {
    dataset: PathBuf,
}

impl Options {
    pub(super) fn run(self, opt: CommonOptions) -> std::io::Result<()> {
        let input = Mutex::new(BufReader::new(File::open(self.dataset)?));
        let next = |boards: &mut Vec<_>| {
            let mut data = input.lock().unwrap();
            boards.clear();
            for _ in 0..1024 {
                let mut board = PackedBoard::zeroed();
                if data.read_exact(bytemuck::bytes_of_mut(&mut board)).is_ok() {
                    boards.push(board);
                };
            }
        };

        let full_stats = Mutex::new(Stats::default());

        opt.parallel(
            || Vec::with_capacity(1024),
            |boards| {
                next(boards);
                if boards.is_empty() {
                    return ControlFlow::Break(());
                }
                let mut stats = Stats::default();

                for board in boards {
                    let (board, eval, _, _) = board.unpack().unwrap();

                    let wdl = 1.0 / (1.0 + (-eval as f64 / 1016.0).exp());
                    let wdl = match board.side_to_move() {
                        Color::White => wdl,
                        Color::Black => 1.0 - wdl,
                    };

                    let material = board.pieces(Piece::Pawn).len() as usize
                        + 3 * board.pieces(Piece::Bishop).len() as usize
                        + 3 * board.pieces(Piece::Knight).len() as usize
                        + 5 * board.pieces(Piece::Rook).len() as usize
                        + 8 * board.pieces(Piece::Queen).len() as usize;

                    let bucket = (material * 16 / 76).min(15);

                    match eval {
                        0 => stats.draw_freq += 1,
                        i16::MIN..=-20000 => stats.mate_freq += 1,
                        20000..=i16::MAX => stats.mate_freq += 1,
                        _ => stats.eval_freq[(wdl * 32.0) as usize] += 1,
                    }
                    stats.buckets_freq[bucket] += 1;
                }

                full_stats
                    .lock()
                    .map(|mut full| {
                        for (t, &n) in full.buckets_freq.iter_mut().zip(stats.buckets_freq.iter()) {
                            *t += n;
                        }
                        for (t, &n) in full.eval_freq.iter_mut().zip(stats.eval_freq.iter()) {
                            *t += n;
                        }
                        full.mate_freq += stats.mate_freq;
                        full.draw_freq += stats.draw_freq;
                    })
                    .unwrap();

                ControlFlow::Continue(())
            },
        );

        dbg!(full_stats.into_inner().unwrap());

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    buckets_freq: [u64; 16],
    eval_freq: [u64; 32],
    draw_freq: u64,
    mate_freq: u64,
}
