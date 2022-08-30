use std::io::{stdin, stdout, Write};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cozy_chess::{Board, Color, File, Move, Piece, Square};
use frozenight::Frozenight;

mod bench;

fn main() {
    if std::env::args().any(|arg| arg == "bench") {
        bench::bench();
        return;
    }

    let mut frozenight = Frozenight::new(32);

    let mut move_overhead = Duration::from_millis(0);
    let mut abort = None;
    let mut ob_no_adj = false;
    let mut chess960 = false;
    let mut threads = 1;

    let mut buf = String::new();
    loop {
        buf.clear();
        match stdin().read_line(&mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to read command: {}", e);
                std::process::exit(1);
            }
        }
        let now = Instant::now();
        let mut stream = buf.split_ascii_whitespace().peekable();

        let _: Option<()> = (|| {
            match stream.next()? {
                "uci" => {
                    println!(
                        "id name Frozenight {} {}",
                        env!("CARGO_PKG_VERSION"),
                        env!("GIT_HASH")
                    );
                    println!("id author MinusKelvin <mark.carlson@minuskelvin.net>");
                    println!("option name Move Overhead type spin default 0 min 0 max 5000");
                    println!("option name Hash type spin default 32 min 1 max 1048576");
                    println!("option name Threads type spin default 1 min 1 max 64");
                    println!("option name OB_noadj type check default false");
                    println!("option name UCI_Chess960 type check default false");
                    println!("option name SyzygyPath type string default <empty>");
                    println!("uciok");
                }
                "quit" => {
                    std::process::exit(0);
                }
                "isready" => {
                    println!("readyok");
                }
                "setoption" => {
                    stream.find(|&tok| tok == "name")?;
                    let mut opt = String::new();
                    while let Some(tok) = stream.next() {
                        if tok == "value" {
                            break;
                        }
                        if !opt.is_empty() {
                            opt.push(' ');
                        }
                        opt.push_str(tok);
                    }
                    match &*opt {
                        "Move Overhead" => {
                            move_overhead = Duration::from_millis(stream.next()?.parse().ok()?)
                        }
                        "Hash" => {
                            frozenight.set_hash(stream.next()?.parse().ok()?);
                        }
                        "OB_noadj" => {
                            ob_no_adj = stream.next()? == "true";
                        }
                        "UCI_Chess960" => {
                            chess960 = stream.next()? == "true";
                        }
                        "Threads" => {
                            threads = stream.next()?.parse().ok()?;
                        }
                        "SyzygyPath" => {
                            let paths = stream.next()?;
                            frozenight.clear_tb();
                            if paths != "<empty>" {
                                for path in paths.split(':') {
                                    if let Err(e) = frozenight.add_tb_path(path) {
                                        eprintln!("Could not load TB from path `{}`: {}", path, e);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                "position" => {
                    let board = match stream.next()? {
                        "startpos" => Board::default(),
                        "fen" => {
                            let fen_start = stream.next().unwrap().to_owned();
                            let fen = (&mut stream)
                                .take_while(|&tok| tok != "moves")
                                .fold(fen_start, |a, b| a + " " + b);
                            match Board::from_fen(&fen, chess960) {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("Invalid FEN: {:?}", e);
                                    return None;
                                }
                            }
                        }
                        _ => return None,
                    };

                    if stream.peek() == Some(&&"moves") {
                        stream.next();
                    }

                    frozenight.set_position(board, |board| {
                        let mv = stream.peek()?.parse().ok()?;
                        stream.next();
                        Some(from_uci_castling(board, mv, chess960))
                    });
                }
                "go" => {
                    let mut time_available = None;
                    let mut increment = Duration::ZERO;
                    let mut budget_time = false;
                    let mut nodes = u64::MAX;
                    let mut to_go = 45;

                    let mut depth = 250;

                    let stm = frozenight.board().side_to_move();
                    while let Some(param) = stream.next() {
                        match param {
                            "wtime" if stm == Color::White => {
                                time_available = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                budget_time = true;
                            }
                            "btime" if stm == Color::Black => {
                                time_available = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                budget_time = true;
                            }
                            "winc" if stm == Color::White => {
                                increment =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                            }
                            "binc" if stm == Color::Black => {
                                increment =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                            }
                            "movetime" => {
                                time_available = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                budget_time = false;
                            }
                            "movestogo" => to_go = stream.next().unwrap().parse().unwrap(),
                            "depth" => depth = stream.next().unwrap().parse().unwrap(),
                            "nodes" => nodes = stream.next().unwrap().parse().unwrap(),
                            _ => {}
                        }
                    }

                    let time_use_suggestion = time_available.map(|amt| match budget_time {
                        true => {
                            amt.min((amt.saturating_sub(increment) / (to_go + 5)) + increment / 2)
                        }
                        false => amt,
                    });

                    abort = Some(frozenight.start_search(
                        time_use_suggestion.map(|d| {
                            now + d
                                .saturating_sub(move_overhead)
                                .max(Duration::from_millis(1))
                        }),
                        time_available.map(|d| {
                            now + (d / 2)
                                .saturating_sub(move_overhead)
                                .max(Duration::from_millis(1))
                        }),
                        depth,
                        nodes,
                        threads,
                        move |depth, stats, eval, board, pv| {
                            let time = now.elapsed();
                            let nodes = stats.nodes.load(Ordering::Relaxed);
                            print!(
                                "info depth {} seldepth {} nodes {} nps {} tbhits {} score {} time {} pv",
                                depth,
                                stats.selective_depth.load(Ordering::Relaxed),
                                nodes,
                                (nodes as f64 / time.as_secs_f64()).round() as u64,
                                stats.tb_probes.load(Ordering::Relaxed),
                                match ob_no_adj {
                                    true => frozenight::Eval::new(250),
                                    false => eval,
                                },
                                time.as_millis()
                            );
                            let mut board = board.clone();
                            for &mv in pv {
                                print!(" {}", to_uci_castling(&board, mv, chess960));
                                board.play(mv);
                            }
                            println!();
                        },
                        move |_, mv, board| {
                            println!("bestmove {}", to_uci_castling(board, mv, chess960));
                            stdout().flush().unwrap();
                        },
                    ));
                }
                "stop" => {
                    abort = None;
                }
                _ => {}
            }
            None
        })();
    }
}

fn to_uci_castling(board: &Board, mut mv: Move, chess960: bool) -> Move {
    if chess960 {
        return mv;
    }
    if board.color_on(mv.from) == board.color_on(mv.to) {
        if mv.to.file() > mv.from.file() {
            mv.to = Square::new(File::G, mv.to.rank());
        } else {
            mv.to = Square::new(File::C, mv.to.rank());
        }
    }
    mv
}

fn from_uci_castling(board: &Board, mut mv: Move, chess960: bool) -> Move {
    if chess960 {
        return mv;
    }
    if mv.from.file() == File::E && board.piece_on(mv.from) == Some(Piece::King) {
        if mv.to.file() == File::G {
            mv.to = Square::new(File::H, mv.to.rank());
        } else if mv.to.file() == File::C {
            mv.to = Square::new(File::A, mv.to.rank());
        }
    }
    mv
}
