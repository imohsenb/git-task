use anyhow::Result;
use clap::{Args, Subcommand};

use crate::cli::config;
use crate::output::{self, ClassifiedError};

// Thin alias over `git task config rule` (custom rules) plus the built-in automation toggle
// (`git task config`'s `run_automation_toggle`). Custom rules live in the event-sourced config
// ref (`refs/tasks/config`) for this repo, or in the global personal file for `--global`.
#[derive(Args)]
pub struct AutomationArgs {
    #[command(subcommand)]
    action: AutomationAction,
}

#[derive(Subcommand)]
enum AutomationAction {
    /// List built-in automations and the effective custom rules (global + this repo's)
    List,
    /// Interactive wizard to build and save a new custom automation rule
    Add,
    /// Enable a built-in automation (auto-unassign-done, auto-sync)
    Enable(ToggleArgs),
    /// Disable a built-in automation (auto-unassign-done, auto-sync)
    Disable(ToggleArgs),
}

#[derive(Args)]
pub struct ToggleArgs {
    /// Built-in automation name (auto-unassign-done, auto-sync)
    name: String,
    /// Apply per-machine (~/.config/git-task/config.toml) instead of this repo's config
    #[arg(long)]
    global: bool,
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
        AutomationAction::Enable(a) => config::run_automation_toggle(a.name, a.global, true),
        AutomationAction::Disable(a) => config::run_automation_toggle(a.name, a.global, false),
    }
}
