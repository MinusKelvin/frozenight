use std::convert::TryInto;
use std::io::{BufRead, BufReader, Write, stdin};
use std::time::{Instant, Duration};
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};
use std::sync::atomic::{AtomicBool, Ordering};

use cozy_chess::*;
use vampirc_uci::{UciFen, UciInfoAttribute, UciMessage, UciMove, UciPiece, UciSquare, UciTimeControl};
use tantabus::eval::*;
use tantabus::search::*;
use tantabus::time::*;

mod options;
mod convert;
mod bench;

use options::UciOptionsHandler;
use convert::*;

const VERSION: &str = "2.0.0";

struct UciHandler {
    time_manager: StandardTimeManager,
    search_begin: Instant,
    last_update: Instant,
    time_left: Duration,
    search_terminator: Arc<AtomicBool>,
    event_sink: Sender<Event>,
    total_nodes: u64,
    prev_result: Option<SearchResult>
}

impl SearchHandler for UciHandler {
    fn stop_search(&self) -> bool {
        self.time_left < self.last_update.elapsed() ||
        self.search_terminator.load(Ordering::Acquire)
    }

    fn new_result(&mut self, mut result: SearchResult) {
        self.time_left = self.time_manager.update(result.clone(), self.last_update.elapsed());
        self.last_update = Instant::now();
        self.prev_result = Some(result.clone());
        self.total_nodes += result.nodes;
        result.nodes = self.total_nodes;
        self.event_sink.send(
            Event::EngineSearchUpdate(
                EngineSearchResult::SearchInfo(
                    result,
                    self.search_begin.elapsed()
                )
            )
        ).unwrap();
    }
}

impl UciHandler {
    fn finish(mut self, cache_table: CacheTable) {
        self.event_sink.send(
            Event::EngineSearchUpdate(
                EngineSearchResult::SearchFinished(
                    self.prev_result.take().unwrap(),
                    cache_table
                )
            )
        ).unwrap();
    }
}

enum EngineSearchResult {
    SearchInfo(SearchResult, Duration),
    SearchFinished(SearchResult, CacheTable)
}

fn send_message(message: UciMessage) {
    println!("{}", message);
    std::io::stdout().flush().unwrap();
}

enum Event {
    UciMessage(UciMessage),
    EngineSearchUpdate(EngineSearchResult)
}

