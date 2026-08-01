pub mod actor;
pub mod automation;
pub mod banner;
pub mod cli;
pub mod color;
pub mod config;
pub mod domain;
pub mod git;
pub mod hints;
pub mod prompt;
pub mod render;
pub mod store;
pub mod table;

use clap::{CommandFactory, FromArgMatches};

/// Parses argv and dispatches, using `bin_name` for the help/usage/version
/// banner — lets `git-task` (invoked as `git task`) and `ght` (invoked
/// directly) share one Cli definition while each shows its own name.
pub fn run(bin_name: &'static str) {
    let mut command = cli::Cli::command().name(bin_name).bin_name(bin_name);
    // `build()` finalizes subcommand metadata (names/about text) so `cli::help::render`
    // can read it back below; required before `override_help` per clap's own docs.
    command.build();
    let help_text = cli::help::render(&command, bin_name);
    let command = command.override_help(help_text);

    let matches = command.get_matches();
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    if let Err(err) = cli.run(bin_name) {
        eprintln!("{} {err:#}", color::bold_red("error:"));
        std::process::exit(1);
    }
}
