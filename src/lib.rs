pub mod actor;
pub mod automation;
pub mod banner;
pub mod cli;
pub mod config;
pub mod domain;
pub mod git;
pub mod prompt;
pub mod render;
pub mod store;

use clap::{CommandFactory, FromArgMatches};

/// Parses argv and dispatches, using `bin_name` for the help/usage/version
/// banner — lets `git-task` (invoked as `git task`) and `ght` (invoked
/// directly) share one Cli definition while each shows its own name.
pub fn run(bin_name: &'static str) {
    let command = cli::Cli::command().name(bin_name).bin_name(bin_name);
    let matches = command.get_matches();
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    if let Err(err) = cli.run(bin_name) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
