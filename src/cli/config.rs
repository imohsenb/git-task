use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};

use crate::automation::rules::{self, Rule};
use crate::cli::wizard;
use crate::config::config_op::ConfigOp;
use crate::config::fields;
use crate::config::global::GlobalConfig;
use crate::config::project::{self, ProjectConfig};
use crate::git;

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

const KNOWN_FIELDS: &[&str] = &["priority", "assignee", "due"];

/// Unified entrypoint for all per-repo configuration. Every edit goes through here (and appends to
/// the event-sourced `refs/tasks/config` op-chain) — there is no config file to hand-edit.
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show the effective config for this repo (key, required fields, automation rules)
    Show,
    /// Show or set this repo's short address key (e.g. SRV, used as SRV-9057e58a)
    Key(KeyArgs),
    /// Mark a field required or optional on new tasks
    Field(FieldArgs),
    /// Manage automation rules (add / list / remove)
    Rule(RuleArgs),
}

#[derive(Args)]
pub struct KeyArgs {
    /// New key to set (e.g. SRV). Omit to print the current effective key.
    pub new_key: Option<String>,
}

#[derive(Args)]
pub struct FieldArgs {
    /// Field name: priority, assignee, or due
    name: String,
    /// Whether the field must be filled in on new tasks
    #[arg(value_enum)]
    requirement: Requirement,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Requirement {
    Required,
    Optional,
}

#[derive(Args)]
pub struct RuleArgs {
    #[command(subcommand)]
    action: RuleAction,
}

#[derive(Subcommand)]
enum RuleAction {
    /// Add a rule — interactive wizard, or fully specified via flags
    Add(RuleAddArgs),
    /// List the effective automation rules (global + this repo's)
    List,
    /// Remove a rule by name
    Remove(RuleRemoveArgs),
}

#[derive(Args)]
pub struct RuleAddArgs {
    /// Rule name. Providing this (with --on/--do) runs non-interactively.
    #[arg(long)]
    name: Option<String>,
    /// Event to fire on (task.created, task.updated, status.changed, comment.added, label.added)
    #[arg(long)]
    on: Option<String>,
    /// Optional evalexpr condition, e.g. --when 'kind == "bug"'
    #[arg(long)]
    when: Option<String>,
    /// Action(s), repeatable: --do 'set_priority high' --do 'add_label triage'
    #[arg(long = "do")]
    actions: Vec<String>,
    /// Save to the global personal rules file instead of this repo's config
    #[arg(long)]
    global: bool,
}

#[derive(Args)]
pub struct RuleRemoveArgs {
    /// Name of the rule to remove
    name: String,
    /// Remove from the global personal rules file instead of this repo's config
    #[arg(long)]
    global: bool,
}

pub fn run(args: ConfigArgs) -> Result<()> {
    match args.action {
        ConfigAction::Show => show(),
        ConfigAction::Key(a) => run_key(a),
        ConfigAction::Field(a) => run_field(a),
        ConfigAction::Rule(a) => match a.action {
            RuleAction::Add(a) => run_rule_add(a),
            RuleAction::List => run_rule_list(),
            RuleAction::Remove(a) => run_rule_remove(a),
        },
    }
}

pub(crate) fn show() -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let global = GlobalConfig::load()?;
    let project = ProjectConfig::load(&repo)?;

    println!("key: {}", project.effective_key(&workdir));
    println!();

    let required = fields::resolve(&global.fields, &project.fields);
    println!("fields:");
    println!("  title       required (fixed)");
    println!("  description required (fixed)");
    println!("  priority    {}", state(required.priority));
    println!("  assignee    {}", state(required.assignee));
    println!("  due         {}", state(required.due));
    println!();

    let global_rules = rules::load_global()?;
    println!("rules:");
    if global_rules.is_empty() && project.rules.is_empty() {
        println!("  (none)");
    } else {
        for r in &global_rules {
            print_rule("global", r);
        }
        for r in &project.rules {
            print_rule("repo", r);
        }
    }
    println!();
    println!("edit: git task config key <K> | config field <name> required|optional | config rule add");
    Ok(())
}

/// The `fields` alias's read-only view — just the required-field schema, reading from the config ref.
pub(crate) fn show_fields() -> Result<()> {
    let repo = git::repo::discover_current()?;
    let global = GlobalConfig::load()?;
    let project = ProjectConfig::load(&repo)?;
    let required = fields::resolve(&global.fields, &project.fields);

    println!("title       required (fixed)");
    println!("description required (fixed)");
    println!("priority    {}", state(required.priority));
    println!("assignee    {}", state(required.assignee));
    println!("due         {}", state(required.due));
    println!();
    println!("Set per-repo with: git task config field <priority|assignee|due> required|optional");
    println!("Global defaults: ~/.config/git-task/config.toml ([fields.<name>] required = true).");
    Ok(())
}

pub(crate) fn run_key(args: KeyArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let cfg = ProjectConfig::load(&repo)?;

    match args.new_key {
        None => {
            let effective = cfg.effective_key(&workdir);
            match &cfg.key {
                Some(_) => println!("{effective} (set via git task config)"),
                None => println!(
                    "{effective} (derived from repo name — run 'git task config key {effective}' to pin it)"
                ),
            }
        }
        Some(raw) => {
            let key = validate_key(&raw)?;
            project::append_ops(&repo, vec![ConfigOp::SetKey { key: key.clone() }])?;
            println!("key set to {key}");
        }
    }
    Ok(())
}

