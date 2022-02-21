use std::io::{stdin, stdout, Write};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Color, File, Move, Piece, Square};
use frozenight::{Eval, Frozenight, Listener, Statistics};

mod bench;

fn main() {
    if std::env::args().any(|arg| arg == "bench") {
        bench::bench();
        return;
    }

    let mut frozenight = Frozenight::new(32);

    let mut move_overhead = Duration::from_millis(1);
    let mut abort = None;

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
                    println!("id name Frozenight {}", env!("CARGO_PKG_VERSION"));
                    println!("id author MinusKelvin <mark.carlson@minuskelvin.net>");
                    println!("option name Move Overhead type spin default 1 min 0 max 5000");
                    println!("option name Hash type spin default 32 min 1 max 65536");
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
                            frozenight = Frozenight::new(stream.next()?.parse().ok()?);
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
                            match fen.parse() {
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
                        Some(from_uci_castling(board, mv))
                    });
                }
                "go" => {
                    let mut deadline = None;
                    let mut time_use_suggestion = None;
                    let mut depth = 250;

                    let stm = frozenight.board().side_to_move();
                    while let Some(param) = stream.next() {
                        match param {
                            "wtime" if stm == Color::White => {
                                let time =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                                deadline = Some(time / 10);
                                time_use_suggestion = Some(time / 40);
                            }
                            "btime" if stm == Color::Black => {
                                let time =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                                deadline = Some(time / 10);
                                time_use_suggestion = Some(time / 40);
                            }
                            "movetime" => {
                                deadline = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ))
                            }
                            "depth" => depth = stream.next().unwrap().parse().unwrap(),
                            _ => {}
                        }
                    }

                    abort = Some(frozenight.start_search(
                        time_use_suggestion.map(|d| now + d),
                        deadline.map(|d| now + d - move_overhead),
                        depth,
                        UciListener(now),
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

fn to_uci_castling(board: &Board, mut mv: Move) -> Move {
    if board.color_on(mv.from) == board.color_on(mv.to) {
        if mv.to.file() > mv.from.file() {
            mv.to = Square::new(File::G, mv.to.rank());
        } else {
            mv.to = Square::new(File::C, mv.to.rank());
        }
    }
    mv
}

fn from_uci_castling(board: &Board, mut mv: Move) -> Move {
    if mv.from.file() == File::E && board.piece_on(mv.from) == Some(Piece::King) {
        if mv.to.file() == File::G {
            mv.to = Square::new(File::H, mv.to.rank());
        } else if mv.to.file() == File::C {
            mv.to = Square::new(File::A, mv.to.rank());
        }
    }
    mv
}

struct UciListener(Instant);

impl Listener for UciListener {
    fn info(&mut self, depth: u16, stats: Statistics, eval: Eval, board: &Board, pv: &[Move]) {
        let time = self.0.elapsed();
        print!(
            "info depth {} seldepth {} nodes {} nps {} score {} time {} pv",
            depth,
            stats.selective_depth,
            stats.nodes,
            (stats.nodes as f64 / time.as_secs_f64()).round() as u64,
            eval,
            self.0.elapsed().as_millis()
        );
        let mut board = board.clone();
        for &mv in pv {
            print!(" {}", to_uci_castling(&board, mv));
            board.play(mv);
        }
        println!();
    }

    fn best_move(self, mv: Move, _: Eval) {
        println!("bestmove {}", mv);
        stdout().flush().unwrap();
    }
}
