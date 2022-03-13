use std::time::{Duration, Instant};

use tantabus::search::*;
use tantabus::search::SearchResult;

//Randomly sampled from self play games
const POSITIONS: &[&str] = &[
    "Q7/5Q2/8/8/3k4/6P1/6BP/7K b - - 0 67",
    "r4rk1/p4ppp/1q2p3/2n1P3/2p5/3bRNP1/1P3PBP/R2Q2K1 b - - 0 24",
    "r1bq1rk1/pp3ppp/2nbpn2/3p4/3P4/1PN1PN2/1BP1BPPP/R2Q1RK1 b - - 2 10",
    "1r4k1/1P3p2/6pp/2Pp4/4P3/PQ1K1R2/6P1/4q3 w - - 0 51",
    "8/8/R7/4n3/4k3/6P1/6K1/8 w - - 68 164",
    "2r3k1/1b4bp/1p2p1p1/3pNp2/3P1P1q/PB1Q3P/1P4P1/4R1K1 w - - 2 36",
    "4rrk1/1b4bp/p1p1p1p1/3pN3/1P3q2/PQN3P1/2P1RP1P/3R2K1 b - - 0 24",
    "rnbq1rk1/ppp1bppp/4p3/3pP1n1/2PP3P/5PP1/PP4B1/RNBQK1NR b KQ - 0 8",
    "3r1r1k/p1p3pp/2p5/8/4K3/2N3Pb/PPP5/R1B4R b - - 0 20",
    "r4k1r/ppq2ppp/4bB2/8/2p5/4P3/P3BPPP/1R1Q1RK1 b - - 0 17",
    "r4rk1/1b1nq1pp/p7/3pNp2/1p3Q2/3B3P/PPP1N1R1/R2K4 w - - 2 21",
    "8/5p2/8/p6k/8/3N4/5PPK/8 w - - 0 49",
    "2r1rbk1/4pp1p/1Q1P1np1/2B1Nq2/P4P2/1B3P2/1PP3bP/1K1RR3 b - - 0 29",
    "6k1/p4ppp/Bpp5/4P3/P7/4QKPb/2P3N1/3r3q w - - 5 36",
    "3br1k1/pp1r1ppp/3pbn2/P2Np3/1PPpP3/3P1NP1/5PBP/3RR1K1 w - - 1 21",
    "8/1p6/p3n3/4k3/8/6PR/1rr5/3R2K1 w - - 8 54",
    "1r4k1/p4p1p/5p2/8/4P3/4K3/PPP3P1/4R3 w - - 0 34",
    "6k1/6p1/7p/7R/7P/5n2/P3K1b1/8 b - - 2 48",
    "2rr2k1/pp5p/3p4/4p3/2b1p3/P4QP1/1P4P1/3R2K1 w - - 0 28",
    "q1r4k/1bR5/rp4pB/3p4/3P2nQ/8/PP3PPP/R5K1 w - - 1 29",
    "rnbqkbnr/pppppp1p/6p1/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
    "rnbqk1nr/1p3ppp/p3p3/2bp4/4P3/5N2/PPPN1PPP/R1BQKB1R w KQkq - 0 6",
    "r2q1rk1/1p1b1p1p/p5p1/3QP3/8/5N2/PP3PPP/2KR3R b - - 0 20",
    "r3r2k/pbp1q2p/1p6/4n3/2NQ4/2P2pB1/P1P2P1P/2R2RK1 b - - 6 26",
    "8/1p2k3/4rp2/p2R3Q/2q2B2/6P1/5P1P/6K1 b - - 14 73",
];

const DEPTH: u8 = 10;
const CACHE: usize = 16_000_000;

#[derive(Default)]
struct BenchHandler {
    nodes: u64,
    prev_result: Option<SearchResult>
}

impl SearchHandler for BenchHandler {
    fn stop_search(&self) -> bool {
        self.prev_result.as_ref().map(|r| r.depth >= DEPTH).unwrap_or_default()
    }

    fn new_result(&mut self, search_result: SearchResult) {
        self.nodes += search_result.nodes;
        self.prev_result = Some(search_result);
    }
}

pub fn bench() {
    let mut total_time = Duration::ZERO;
    let mut total_nodes = 0;
    for position in POSITIONS {
        let init_pos = position.parse().unwrap();
        let mut handler = BenchHandler::default();
        let mut state = Engine::new(
            &mut handler,
            init_pos,
            Vec::new(),
            EngineOptions::default(),
            CacheTable::new_with_size(CACHE).unwrap()
        );
        let start_time = Instant::now();
        state.search();
        total_time += start_time.elapsed();
        total_nodes += handler.nodes;
    }
    let nps = (total_nodes as f32 / total_time.as_secs_f32()) as u64;
    println!("{} nodes {} nps", total_nodes, nps);
}
