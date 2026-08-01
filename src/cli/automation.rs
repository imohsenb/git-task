use anyhow::Result;
use clap::{Args, Subcommand};

use crate::automation::rules::{self, Rule};
use crate::cli::wizard;
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
    /// Interactive wizard to build and save a new automation rule
    Add,
}

const EVENTS: &[&str] =
    &["task.created", "task.updated", "status.changed", "comment.added", "label.added"];

const ACTION_VERBS: &[&str] = &[
    "set_priority",
    "set_status",
    "set_assignee",
    "set_kind",
    "add_label",
    "remove_label",
    "set_due",
    "set_milestone",
    "add_comment",
];

pub fn run(args: AutomationArgs) -> Result<()> {
    match args.action {
        AutomationAction::List => list(),
        AutomationAction::Add => add_interactive(),
    }
}

/// Also called from `git task init` when the user opts to add a rule right after project setup.
pub(crate) fn add_interactive() -> Result<()> {
    println!("new automation rule (Ctrl+C to abort)");

    let name = loop {
        let raw = wizard::prompt("rule name")?;
        if raw.is_empty() {
            println!("name can't be empty");
            continue;
        }
        break raw;
    };

    let on = EVENTS[wizard::prompt_choice("fire on which event?", EVENTS, 0)?].to_string();

    let when = loop {
        let raw = wizard::prompt("condition (evalexpr, e.g. kind == \"bug\"; blank = always)")?;
        if raw.is_empty() {
            break None;
        }
        match evalexpr::build_operator_tree(&raw) {
            Ok(_) => break Some(raw),
            Err(err) => println!("invalid expression: {err}"),
        }
    };

    println!("add actions (blank verb to finish). available: {}", ACTION_VERBS.join(", "));
    let mut actions = Vec::new();
    loop {
        let verb = wizard::prompt(&format!("  action {} verb", actions.len() + 1))?;
        if verb.is_empty() {
            if actions.is_empty() {
                println!("at least one action is required");
                continue;
            }
            break;
        }
        if !ACTION_VERBS.contains(&verb.as_str()) {
            println!("unknown verb '{verb}', pick one of: {}", ACTION_VERBS.join(", "));
            continue;
        }
        let value = wizard::prompt("  value")?;
        actions.push(if value.contains(' ') {
            format!("{verb} \"{value}\"")
        } else {
            format!("{verb} {value}")
        });
    }

    let rule = Rule { name, on, when, actions };
    println!("about to save:");
    print_rule(&rule);

    let scope = wizard::prompt_choice(
        "save where?",
        &["this repo (.gittask/config.toml)", "global (~/.config/git-task/automation.toml)"],
        0,
    )?;

    if scope == 0 {
        let repo = git::repo::discover_current()?;
        let workdir = git::repo::workdir(&repo)?;
        let mut cfg = ProjectConfig::load(&workdir)?;
        cfg.rules.push(rule);
        cfg.save(&workdir)?;
        println!("saved to .gittask/config.toml");
    } else {
        let mut global_rules = rules::load_global()?;
        global_rules.push(rule);
        rules::save_global(&global_rules)?;
        println!("saved to ~/.config/git-task/automation.toml");
    }

    Ok(())
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
