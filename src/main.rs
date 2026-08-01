mod actor;
mod cli;
mod config;
mod git;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(err) = cli.run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
