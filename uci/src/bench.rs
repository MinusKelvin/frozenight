use std::sync::mpsc::{sync_channel, SyncSender};
use std::time::{Duration, Instant};

use cozy_chess::{Board, Move};
use frozenight::{Eval, Frozenight, Listener, Statistics};

// generated from self-play
const POSITIONS: &[&str] = &[
    "8/5pkp/1p3np1/8/1r3B2/6PB/2P1K2P/8 b - - 1 35",
    "r1bqkb1r/1ppp1pp1/p1n1pn2/7p/2B1P2P/2NP4/PPP2PP1/R1BQK1NR w KQkq - 0 6",
    "r1b1k2B/ppppqp1p/2n2n2/4p3/4P3/5N2/P1PNKPPP/Q4B1R w q - 1 10",
    "5rk1/5p2/3pp1p1/p1q1n2p/4PN1P/6P1/5RB1/3Q2K1 b - - 0 30",
    "2k5/6pp/2n5/1pp3r1/P7/1P6/1B6/R1K5 b - a3 0 38",
    "1k6/6p1/7p/2p1N1r1/1p1PB3/1P2P3/P1P2K2/R7 w - c6 0 30",
    "r1b1kb1r/pp2pp2/2p1n1p1/6Np/2PP4/P2BB3/1P3PPP/3R1RK1 w - - 1 18",
    "8/2b5/4B1k1/1b1R3p/p3P1pP/6P1/5P2/6K1 b - - 1 38",
    "8/8/3k4/1p6/3p1p2/P3n1p1/1KP5/2N5 b - - 1 48",
    "r2r2n1/q5k1/1pbNp2p/2npPp1Q/3R1B2/P5P1/1PP4P/3R1BK1 w - - 2 28",
    "8/pp3k2/4b2p/P1p4p/2N5/4r3/KP2r2P/1RR5 w - - 0 29",
    "r3kb1r/p4pp1/Ppp5/3p4/1P1P2b1/2P5/3N3P/R1B1K2R w KQkq - 0 21",
    "4r2r/p1pQnk1p/2N2p2/1pq5/5P2/1P6/PP4P1/R1B1RK2 b - - 2 28",
    "8/2Q2bkp/r5p1/3p2P1/3P1PN1/r3BB1P/Pq3P2/2R3K1 b - - 2 38",
    "r7/p1R2p1p/1p2kBr1/4P3/4N3/1b4P1/1P5P/4K3 w - - 2 27",
    "r1bqkbnr/p1p1pp1p/2n3p1/3p4/1pN5/1PP3P1/P2PPP1P/R1BQKBNR w KQkq d6 0 6",
    "1r2kb1r/pp1b1ppp/4p1n1/2npP2N/3p1B2/5N2/PqP1BPPP/R2Q1RK1 w k - 2 14",
    "r2r2k1/pbqp1ppp/1p1p1n2/1P2p3/2P1P3/3P1NPP/P2Q1PB1/R4RK1 b - - 0 19",
    "2r1r1k1/2Q2pp1/1p6/3p2B1/P6p/PB3P1P/4q3/3R2K1 w - - 3 37",
    "1k1r3r/p1qnbp2/2p4p/3p4/3PPQp1/2P5/PP1NB1P1/R1B1K3 w Q - 0 21",
];

pub fn bench() {
    let mut total_time = Duration::ZERO;
    let mut total_nodes = 0;

    let (send, recv) = sync_channel(0);

    for &pos in POSITIONS {
        let mut engine = Frozenight::new(16);
        engine.set_position(pos.parse().unwrap(), |_| None);

        let start = Instant::now();
        engine
            .start_search(
                None,
                None,
                8,
                BenchListener {
                    sender: send.clone(),
                    nodes: 0,
                },
            )
            .forget();

        total_nodes += recv.recv().unwrap();
        total_time += start.elapsed();
    }

    let nps = (total_nodes as f64 / total_time.as_secs_f64()) as u64;
    println!("{} nodes {} nps", total_nodes, nps);
}

struct BenchListener {
    sender: SyncSender<u64>,
    nodes: u64,
}

impl Listener for BenchListener {
    fn info(&mut self, _: u16, stats: Statistics, _: Eval, _: &Board, _: &[Move]) {
        self.nodes = stats.nodes;
    }

    fn best_move(self, _: Move, _: Eval) {
        self.sender.send(self.nodes).unwrap();
    }
}
