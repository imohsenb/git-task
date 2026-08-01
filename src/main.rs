mod actor;
mod cli;
mod config;
mod domain;
mod git;
mod render;
mod store;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(err) = cli.run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
