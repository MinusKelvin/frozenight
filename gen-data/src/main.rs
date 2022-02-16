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
    let handles: Vec<_> = (0..options.concurrency)
        .map(|_| {
            let tb = tb.clone();
            let game_counter = game_counter.clone();
            let output = output.clone();
            let count = options.count;
            std::thread::spawn(move || loop {
                let samples = sample_game(tb.as_deref(), &output);
                let old = game_counter.fetch_add(samples, Ordering::SeqCst);
                if old * 100 / count < (old + samples) * 100 / count {
                    print!("\r{}%", (old + samples) * 100 / count);
                    stdout().flush().unwrap()
                }
                if old + samples >= count {
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
        engine
            .start_search(
                Some(Instant::now() + Duration::from_millis(10)),
                5000,
                move |mv, _| mvsend.send(mv).unwrap(),
            )
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
    let mut board = Board::default();
    let mut samples = 0;
    for mv in game {
        if board.checkers().is_empty() && board.color_on(mv.to) != Some(!board.side_to_move()) {
            emit_sample(&mut *output, &board, winner);
            samples += 1;
        }

        board.play(mv);
    }

    samples
}

fn emit_sample(mut out: impl Write, board: &Board, winner: Option<Color>) {
    let color_flip = |c: Color| match board.side_to_move() {
        Color::White => c,
        Color::Black => !c,
    };
    let sq_flip = |sq: Square| match board.side_to_move() {
        Color::White => sq,
        Color::Black => sq.flip_rank(),
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
    out.write_all(match winner.map(color_flip) {
        Some(Color::White) => &[2, 0],
        None => &[1, 0],
        Some(Color::Black) => &[0, 0],
    })
    .unwrap();
}

// note: duplicate of function in /frozenight/src/nnue.rs
fn feature(color: Color, piece: Piece, sq: Square) -> usize {
    sq as usize + Square::NUM * (piece as usize + Piece::NUM * color as usize)
}
