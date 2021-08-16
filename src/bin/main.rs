#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use anyhow::Result;
use clap::Clap;
use crank::Opts;

fn main() -> Result<()> {
    let opts = Opts::parse();
    crank::start(opts)
}
