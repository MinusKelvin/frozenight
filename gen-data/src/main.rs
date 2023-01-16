use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use cozy_syzygy::Tablebase;
use once_cell::sync::Lazy;
use structopt::StructOpt;

mod annotate;
mod filter;
mod games;
mod stats;

static ABORT: AtomicBool = AtomicBool::new(false);

#[derive(StructOpt)]
struct Args {
    #[structopt(flatten)]
    common: CommonOptions,

    #[structopt(subcommand)]
    subcommand: Subcommand,
}

#[derive(StructOpt)]
struct CommonOptions {
    #[structopt(short = "p", long, default_value = &DEFAULT_CONCURRENCY_STR)]
    concurrency: usize,

    #[structopt(short = "s", long)]
    syzygy: Option<PathBuf>,
}

#[derive(StructOpt)]
enum Subcommand {
    /// Generate positions from self-play games
    Games(games::Options),
    Annotate(annotate::Options),
    Stats(stats::Options),
    Filter(filter::Options),
}

fn main() {
    let options = Args::from_args();

    ctrlc::set_handler(|| {
        ABORT.store(true, Ordering::SeqCst);
    })
    .unwrap();

    let r = match options.subcommand {
        Subcommand::Games(opt) => opt.run(options.common),
        Subcommand::Annotate(opt) => opt.run(options.common),
        Subcommand::Stats(opt) => opt.run(options.common),
        Subcommand::Filter(opt) => opt.run(options.common),
    };

    if let Err(e) = r {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

static DEFAULT_CONCURRENCY_STR: Lazy<String> = Lazy::new(|| {
    std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(1)
        .to_string()
});

fn parse_filter_underscore<T: FromStr>(s: &str) -> Result<T, T::Err> {
    s.replace('_', "").parse()
}

impl CommonOptions {
    fn parallel<T>(
        &self,
        init: impl Fn() -> T + Sync,
        f: impl Fn(&mut T) -> ControlFlow<()> + Sync,
    ) {
        std::thread::scope(|s| {
            for _ in 0..self.concurrency {
                s.spawn(|| {
                    let mut tl = init();
                    while !ABORT.load(Ordering::Relaxed) {
                        if f(&mut tl).is_break() {
                            break;
                        }
                    }
                });
            }
        });
    }

    fn syzygy(&self) -> Tablebase {
        let mut tb = Tablebase::new();
        if let Some(path) = &self.syzygy {
            tb.add_directory(path).unwrap();
        }
        if tb.max_pieces() > 2 {
            println!("Using TB with up to {} pieces", tb.max_pieces());
        }
        tb
    }
}

fn eta(time: f64, completion: f64) -> String {
    let eta = time / completion - time;
    let mins = eta as i64 / 60;
    let hours = mins / 60;
    let days = hours / 24;
    let hours = hours % 24;
    let mins = mins % 60;
    format!("{days} days {hours:02}:{mins:02}")
}
