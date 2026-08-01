use anyhow::Result;
use clap::{Args, Subcommand};

use crate::automation::rules::{self, Rule};
use crate::config::project::ProjectConfig;
use crate::git;

#[derive(Args)]
pub struct AutomationArgs {
    #[command(subcommand)]
    action: AutomationAction,
}

#[derive(Subcommand)]
enum AutomationAction {
    /// List the effective automation rules (global + this repo's)
    List,
}

pub fn run(args: AutomationArgs) -> Result<()> {
    match args.action {
        AutomationAction::List => list(),
    }
}

fn list() -> Result<()> {
    let global = rules::load_global()?;
    let project = match git::repo::discover_current() {
        Ok(repo) => {
            let workdir = git::repo::workdir(&repo)?;
            ProjectConfig::load(&workdir)?.rules
        }
        Err(_) => Vec::new(),
    };

    if global.is_empty() && project.is_empty() {
        println!("no automation rules configured.");
        println!("global:   ~/.config/git-task/automation.toml");
        println!("per-repo: .gittask/config.toml ([[rule]] entries)");
        return Ok(());
    }

    if !global.is_empty() {
        println!("global (~/.config/git-task/automation.toml):");
        for r in &global {
            print_rule(r);
        }
    }
    if !project.is_empty() {
        println!("this repo (.gittask/config.toml):");
        for r in &project {
            print_rule(r);
        }
    }
    Ok(())
}

fn print_rule(r: &Rule) {
    let when = r.when.as_deref().unwrap_or("(always)");
    println!("  - {} | on={} | when={} | do={}", r.name, r.on, when, r.actions.join("; "));
}
