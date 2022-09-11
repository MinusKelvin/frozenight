use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    ops::ControlFlow,
    path::PathBuf,
    sync::Mutex,
};

use bytemuck::Zeroable;
use marlinformat::PackedBoard;
use structopt::StructOpt;

use crate::CommonOptions;

#[derive(StructOpt)]
pub struct Options {
    input: PathBuf,
    #[structopt(short = "o", long)]
    output: PathBuf,

    #[structopt(short = "e", long)]
    filter_eval: Option<i16>,
    #[structopt(short = "c", long)]
    filter_capture: bool,
    #[structopt(short = "i", long)]
    filter_in_check: bool,
    #[structopt(short = "g", long)]
    filter_give_check: bool,
}

impl Options {
    pub(super) fn run(self, opt: CommonOptions) {
        let input = Mutex::new(BufReader::new(File::open(self.input).unwrap()));
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

        let output = Mutex::new(BufWriter::new(
            File::options()
                .create_new(true)
                .write(true)
                .open(self.output)
                .unwrap(),
        ));

        opt.parallel(
            || Vec::with_capacity(1024),
            |boards| {
                next(boards);
                if boards.is_empty() {
                    return ControlFlow::Break(());
                }

                boards.retain(|board| {
                    let (_board, eval, _wdl, extra) = board.unpack().unwrap();

                    if self.filter_capture && extra & 1 << 0 != 0 {
                        false
                    } else if self.filter_in_check && extra & 1 << 1 != 0 {
                        false
                    } else if self.filter_give_check && extra & 1 << 2 != 0 {
                        false
                    } else if matches!(
                        self.filter_eval,
                        Some(cp_threshold) if eval.abs() >= cp_threshold * 5
                    ) {
                        false
                    } else {
                        true
                    }
                });

                output
                    .lock()
                    .map(|mut file| file.write_all(bytemuck::cast_slice(&boards)))
                    .unwrap()
                    .unwrap();

                ControlFlow::Continue(())
            },
        );
    }
}
