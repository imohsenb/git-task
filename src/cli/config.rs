use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};

use crate::automation::rules::{self, Rule};
use crate::cli::wizard;
use crate::color;
use crate::config::config_op::ConfigOp;
use crate::config::fields;
use crate::config::global::GlobalConfig;
use crate::config::project::{self, ProjectConfig};
use crate::git;
use crate::hints;
use crate::logger::Logger;
use crate::table::{self, Seg};
use crate::wrap;

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

/// Boxed detail card, built from the same `table::field_row`/`text_row` primitives `show <id>`
/// uses for a task — so `config show` and `show` read as one visual system instead of two, and
/// any future "show one thing in a box" command has the pieces ready in `table.rs` rather than
/// reinventing box math again.
pub(crate) fn show() -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let global = GlobalConfig::load()?;
    let project = ProjectConfig::load(&repo)?;
    let required = fields::resolve(&global.fields, &project.fields);
    let global_rules = rules::load_global()?;

    let width = wrap::terminal_width();
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };

    line(table::boxed_titled_border("╭", "╮", Some("CONFIG"), width));
    line(table::boxed_blank(width));

    line(table::field_row("Key", table::plain_seg(&project.effective_key(&workdir)), width));
    line(table::boxed_blank(width));

    line(table::field_row2(
        "Priority",
        requirement_seg(required.priority),
        "Assignee",
        requirement_seg(required.assignee),
        width,
    ));
    line(table::field_row("Due", requirement_seg(required.due), width));
    line(table::boxed_blank(width));

    let rule_count = global_rules.len() + project.rules.len();
    line(table::boxed_titled_border("├", "┤", Some(&format!("RULES ({rule_count})")), width));
    line(table::boxed_blank(width));
    if rule_count == 0 {
        line(table::text_row("(none)", width));
    } else {
        let rows: Vec<(&'static str, &Rule)> = global_rules
            .iter()
            .map(|r| ("global", r))
            .chain(project.rules.iter().map(|r| ("repo", r)))
            .collect();
        for (i, (scope, r)) in rows.iter().enumerate() {
            for row in rule_lines(scope, r, width) {
                line(row);
            }
            if i + 1 < rows.len() {
                line(table::boxed_blank(width));
            }
        }
    }
    line(table::boxed_blank(width));
    line(table::boxed_titled_border("╰", "╯", None, width));

    println!();
    print!("{out}");
    hints::print(&[
        ("config key <K>".to_string(), "pin the address key".to_string()),
        ("config field <name> required|optional".to_string(), "require/optional a field".to_string()),
        ("config rule add".to_string(), "add an automation rule".to_string()),
    ]);
    Ok(())
}

fn requirement_seg(required: bool) -> Seg {
    if required {
        Seg { colored: color::bold("required"), plain: "required".to_string() }
    } else {
        Seg { colored: color::dim("optional"), plain: "optional".to_string() }
    }
}

/// One rule as a bold name/scope-tag row plus a wrapped detail line beneath it — the same
/// name-row/wrapped-body shape `render::to_text` uses for comments.
fn rule_lines(scope: &str, r: &Rule, width: usize) -> Vec<String> {
    let mut lines = vec![table::boxed_row(
        &[
            table::spaces_seg(table::BOX_INDENT),
            table::dim_seg(&format!("[{scope}] ")),
            table::bold_seg(&r.name),
        ],
        width,
    )];
    let when = r.when.as_deref().unwrap_or("(always)");
    let detail = format!("on={} | when={} | do={}", r.on, when, r.actions.join("; "));
    let detail_width = table::wrap_width_for(table::BOX_INDENT + 2, width);
    for wrapped in wrap::wrap(&detail, detail_width) {
        lines.push(table::boxed_row(
            &[table::spaces_seg(table::BOX_INDENT + 2), table::dim_seg(&wrapped)],
            width,
        ));
    }
    lines
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
            Logger::info(
                &format!("Key set to {key}"),
                None,
                &[("config show".to_string(), "view the effective config".to_string())],
            );
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
    let action = if required { "required" } else { "optional" };
    Logger::info(
        &format!("'{name}' is now {action} on new tasks"),
        None,
        &[("config show".to_string(), "view the effective config".to_string())],
    );
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
    let tips: &[(String, String)] = &[("config show".to_string(), "view the effective config".to_string())];
    if args.global {
        let mut all = rules::load_global()?;
        let before = all.len();
        all.retain(|r| r.name != args.name);
        if all.len() == before {
            bail!("no global rule named '{}'", args.name);
        }
        rules::save_global(&all)?;
        Logger::info(&format!("Removed global rule '{}'", args.name), None, tips);
    } else {
        let repo = git::repo::discover_current()?;
        let cfg = ProjectConfig::load(&repo)?;
        if !cfg.rules.iter().any(|r| r.name == args.name) {
            bail!("no rule named '{}' in this repo's config", args.name);
        }
        project::append_ops(&repo, vec![ConfigOp::RemoveRule { name: args.name.clone() }])?;
        Logger::info(&format!("Removed rule '{}' from this repo's config", args.name), None, tips);
    }
    Ok(())
}

fn save_rule(rule: Rule, global: bool) -> Result<()> {
    let tips: &[(String, String)] = &[("config show".to_string(), "view the effective config".to_string())];
    if global {
        let mut all = rules::load_global()?;
        match all.iter_mut().find(|r| r.name == rule.name) {
            Some(existing) => *existing = rule,
            None => all.push(rule),
        }
        rules::save_global(&all)?;
        Logger::info("Saved rule to ~/.config/git-task/automation.toml", None, tips);
    } else {
        let repo = git::repo::discover_current()?;
        let name = rule.name.clone();
        project::append_ops(&repo, vec![ConfigOp::UpsertRule { rule }])?;
        Logger::info(&format!("Saved rule '{name}' to this repo's config"), None, tips);
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
