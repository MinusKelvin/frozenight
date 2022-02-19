use std::collections::HashMap;
use std::fs::File;
use std::io::{stdout, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Color, GameStatus, Piece, Square};
use cozy_syzygy::{Tablebase, Wdl};
use frozenight::Frozenight;
use rand::prelude::*;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Options {
    #[structopt(short = "p", long, default_value = "1")]
    concurrency: usize,
    #[structopt(short = "s", long)]
    syzygy_path: Option<PathBuf>,

    count: usize,
}

fn main() {
    let options = Options::from_args();

    let output = File::options()
        .create_new(true)
        .write(true)
        .open("data.bin")
        .unwrap_or_else(|e| {
            eprintln!("Could not create data.bin: {e}");
            std::process::exit(1)
        });
    let output = Arc::new(Mutex::new(BufWriter::new(output)));

    let tb = options.syzygy_path.map(Tablebase::new).map(Arc::new);
    if let Some(tb) = tb.as_ref() {
        println!("Using tablebase adjudication with {} men", tb.max_pieces());
    }
    let game_counter = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let handles: Vec<_> = (0..options.concurrency)
        .map(|_| {
            let tb = tb.clone();
            let game_counter = game_counter.clone();
            let output = output.clone();
            let count = options.count;
            std::thread::spawn(move || loop {
                let samples = sample_game(tb.as_deref(), &output);
                let total = samples + game_counter.fetch_add(samples, Ordering::SeqCst);
                let completion = total as f64 / count as f64;
                let time = start.elapsed().as_secs_f64();
                let eta = time / completion - time;
                print!(
                    "\r\x1b[K{:>6.2}% complete. {:.0} samples/sec. Estimated time remaining: {} minutes",
                    completion * 100.0,
                    total as f64 / time,
                    eta as i64 / 60,
                );
                stdout().flush().unwrap();
                if total >= count {
                    break;
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    println!();
}

fn sample_game(tb: Option<&Tablebase>, output: &Mutex<BufWriter<File>>) -> usize {
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
            return 0;
        }
        let entry = history.entry(board.clone()).or_default();
        *entry += 1;
        if *entry >= 3 {
            return 0;
        }
        let mv = *moves.choose(&mut thread_rng()).unwrap();
        board.play_unchecked(mv);
        game.push(mv);
    }
    if board.status() != GameStatus::Ongoing {
        return 0;
    }

    let mut engine = Frozenight::new(64);
    let (mvsend, mvrecv) = sync_channel(0);

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

        let mut moves = game.iter().copied();
        engine.set_position(Board::default(), |_| moves.next());

        let mvsend = mvsend.clone();
        let alarm = Instant::now() + Duration::from_millis(10);
        engine
            .start_search(None, Some(alarm), 5000, move |mv, _| mvsend.send(mv).unwrap())
            .forget();

        let mv = mvrecv.recv().unwrap();
        game.push(mv);
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

    let mut output = output.lock().unwrap();
    let mut moves = game.into_iter();
    let mut board = Board::default();
    for mv in (&mut moves).take(8) {
        board.play(mv);
    }
    let mut samples = 0;
    for mv in moves {
        // Don't sample positions in check or where the "best" move is a capture
        // These are noisy positions that the search is supposed to take care of
        if board.checkers().is_empty() && board.color_on(mv.to) != Some(!board.side_to_move()) {
            emit_sample(&mut *output, &board, winner);
            samples += 1;
        }

        board.play(mv);
    }

    samples
}

fn emit_sample(mut out: impl Write, board: &Board, winner: Option<Color>) {
    write_features(&mut out, board, board.side_to_move() == Color::Black);
    write_features(&mut out, board, board.side_to_move() == Color::White);
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
