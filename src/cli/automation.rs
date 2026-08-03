use anyhow::Result;
use clap::{Args, Subcommand};

use crate::cli::config;
use crate::output::{self, ClassifiedError};

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
        AutomationAction::Add => {
            // Unlike `config rule add`, this alias has no flag-based non-interactive form —
            // it's always the wizard. A JSON caller can't answer prompts, so point it at the
            // command that does have one instead of blocking on stdin.
            if output::is_json() {
                return Err(anyhow::Error::new(ClassifiedError::Validation {
                    message: "automation add is interactive-only; use 'config rule add --name --on --do' under --format json".to_string(),
                    field: None,
                    missing: Vec::new(),
                }));
            }
            config::add_interactive()
        }
    }
}
