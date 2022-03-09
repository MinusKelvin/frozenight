use std::collections::HashSet;
use std::fs::File;
use std::io::{stdout, BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use cozy_chess::{Board, Color, Piece, Square};
use cozy_syzygy::Tablebase;
use frozenight::{Eval, Frozenight};
use structopt::StructOpt;

use crate::{CommonOptions, ABORT};

#[derive(StructOpt)]
pub(crate) struct Options {
    #[structopt(short = "d", long, default_value = "5")]
    depth: u16,
    #[structopt(short = "t", long)]
    filter_captures: bool,
    #[structopt(short = "c", long)]
    filter_in_check: bool,
    #[structopt(short = "g", long)]
    filter_give_check: bool,
    #[structopt(short = "m", long)]
    filter_mate_scores: bool,
    #[structopt(short = "b", long)]
    filter_tb_positions: bool,
}

impl Options {
    pub fn run(self, opt: CommonOptions) {
        let output = Mutex::new(BufWriter::new(File::create("data.bin").unwrap()));

        let tb = opt.syzygy_path.map(Tablebase::new);
        if let Some(tb) = tb.as_ref() {
            println!("Using tablebase with {} men", tb.max_pieces());
        }

        let start = Instant::now();
        let positions = AtomicUsize::new(0);
        let games = AtomicUsize::new(0);

        let mut input = File::open("games.dat").unwrap();
        let total_games = BufReader::new(&mut input).lines().count();
        input.seek(SeekFrom::Start(0)).unwrap();
        let input = Mutex::new(BufReader::new(input).lines());

        let seen_positions = Mutex::new(HashSet::with_capacity(total_games * 32));

        crossbeam_utils::thread::scope(|s| {
            for _ in 0..opt.concurrency {
                s.spawn(|_| {
                    let mut engine = Frozenight::new(64);
                    while !ABORT.load(Ordering::SeqCst) {
                        let line = match input.lock().unwrap().next() {
                            Some(l) => l,
                            None => break,
                        };
                        let mut line = line.as_ref().unwrap().split('\t');
                        let start_pos = Board::from_fen(line.next().unwrap(), true).unwrap();
                        let moves = line.next().unwrap();
                        let moves = moves.split(' ').map(|s| s.parse().unwrap());
                        let winner = match line.next().unwrap() {
                            "1-0" => Some(Color::White),
                            "0-1" => Some(Color::Black),
                            "1/2-1/2" => None,
                            s => panic!("Invalid game result: {s}"),
                        };

                        let boards = moves.scan(start_pos, |b, mv| {
                            let r = b.clone();
                            b.play(mv);
                            Some(r)
                        });

                        for board in boards {
                            if self.filter_in_check && !board.checkers().is_empty() {
                                continue;
                            }
                            match tb {
                                Some(ref tb)
                                    if self.filter_tb_positions
                                        && board.occupied().popcnt() <= tb.max_pieces() as u32
                                        && tb.probe_wdl(&board).is_some() =>
                                {
                                    continue
                                }
                                _ => {}
                            }
                            if !seen_positions.lock().unwrap().insert(board.hash()) {
                                continue;
                            }

                            engine.set_position(board.clone(), |_| None);
                            let (eval, mv) =
                                engine.search_synchronous(None, self.depth, |_, _, _, _, _| {});

                            if self.filter_captures
                                && board.colors(!board.side_to_move()).has(mv.to)
                            {
                                continue;
                            }

                            if self.filter_give_check {
                                let mut b = board.clone();
                                b.play_unchecked(mv);
                                if !b.checkers().is_empty() {
                                    continue;
                                }
                            }

                            if self.filter_mate_scores
                                && (eval > Eval::TB_WIN || eval < -Eval::TB_WIN)
                            {
                                continue;
                            }

                            emit_sample(&mut *output.lock().unwrap(), &board, eval, winner);
                            positions.fetch_add(1, Ordering::Relaxed);
                        }

                        let total = games.fetch_add(1, Ordering::Relaxed) + 1;
                        let completion = total as f64 / total_games as f64;
                        let time = start.elapsed().as_secs_f64();
                        let eta = time / completion - time;
                        print!(
                            "\r\x1b[K{:>6.2}% complete. {} positions so far. Estimated time remaining: {} minutes",
                            completion * 100.0,
                            positions.load(Ordering::Relaxed),
                            eta as i64 / 60,
                        );
                        stdout().flush().unwrap();
                    }
                });
            }
        })
        .unwrap();

        println!();
    }
}

fn emit_sample(mut out: impl Write, board: &Board, eval: Eval, winner: Option<Color>) {
    write_features(&mut out, board, board.side_to_move() == Color::Black);
    write_features(&mut out, board, board.side_to_move() == Color::White);
    out.write_all(&eval.raw().to_le_bytes()).unwrap();
    out.write_all(match (winner, board.side_to_move()) {
        (Some(win), stm) if win == stm => &[2, 0],
        (Some(win), stm) if win != stm => &[0, 0],
        (None, _) => &[1, 0],
        _ => unreachable!(),
    })
    .unwrap();
}

fn write_features(mut out: impl Write, board: &Board, flip: bool) {
    let color_flip = |c: Color| match flip {
        false => c,
        true => !c,
    };
    let sq_flip = |sq: Square| match flip {
        false => sq,
        true => sq.flip_rank(),
    };
    for sq in board.occupied() {
        let index = feature(
            color_flip(board.color_on(sq).unwrap()),
            board.piece_on(sq).unwrap(),
            sq_flip(sq),
        );
        out.write_all(&u16::try_from(index).unwrap().to_le_bytes())
            .unwrap();
    }
    for _ in board.occupied().popcnt()..32 {
        out.write_all(&u16::MAX.to_le_bytes()).unwrap();
    }
}

// note: duplicate of function in /frozenight/src/nnue.rs
fn feature(color: Color, piece: Piece, sq: Square) -> usize {
    sq as usize + Square::NUM * (piece as usize + Piece::NUM * color as usize)
}
