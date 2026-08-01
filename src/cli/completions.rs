use anyhow::Result;
use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};

use crate::cli::Cli;

#[derive(Args)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    shell: Shell,
}

pub fn run(args: CompletionsArgs, bin_name: &str) -> Result<()> {
    // Shell completion functions need a valid single-word identifier — "git task"
    // (the two-word dispatch form) isn't one and crashes the bash generator, so use
    // the real on-PATH executable name ("git-task") instead of the display bin_name.
    let exe_name = bin_name.replace(' ', "-");
    let mut cmd = Cli::command();
    generate(args.shell, &mut cmd, exe_name, &mut std::io::stdout());
    Ok(())
}
