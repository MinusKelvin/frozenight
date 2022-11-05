use std::io::{stdin, stdout, Write};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Color, File, GameStatus, Move, Piece, Square};
use frozenight::{MtFrozenight, TimeConstraint};

mod bench;

fn main() {
    if std::env::args().any(|arg| arg == "bench") {
        bench::bench();
        return;
    }

    let mut frozenight = MtFrozenight::new(32);

    let mut move_overhead = Duration::from_millis(0);
    let mut ob_no_adj = false;
    let mut chess960 = false;

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
                variant @ ("uci" | "ugi") => {
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
                    #[cfg(feature = "tweakable")]
                    for param in frozenight::all_parameters() {
                        println!(
                            "option name {} type spin default {} min {} max {}",
                            param.name(),
                            param.default,
                            param.min,
                            param.max
                        );
                    }
                    println!("{}ok", variant);
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
                            frozenight.set_threads(stream.next()?.parse().ok()?);
                        }
                        _ =>
                        {
                            #[cfg(feature = "tweakable")]
                            for param in frozenight::all_parameters() {
                                if opt != param.name() {
                                    continue;
                                }
                                param.set(stream.next()?.parse().ok()?);
                                break;
                            }
                        }
                    }
                }
                "ucinewgame" | "uginewgame" => {
                    frozenight.new_game();
                }
                "position" => {
                    let mut board = match stream.next()? {
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

                    frozenight.set_position(
                        board.clone(),
                        std::iter::from_fn(|| {
                            let mv =
                                from_uci_castling(&board, stream.peek()?.parse().ok()?, chess960);
                            stream.next();
                            board.play(mv);
                            Some(mv)
                        }),
                    );
                }
                "query" => match stream.next()? {
                    "gameover" => println!(
                        "response {}",
                        frozenight.board().status() != GameStatus::Ongoing
                    ),
                    "p1turn" => println!(
                        "response {}",
                        frozenight.board().side_to_move() == Color::White
                    ),
                    "result" => println!(
                        "response {}",
                        match frozenight.board().status() {
                            GameStatus::Won => match frozenight.board().side_to_move() {
                                Color::White => "p2win",
                                Color::Black => "p1win",
                            },
                            GameStatus::Drawn => "draw",
                            GameStatus::Ongoing => "none",
                        }
                    ),
                    _ => {}
                },
                "go" => {
                    let mut clock = None;
                    let mut increment = Duration::ZERO;
                    let mut use_all_time = true;
                    let mut nodes = u64::MAX;
                    let mut moves_to_go = None;

                    let mut depth = 250;

                    let stm = frozenight.board().side_to_move();
                    while let Some(param) = stream.next() {
                        match param {
                            "wtime" | "p1time" if stm == Color::White => {
                                clock = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                use_all_time = false;
                            }
                            "btime" | "p2time" if stm == Color::Black => {
                                clock = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                use_all_time = false;
                            }
                            "winc" | "p1inc" if stm == Color::White => {
                                increment =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                            }
                            "binc" | "p2inc" if stm == Color::Black => {
                                increment =
                                    Duration::from_millis(stream.next().unwrap().parse().unwrap());
                            }
                            "movetime" => {
                                clock = Some(Duration::from_millis(
                                    stream.next().unwrap().parse().unwrap(),
                                ));
                                use_all_time = true;
                            }
                            "movestogo" => {
                                moves_to_go = Some(stream.next().unwrap().parse().unwrap())
                            }
                            "depth" => depth = stream.next().unwrap().parse().unwrap(),
                            "nodes" => nodes = stream.next().unwrap().parse().unwrap(),
                            _ => {}
                        }
                    }

                    let board1 = frozenight.board().clone();
                    let board2 = frozenight.board().clone();
                    frozenight.search(
                        TimeConstraint {
                            nodes,
                            depth,
                            clock,
                            increment,
                            overhead: move_overhead,
                            moves_to_go,
                            use_all_time,
                        },
                        move |info| {
                            let time = now.elapsed();
                            print!(
                                "info depth {} seldepth {} nodes {} nps {} score {} time {} hashfull {} pv",
                                info.depth,
                                info.selective_depth,
                                info.nodes,
                                (info.nodes as f64 / time.as_secs_f64()).round() as u64,
                                match ob_no_adj {
                                    true => frozenight::Eval::new(250),
                                    false => info.eval,
                                },
                                time.as_millis(),
                                info.hashfull,
                            );
                            let mut board = board1.clone();
                            for &mv in &info.pv {
                                print!(" {}", to_uci_castling(&board, mv, chess960));
                                board.play(mv);
                            }
                            println!();
                        },
                        move |info| {
                            println!(
                                "bestmove {}",
                                to_uci_castling(&board2, info.best_move, chess960)
                            );
                            stdout().flush().unwrap();
                        },
                    );
                }
                "stop" => {
                    frozenight.abort();
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
