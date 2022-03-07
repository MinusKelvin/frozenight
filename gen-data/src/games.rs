use std::fs::OpenOptions;
use std::io::{stdout, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cozy_chess::{Board, Color, Piece, Square};
use cozy_syzygy::Tablebase;

use crate::Sample;

pub fn generate_games(syzygy_path: Option<PathBuf>, concurrency: usize, count: usize) {
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open("data.bin")
        .unwrap_or_else(|e| {
            eprintln!("Could not create data.bin: {}", e);
            std::process::exit(1)
        });
    let output = Arc::new(Mutex::new(BufWriter::new(output)));

    let tb = syzygy_path.map(Tablebase::new).map(Arc::new);
    if let Some(tb) = tb.as_ref() {
        println!("Using tablebase adjudication with {} men", tb.max_pieces());
    }
    let game_counter = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let tb = tb.clone();
            let game_counter = game_counter.clone();
            let output = output.clone();
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

fn sample_game(tb: Option<&Tablebase>, output: &Mutex<impl Write>) -> usize {
    let (game, winner) = super::play_game(8, tb);

    let mut samples = 0;
    for sample in game {
        if sample.board.color_on(sample.mv.to) == Some(!sample.board.side_to_move()) {
            // skip captures
            continue;
        }

        if !sample.board.checkers().is_empty() {
            // skip in check
            continue;
        }

        samples += 1;
        emit_sample(&mut *output.lock().unwrap(), &sample, winner);
    }

    samples
}

fn emit_sample(mut out: impl Write, sample: &Sample, winner: Option<Color>) {
    write_features(&mut out, &sample.board, sample.board.side_to_move() == Color::Black);
    write_features(&mut out, &sample.board, sample.board.side_to_move() == Color::White);
    out.write_all(&sample.eval.raw().to_le_bytes()).unwrap();
    out.write_all(match (winner, sample.board.side_to_move()) {
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
