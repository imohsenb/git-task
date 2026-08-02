use anyhow::Result;
use clap::{Args, Subcommand};

use crate::cli::config;

// Thin alias over `git task config rule`. Rules live in the event-sourced config ref
// (`refs/tasks/config`) for this repo, or in the global personal file for `--global`.
#[derive(Args)]
pub struct AutomationArgs {
    #[command(subcommand)]
    action: AutomationAction,
}

#[derive(Subcommand)]
enum AutomationAction {
    /// List the effective automation rules (global + this repo's)
    List,
    /// Interactive wizard to build and save a new automation rule
    Add,
}

pub fn run(args: AutomationArgs) -> Result<()> {
    match args.action {
        AutomationAction::List => config::run_rule_list(),
        AutomationAction::Add => config::add_interactive(),
    }
}