fn parse_message(msg: &str) -> UciMessage {
    fn parse_square(chars: &mut impl Iterator<Item=char>) -> Option<UciSquare> {
        let file = chars.next()?;
        if !('a'..='h').contains(&file) {
            return None;
        }
        let rank = chars.next()?.to_digit(10)? as u8;
        if !(1..=8).contains(&rank) {
            return None;
        }
        Some(UciSquare {
            file,
            rank
        })
    }
    fn try_parse(msg: &str) -> Option<UciMessage> {
        let mut parts = msg.split(' ');
        let kind = parts.next()?;
        if kind == "position" {
            if parts.next()? != "fen" {
                return None;
            }
            let mut fen = parts.next()?.to_owned();
            for _ in 0..5 {
                fen.push(' ');
                fen.push_str(parts.next()?);
            }
            if Board::from_fen(&fen, true).is_err() {
                return None;
            }
            let fen = UciFen(fen);

            let mut moves = Vec::new();
            if parts.next() == Some("moves") {
                for mv in parts {
                    let mut chars = mv.chars();
                    let from = parse_square(&mut chars)?;
                    let to = parse_square(&mut chars)?;
                    let promotion =
                        if let Some(p) = chars.next() {
                            Some(match p {
                                'q' => UciPiece::Queen,
                                'k' => UciPiece::Knight,
                                'r' => UciPiece::Rook,
                                'b' => UciPiece::Bishop,
                                _ => return None
                            })
                        } else {
                            None
                        };
                    if chars.next().is_some() {
                        return None;
                    }
                    moves.push(UciMove {
                        from,
                        to,
                        promotion
                    });
                }
            }
            return Some(UciMessage::Position {
                startpos: false,
                fen: Some(fen),
                moves
            });
        }
        None
    }
    let mut msg = vampirc_uci::parse_one(msg);
    if let UciMessage::Unknown(raw_msg, _) = &msg {
        if let Some(parsed) = try_parse(raw_msg) {
            msg = parsed;
        }
    }
    msg
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("bench") {
        bench::bench();
        return;
    }
    
    let mut position: Option<(Board, Vec<Move>)> = None;
    let mut search = None;
    let mut cache_table = None;

    let mut options = UciOptionsHandler::new();

    let (event_sink, events) = channel();
    std::thread::spawn({
        let event_sink = event_sink.clone();
        move || {
            let mut lines = BufReader::new(stdin()).lines();
            while let Some(Ok(line)) = lines.next() {
                let msg = parse_message(&line);
                let _ = event_sink.send(Event::UciMessage(msg));
            }
            let _ = event_sink.send(Event::UciMessage(UciMessage::Quit));
        }
    });

    'main: while let Ok(event) = events.recv() {
        match event {
            Event::UciMessage(message) => match message {
                UciMessage::Uci => {
                    send_message(UciMessage::id_name(&format!("Tantabus {}", VERSION)));
                    send_message(UciMessage::id_author("Analog Hors"));
                    for (option, _) in options.handlers.values() {
                        send_message(UciMessage::Option(option.clone()));
                    }
                    send_message(UciMessage::UciOk);
                }
                UciMessage::Debug(_) => {}
                UciMessage::IsReady => send_message(UciMessage::ReadyOk),
                UciMessage::SetOption { name, value } => {
                    options.update(&name, value);
                }
                UciMessage::UciNewGame => cache_table = None,
    
                UciMessage::Position { fen, moves, .. } => {
                    let board: Board = fen
                        .map(|fen| {
                            Board::from_fen(fen.as_str(), options.options.chess960)
                                .unwrap()
                        })
                        .unwrap_or_default();
                    let mut converted_moves = Vec::new();
                    let mut current_pos = board.clone();
                    for mv in moves {
                        let mv = mv.uci_move_into(&current_pos, options.options.chess960);
                        current_pos.play_unchecked(mv);
                        converted_moves.push(mv);
                    }
                    position = Some((board, converted_moves));
                }
                UciMessage::Go { time_control, search_control } => {
                    let time_manager;
                    time_manager = match time_control {
                        Some(UciTimeControl::MoveTime(time)) => StandardTimeManager::new(
                            Duration::ZERO,
                            0.0,
                            time.to_std().unwrap()
                        ),
                        Some(UciTimeControl::TimeLeft {
                            white_time,
                            black_time,
                            ..
                        }) => {
                            let (initial_pos, moves) = position.as_ref().unwrap();
                            let side_to_move = if moves.len() % 2 == 0 {
                                initial_pos.side_to_move()
                            } else {
                                !initial_pos.side_to_move()
                            };
                            let time_left = match side_to_move {
                                Color::White => white_time,
                                Color::Black => black_time
                            }.unwrap().to_std().unwrap();
                            StandardTimeManager::new(
                                time_left, 
                                options.options.percent_time_used_per_move,
                                options.options.minimum_time_used_per_move
                            )
                        }
                        Some(UciTimeControl::Ponder) => todo!("Ponder is unimplemented."),
                        None | Some(UciTimeControl::Infinite) => StandardTimeManager::new(
                            Duration::ZERO,
                            0.0,
                            Duration::MAX
                        )
                    };
                    
                    options.options.engine_options.max_depth = 64u8.try_into().unwrap();
                    if let Some(search_control) = search_control {
                        if let Some(depth) = search_control.depth {
                            options.options.engine_options.max_depth = depth.try_into().unwrap();
                        }
                        //TODO implement the rest
                        if let Some(_) = search_control.nodes {
                            let warn = "WARNING: The nodes search control is currently unimplemented.";
                            send_message(UciMessage::info_string(warn.to_owned()));
                        }
                        if let Some(_) = search_control.mate {
                            let warn = "WARNING: The mate search control is currently unimplemented.";
                            send_message(UciMessage::info_string(warn.to_owned()));
                        }
                        if !search_control.search_moves.is_empty() {
                            let warn = "WARNING: The search_moves search control is currently unimplemented.";
                            send_message(UciMessage::info_string(warn.to_owned()));
                        }
                    }
                    let (init_pos, moves) = position.clone().unwrap();
                    let terminator = Arc::new(AtomicBool::new(false));
                    let mut handler = UciHandler {
                        time_manager,
                        search_begin: Instant::now(),
                        last_update: Instant::now(),
                        time_left: Duration::MAX,
                        search_terminator: terminator.clone(),
                        event_sink: event_sink.clone(),
                        total_nodes: 0,
                        prev_result: None,
                    };
                    std::thread::spawn({
                        let cache_table_size = options.options.cache_table_size;
                        let cache_table = cache_table
                            .take()
                            .unwrap_or_else(|| CacheTable::new_with_size(cache_table_size).unwrap());
                        let options = options.options.engine_options.clone();
                        move || {
                            let mut search_state = Engine::new(
                                &mut handler,
                                init_pos,
                                moves,
                                options,
                                cache_table
                            );
                            search_state.search();
                            let cache_table = search_state.into_cache_table();
                            handler.finish(cache_table);
                        }
                    });
                    search = Some(terminator);
                }
                UciMessage::Stop => if let Some(search) = &mut search {
                    search.store(true, Ordering::Release);
                },

                UciMessage::PonderHit => {}
                UciMessage::Quit => break 'main,
                UciMessage::Register { .. } => {}
                UciMessage::Unknown(..) => {}
                //Engine to GUI messages
                _ => {}
            }
            Event::EngineSearchUpdate(result) => match result {
                EngineSearchResult::SearchInfo(result, duration) => {
                    let tt_filledness =
                        result.used_cache_entries
                        * 1000
                        / result.total_cache_entries;
                    let (board, moves) = position.as_ref().unwrap();
                    let mut current_pos = board.clone();
                    for &mv in moves {
                        current_pos.play_unchecked(mv);
                    }
                    let mut principal_variation = Vec::new();
                    for mv in result.principal_variation {
                        let uci_mv = mv.uci_move_into(&current_pos, options.options.chess960);
                        principal_variation.push(uci_mv);
                        current_pos.play_unchecked(mv);
                    }
                    send_message(UciMessage::Info(vec![
                        match result.eval.kind() {
                            EvalKind::Centipawn(cp) => UciInfoAttribute::from_centipawns(cp as i32),
                            EvalKind::MateIn(m) => UciInfoAttribute::from_mate(((m + 1) / 2) as i8),
                            EvalKind::MatedIn(m) => UciInfoAttribute::from_mate(-(((m + 1) / 2) as i8))
                        },
                        UciInfoAttribute::Depth(result.depth),
                        UciInfoAttribute::SelDepth(result.seldepth),
                        UciInfoAttribute::Nodes(result.nodes),
                        UciInfoAttribute::Pv(principal_variation),
                        UciInfoAttribute::Time(vampirc_uci::Duration::from_std(duration).unwrap()),
                        UciInfoAttribute::HashFull(tt_filledness as u16)
                    ]));
                }
                EngineSearchResult::SearchFinished(result, cache) => {
                    cache_table = Some(cache);
                    let (board, moves) = position.as_ref().unwrap();
                    let mut current_pos = board.clone();
                    for &mv in moves {
                        current_pos.play_unchecked(mv);
                    }
                    let mv = result.mv.uci_move_into(&current_pos, options.options.chess960);
                    send_message(UciMessage::best_move(mv));
                    search = None;
                }
            }
        }
    }
}
