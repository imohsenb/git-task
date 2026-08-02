use anyhow::Result;
use clap::Args;

use crate::actor::Actor;
use crate::automation;
use crate::cli::wizard;
use crate::config::config_op::ConfigOp;
use crate::config::fields;
use crate::config::global::GlobalConfig;
use crate::config::project::{self, ProjectConfig};
use crate::domain::id;
use crate::domain::op::{Operation, Priority, TaskKind};
use crate::git;
use crate::identity;
use crate::logger::{task_ref, Logger};
use crate::output::ClassifiedError;
use crate::prompt;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct NewArgs {
    /// Task title (prompted for if omitted, when running interactively)
    title: Option<String>,
    #[arg(long, value_enum, default_value = "task")]
    kind: TaskKind,
    /// Description (prompted for if omitted, when running interactively)
    #[arg(long = "desc")]
    description: Option<String>,
    #[arg(long)]
    assignee: Option<String>,
    /// Repeatable: --label x --label y
    #[arg(long = "label")]
    labels: Vec<String>,
    #[arg(long, value_enum)]
    priority: Option<Priority>,
    #[arg(long)]
    due: Option<String>,
    #[arg(long)]
    milestone: Option<String>,
    /// Parent epic (id or KEY-hash address)
    #[arg(long)]
    parent: Option<String>,
}

pub fn run(args: NewArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let workdir = git::repo::workdir(&repo)?;
    let store = Store::new(&repo);

    let global_cfg = GlobalConfig::load()?;
    let project_cfg = ProjectConfig::load(&repo)?;
    let required = fields::resolve(&global_cfg.fields, &project_cfg.fields);
    let key = project_cfg.effective_key(&workdir);

    let mut missing: Vec<&str> = Vec::new();
    if args.title.is_none() {
        missing.push("title");
    }
    if args.description.is_none() {
        missing.push("description");
    }
    if required.priority && args.priority.is_none() {
        missing.push("priority");
    }
    if required.assignee && args.assignee.is_none() {
        missing.push("assignee");
    }
    if required.due && args.due.is_none() {
        missing.push("due");
    }

    if !missing.is_empty() && !prompt::is_interactive() {
        let message = format!(
            "missing required field(s): {} — pass them as flags (not running interactively, so nothing to prompt)",
            missing.join(", ")
        );
        return Err(anyhow::Error::new(ClassifiedError::Validation {
            message,
            field: None,
            missing: missing.into_iter().map(str::to_string).collect(),
        }));
    }

    let title = match args.title {
        Some(t) => t,
        None => prompt::ask_required("Title")?,
    };
    let description = match args.description {
        Some(d) => d,
        None => prompt::ask_required("Description")?,
    };
    let priority = match args.priority {
        Some(p) => Some(p),
        None if required.priority => Some(ask_required_priority()?),
        None => None,
    };
    let assignee = match args.assignee {
        Some(a) => Some(identity::validate_email(&a)?),
        None if required.assignee => Some(ask_required_assignee(&repo)?),
        None => None,
    };
    let due = match args.due {
        Some(d) => Some(d),
        None if required.due => Some(prompt::ask_required("Due date")?),
        None => None,
    };

    let mut ops = vec![Operation::CreateTask {
        title: title.clone(),
        kind: args.kind,
        description,
    }];
    if let Some(email) = assignee {
        ops.push(Operation::SetAssignee { email });
    }
    if let Some(priority) = priority {
        ops.push(Operation::SetPriority { priority });
    }
    if let Some(due) = due {
        ops.push(Operation::SetDueDate { due });
    }
    for label in args.labels {
        ops.push(Operation::AddLabel { label });
    }

    if let Some(milestone) = args.milestone {
        ops.push(Operation::SetMilestone { milestone });
    }
    let parent = args.parent.map(|p| store.resolve(&p)).transpose()?;
    if let Some(parent) = parent {
        ops.push(Operation::SetParent { parent });
    }

    // First task in a repo with no pinned key: lock in the derived key now, so it stays
    // stable even if the working directory later gets renamed or cloned elsewhere.
    if project_cfg.key.is_none() {
        project::append_ops(&repo, vec![ConfigOp::SetKey { key: key.clone() }])?;
    }

    let task_id = store.create(&author, ops.clone())?;
    let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
    automation::engine::print_fired(&automation_events);
    let display_id = id::display(&key, &task_id);
    Logger::info(
        &format!("Created {}", task_ref(&display_id, args.kind, &title)),
        None,
        &[
            (format!("show {display_id}"), "view full details".to_string()),
            (format!("status {display_id} doing"), "mark it in progress".to_string()),
        ],
    );
    Ok(())
}

const PRIORITY_OPTIONS: &[&str] = &["low", "medium", "high"];

/// Priority is a closed low/medium/high enum, so a required-but-omitted priority gets a
/// numbered menu instead of `prompt::ask_required`'s free-text loop.
fn ask_required_priority() -> Result<Priority> {
    let choice = wizard::prompt_choice("Priority", PRIORITY_OPTIONS, 1)?;
    Ok(Priority::from_str_loose(PRIORITY_OPTIONS[choice]).expect("prompt_choice returns a valid index into PRIORITY_OPTIONS"))
}

/// Assignee needs a real email (`identity::validate_email`), not free text, so it gets its own
/// prompt: lists every email `identity::contributor_directory` already knows about (the closest
/// thing to autocomplete these line-based prompts can do) as a numbered menu, and still accepts
/// a freshly typed email for someone not in it yet.
fn ask_required_assignee(repo: &git2::Repository) -> Result<String> {
    let contributors = identity::sorted_contributors(repo)?;
    if !contributors.is_empty() {
        println!("known contributors:");
        for (i, (email, name)) in contributors.iter().enumerate() {
            println!("  {}) {name} <{email}>", i + 1);
        }
    }
    loop {
        let raw = wizard::prompt("Assignee (number or email)")?;
        match raw.parse::<usize>() {
            Ok(n) if n >= 1 && n <= contributors.len() => return Ok(contributors[n - 1].0.clone()),
            Ok(n) => println!("no contributor #{n} — pick 1-{}, or type an email", contributors.len()),
            Err(_) => match identity::validate_email(&raw) {
                Ok(email) => return Ok(email),
                Err(err) => println!("{err:#}"),
            },
        }
    }
}
