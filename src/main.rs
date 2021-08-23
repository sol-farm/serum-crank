#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use anyhow::{anyhow, Result};
use clap::{App, Arg, SubCommand};
use log::{error, info, warn};
pub mod config;
pub mod crank;

#[tokio::main]
async fn main() {
    let matches = clap::App::new("serum-crank")
        .version("0.0.1")
        .author("Solfarm")
        .about("a performance optimized serum crank service")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("sets the config file")
                .takes_value(true),
        )
        .subcommand(
            SubCommand::with_name("config")
                .about("configuration management commands")
                .subcommands(vec![
                    SubCommand::with_name("new").about("generates a new configuration file")
                ]),
        )
        .get_matches();
    let config_file_path = get_config_or_default(&matches);
    let res = process_matches(&matches, config_file_path).await;
    if res.is_err() {
        error!("failed to process command matches {:#?}", res.err());
    }
}
async fn process_matches<'a>(
    matches: &clap::ArgMatches<'a>,
    config_file_path: String,
) -> Result<()> {
    match matches.subcommand() {
        ("config", Some(config)) => match config.subcommand() {
            ("new", Some(new_config)) => {
                config::Configuration::new(config_file_path.as_str(), false)?;
            }
            _ => return Err(anyhow!("failed to match subcommand")),
        },
        _ => return Err(anyhow!("failed to match subcommand")),
    }
    Ok(())
}
// returns the value of the config file argument or the default
fn get_config_or_default(matches: &clap::ArgMatches) -> String {
    matches
        .value_of("config")
        .unwrap_or("config.yaml")
        .to_string()
}
