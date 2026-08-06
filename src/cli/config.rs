use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use git2::Repository;
use serde::Serialize;

use crate::automation::builtins;
use crate::automation::rules::{self, Rule};
use crate::cli::wizard;
use crate::color;
use crate::config::automation_toggle::{self, AutomationOverrides};
use crate::config::config_op::ConfigOp;
use crate::config::fields::{self, FieldMap};
use crate::config::global::GlobalConfig;
use crate::config::project::{self, ProjectConfig};
use crate::git;
use crate::hints;
use crate::logger::Logger;
use crate::output::{self, ClassifiedError};
use crate::table::{self, Seg};
use crate::wrap;

const EVENTS: &[&str] =
    &["task.created", "task.updated", "status.changed", "comment.added", "label.added"];

const ACTION_VERBS: &[&str] = &[
    "set_priority",
    "set_status",
    "set_assignee",
    "clear_assignee",
    "set_kind",
    "add_label",
    "remove_label",
    "set_due",
    "set_milestone",
    "add_comment",
];

const KNOWN_FIELDS: &[&str] = &["priority", "assignee", "due"];

#[derive(Serialize)]
struct FieldStatusJson {
    required: bool,
    source: &'static str,
}

#[derive(Serialize)]
struct RuleJson {
    scope: &'static str,
    name: String,
    on: String,
    when: Option<String>,
    actions: Vec<String>,
}

fn rule_json(scope: &'static str, r: &Rule) -> RuleJson {
    RuleJson { scope, name: r.name.clone(), on: r.on.clone(), when: r.when.clone(), actions: r.actions.clone() }
}

#[derive(Serialize)]
struct BuiltinJson {
    name: &'static str,
    enabled: bool,
    source: &'static str,
}

#[derive(Serialize)]
struct ConfigJson {
    key: String,
    key_source: &'static str,
    fields: BTreeMap<String, FieldStatusJson>,
    builtins: Vec<BuiltinJson>,
    rules: Vec<RuleJson>,
}

fn field_status(name: &str, global: &FieldMap, project: &FieldMap) -> FieldStatusJson {
    if let Some(spec) = project.get(name) {
        return FieldStatusJson { required: spec.required, source: "repo" };
    }
    if let Some(spec) = global.get(name) {
        return FieldStatusJson { required: spec.required, source: "global" };
    }
    FieldStatusJson { required: false, source: "default" }
}

/// Builds the `--format json` config shape shared by `show`, `key`, `field`, and `rule`.
/// `repo` is `None` only for a global-only rule mutation (`rule add/remove --global`) run outside
/// any git repo — every other config command already requires a repo — in which case the
/// repo-specific parts (`key`, per-repo fields/rules) come back empty/default rather than erroring.
fn build_config_json(repo: Option<&Repository>) -> Result<ConfigJson> {
    let global_cfg = GlobalConfig::load()?;
    let global_rules = rules::load_global()?;

    let (key, key_source, project_fields, project_rules, project_automation) = match repo {
        Some(repo) => {
            let workdir = git::repo::workdir(repo)?;
            let project_cfg = ProjectConfig::load(repo)?;
            let key = project_cfg.effective_key(&workdir);
            let key_source = if project_cfg.key.is_some() { "config" } else { "derived" };
            (key, key_source, project_cfg.fields, project_cfg.rules, project_cfg.automation)
        }
        None => (String::new(), "derived", FieldMap::new(), Vec::new(), AutomationOverrides::new()),
    };

    let mut fields = BTreeMap::new();
    for name in KNOWN_FIELDS {
        fields.insert(name.to_string(), field_status(name, &global_cfg.fields, &project_fields));
    }

    let builtins_json: Vec<BuiltinJson> = builtins::NAMES
        .iter()
        .map(|&name| BuiltinJson {
            name,
            enabled: automation_toggle::resolve_enabled(name, &global_cfg.automation, &project_automation),
            source: automation_toggle::source(name, &global_cfg.automation, &project_automation),
        })
        .collect();

    let mut rules_json: Vec<RuleJson> = global_rules.iter().map(|r| rule_json("global", r)).collect();
    rules_json.extend(project_rules.iter().map(|r| rule_json("repo", r)));

    Ok(ConfigJson { key, key_source, fields, builtins: builtins_json, rules: rules_json })
}