fn run_field(args: FieldArgs) -> Result<()> {
    let name = args.name.to_ascii_lowercase();
    if !KNOWN_FIELDS.contains(&name.as_str()) {
        bail!("unknown field '{}' (known: {})", args.name, KNOWN_FIELDS.join(", "));
    }
    let required = matches!(args.requirement, Requirement::Required);
    let repo = git::repo::discover_current()?;
    project::append_ops(&repo, vec![ConfigOp::SetFieldRequired { field: name.clone(), required }])?;
    println!("'{name}' is now {} on new tasks", if required { "required" } else { "optional" });
    Ok(())
}

fn run_rule_add(args: RuleAddArgs) -> Result<()> {
    // Any of the defining flags switches to non-interactive mode.
    let non_interactive = args.name.is_some() || args.on.is_some() || !args.actions.is_empty();
    if !non_interactive {
        return add_interactive();
    }

    let name = args.name.context("--name is required when adding a rule non-interactively")?;
    let on = args.on.context("--on is required when adding a rule non-interactively")?;
    if !EVENTS.contains(&on.as_str()) {
        bail!("unknown event '{on}' (known: {})", EVENTS.join(", "));
    }
    if args.actions.is_empty() {
        bail!("at least one --do action is required");
    }
    if let Some(expr) = &args.when {
        evalexpr::build_operator_tree(expr)
            .with_context(|| format!("invalid --when expression '{expr}'"))?;
    }
    for action in &args.actions {
        validate_action_verb(action)?;
    }

    let rule = Rule { name, on, when: args.when, actions: args.actions };
    save_rule(rule, args.global)
}

/// Interactive rule wizard. Also invoked from `git task init` and the `automation add` alias.
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
    print_rule_line(&rule);

    let scope = wizard::prompt_choice(
        "save where?",
        &["this repo (git task config)", "global (~/.config/git-task/automation.toml)"],
        0,
    )?;
    save_rule(rule, scope == 1)
}

pub(crate) fn run_rule_list() -> Result<()> {
    let global = rules::load_global()?;
    let project = match git::repo::discover_current() {
        Ok(repo) => ProjectConfig::load(&repo)?.rules,
        Err(_) => Vec::new(),
    };

    if global.is_empty() && project.is_empty() {
        println!("no automation rules configured.");
        println!("global:   ~/.config/git-task/automation.toml");
        println!("per-repo: git task config rule add");
        return Ok(());
    }

    if !global.is_empty() {
        println!("global (~/.config/git-task/automation.toml):");
        for r in &global {
            print_rule_line(r);
        }
    }
    if !project.is_empty() {
        println!("this repo (git task config):");
        for r in &project {
            print_rule_line(r);
        }
    }
    Ok(())
}

fn run_rule_remove(args: RuleRemoveArgs) -> Result<()> {
    if args.global {
        let mut all = rules::load_global()?;
        let before = all.len();
        all.retain(|r| r.name != args.name);
        if all.len() == before {
            bail!("no global rule named '{}'", args.name);
        }
        rules::save_global(&all)?;
        println!("removed global rule '{}'", args.name);
    } else {
        let repo = git::repo::discover_current()?;
        let cfg = ProjectConfig::load(&repo)?;
        if !cfg.rules.iter().any(|r| r.name == args.name) {
            bail!("no rule named '{}' in this repo's config", args.name);
        }
        project::append_ops(&repo, vec![ConfigOp::RemoveRule { name: args.name.clone() }])?;
        println!("removed rule '{}' from this repo's config", args.name);
    }
    Ok(())
}

fn save_rule(rule: Rule, global: bool) -> Result<()> {
    if global {
        let mut all = rules::load_global()?;
        match all.iter_mut().find(|r| r.name == rule.name) {
            Some(existing) => *existing = rule,
            None => all.push(rule),
        }
        rules::save_global(&all)?;
        println!("saved to ~/.config/git-task/automation.toml");
    } else {
        let repo = git::repo::discover_current()?;
        let name = rule.name.clone();
        project::append_ops(&repo, vec![ConfigOp::UpsertRule { rule }])?;
        println!("saved rule '{name}' to this repo's config");
    }
    Ok(())
}

fn validate_key(raw: &str) -> Result<String> {
    let key = raw.to_ascii_uppercase();
    let valid = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid {
        bail!("key must start with a letter and contain only letters/digits, got '{raw}'");
    }
    Ok(key)
}

fn validate_action_verb(action: &str) -> Result<()> {
    let verb = action.trim().split_whitespace().next().unwrap_or("");
    if !ACTION_VERBS.contains(&verb) {
        bail!("unknown action verb '{verb}' (known: {})", ACTION_VERBS.join(", "));
    }
    Ok(())
}

fn state(required: bool) -> &'static str {
    if required {
        "required"
    } else {
        "optional"
    }
}

fn print_rule_line(r: &Rule) {
    let when = r.when.as_deref().unwrap_or("(always)");
    println!("  - {} | on={} | when={} | do={}", r.name, r.on, when, r.actions.join("; "));
}

fn print_rule(scope: &str, r: &Rule) {
    let when = r.when.as_deref().unwrap_or("(always)");
    println!("  [{scope}] {} | on={} | when={} | do={}", r.name, r.on, when, r.actions.join("; "));
}
