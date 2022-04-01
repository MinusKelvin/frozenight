use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::sync::Lazy;
use structopt::StructOpt;

mod annotate;
mod games;

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
    syzygy_path: Vec<PathBuf>,
}

#[derive(StructOpt)]
enum Subcommand {
    /// Generate positions from self-play games
    Games(games::Options),
    Annotate(annotate::Options),
}

fn main() {
    let options = Args::from_args();

    ctrlc::set_handler(|| {
        ABORT.store(true, Ordering::SeqCst);
    }).unwrap();

    match options.subcommand {
        Subcommand::Games(opt) => opt.run(options.common),
        Subcommand::Annotate(opt) => opt.run(options.common),
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