#[derive(Serialize)]
struct ConfigMutationJson {
    config: ConfigJson,
}

fn print_config_mutation(repo: Option<&Repository>) -> Result<()> {
    output::print_ok(ConfigMutationJson { config: build_config_json(repo)? });
    Ok(())
}

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

    if output::is_json() {
        output::print_ok(build_config_json(Some(&repo))?);
        return Ok(());
    }

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

    automation_sections(
        &mut line,
        width,
        "├",
        "┤",
        Some("BUILTIN AUTOMATIONS"),
        &global,
        &project.automation,
        &global_rules,
        &project.rules,
    );

    line(table::boxed_blank(width));
    line(table::boxed_titled_border("╰", "╯", None, width));

    println!();
    print!("{out}");
    hints::print(&[
        ("config key <K>".to_string(), "pin the address key".to_string()),
        ("config field <name> required|optional".to_string(), "require/optional a field".to_string()),
        ("config rule add".to_string(), "add an automation rule".to_string()),
        ("automation disable <name>".to_string(), "turn off a built-in automation".to_string()),
    ]);
    Ok(())
}

/// Renders the BUILTIN AUTOMATIONS + RULES box sections shared by `config show` (where they're
/// a middle section of the CONFIG card) and `config rule list`/`automation list` (where they're
/// the whole box) — same content, different callers just supply the top border/title.
#[allow(clippy::too_many_arguments)]
fn automation_sections(
    line: &mut dyn FnMut(String),
    width: usize,
    top_left: &str,
    top_right: &str,
    top_title: Option<&str>,
    global: &GlobalConfig,
    project_automation: &AutomationOverrides,
    global_rules: &[Rule],
    project_rules: &[Rule],
) {
    line(table::boxed_titled_border(top_left, top_right, top_title, width));
    line(table::boxed_blank(width));
    for &name in builtins::NAMES {
        let enabled = automation_toggle::resolve_enabled(name, &global.automation, project_automation);
        let source = automation_toggle::source(name, &global.automation, project_automation);
        line(table::field_row(name, builtin_state_seg(enabled, source), width));
    }
    line(table::boxed_blank(width));

    let rule_count = global_rules.len() + project_rules.len();
    line(table::boxed_titled_border("├", "┤", Some(&format!("RULES ({rule_count})")), width));
    line(table::boxed_blank(width));
    if rule_count == 0 {
        line(table::text_row("(none)", width));
    } else {
        let rows: Vec<(&'static str, &Rule)> = global_rules
            .iter()
            .map(|r| ("global", r))
            .chain(project_rules.iter().map(|r| ("repo", r)))
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
}

fn requirement_seg(required: bool) -> Seg {
    if required {
        Seg { colored: color::bold("required"), plain: "required".to_string() }
    } else {
        Seg { colored: color::dim("optional"), plain: "optional".to_string() }
    }
}

fn builtin_state_seg(enabled: bool, source: &str) -> Seg {
    let label = if enabled { "enabled" } else { "disabled" };
    let text = if source == "default" { label.to_string() } else { format!("{label} ({source})") };
    if enabled {
        Seg { colored: color::bold(&text), plain: text }
    } else {
        Seg { colored: color::dim(&text), plain: text }
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

    if output::is_json() {
        output::print_ok(build_config_json(Some(&repo))?);
        return Ok(());
    }

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
            if output::is_json() {
                return print_config_mutation(Some(&repo));
            }
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
            if output::is_json() {
                return print_config_mutation(Some(&repo));
            }
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

    if output::is_json() {
        return print_config_mutation(Some(&repo));
    }

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
        if output::is_json() {
            return Err(anyhow::Error::new(ClassifiedError::Validation {
                message: "config rule add needs --name/--on/--do under --format json (no interactive wizard)"
                    .to_string(),
                field: None,
                missing: vec!["name".to_string(), "on".to_string(), "do".to_string()],
            }));
        }
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
    if output::is_json() {
        let repo = git::repo::discover_current().ok();
        output::print_ok(build_config_json(repo.as_ref())?);
        return Ok(());
    }

    let global_cfg = GlobalConfig::load()?;
    let global_rules = rules::load_global()?;
    let (project_rules, project_automation) = match git::repo::discover_current() {
        Ok(repo) => {
            let cfg = ProjectConfig::load(&repo)?;
            (cfg.rules, cfg.automation)
        }
        Err(_) => (Vec::new(), AutomationOverrides::new()),
    };

    let width = wrap::terminal_width();
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };

    automation_sections(
        &mut line,
        width,
        "╭",
        "╮",
        Some("BUILTIN AUTOMATIONS"),
        &global_cfg,
        &project_automation,
        &global_rules,
        &project_rules,
    );

    line(table::boxed_blank(width));
    line(table::boxed_titled_border("╰", "╯", None, width));

    println!();
    print!("{out}");
    hints::print(&[
        ("automation enable|disable <name>".to_string(), "toggle a built-in automation".to_string()),
        ("config rule add".to_string(), "add a custom automation rule".to_string()),
    ]);
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
        if output::is_json() {
            return print_config_mutation(None);
        }
        Logger::info(&format!("Removed global rule '{}'", args.name), None, tips);
    } else {
        let repo = git::repo::discover_current()?;
        let cfg = ProjectConfig::load(&repo)?;
        if !cfg.rules.iter().any(|r| r.name == args.name) {
            bail!("no rule named '{}' in this repo's config", args.name);
        }
        project::append_ops(&repo, vec![ConfigOp::RemoveRule { name: args.name.clone() }])?;
        if output::is_json() {
            return print_config_mutation(Some(&repo));
        }
        Logger::info(&format!("Removed rule '{}' from this repo's config", args.name), None, tips);
    }
    Ok(())
}

/// Enables/disables a built-in automation (`automation::builtins::NAMES`) at either scope —
/// the `git task automation enable|disable <name> [--global]` handler.
pub(crate) fn run_automation_toggle(name: String, global: bool, enabled: bool) -> Result<()> {
    if !builtins::is_known(&name) {
        bail!("unknown built-in automation '{name}' (known: {})", builtins::NAMES.join(", "));
    }
    let tips: &[(String, String)] = &[("config show".to_string(), "view the effective config".to_string())];
    let action = if enabled { "enabled" } else { "disabled" };

    if global {
        let mut cfg = GlobalConfig::load()?;
        cfg.set_automation_enabled(name.clone(), enabled);
        cfg.save()?;
        if output::is_json() {
            return print_config_mutation(None);
        }
        Logger::info(&format!("'{name}' {action} (global, this machine)"), None, tips);
    } else {
        let repo = git::repo::discover_current()?;
        project::append_ops(&repo, vec![ConfigOp::SetAutomationEnabled { name: name.clone(), enabled }])?;
        if output::is_json() {
            return print_config_mutation(Some(&repo));
        }
        Logger::info(&format!("'{name}' {action} (this repo)"), None, tips);
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
        if output::is_json() {
            return print_config_mutation(None);
        }
        Logger::info("Saved rule to ~/.config/git-task/automation.toml", None, tips);
    } else {
        let repo = git::repo::discover_current()?;
        let name = rule.name.clone();
        project::append_ops(&repo, vec![ConfigOp::UpsertRule { rule }])?;
        if output::is_json() {
            return print_config_mutation(Some(&repo));
        }
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
