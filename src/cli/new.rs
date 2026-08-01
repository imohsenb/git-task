use anyhow::{bail, Result};
use clap::Args;

use crate::actor::Actor;
use crate::config::fields;
use crate::config::global::GlobalConfig;
use crate::config::project::ProjectConfig;
use crate::domain::id;
use crate::domain::op::{Operation, TaskKind};
use crate::git;
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
    #[arg(long)]
    priority: Option<String>,
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
    let project_cfg = ProjectConfig::load(&workdir)?;
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
        bail!(
            "missing required field(s): {} — pass them as flags (not running interactively, so nothing to prompt)",
            missing.join(", ")
        );
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
        None if required.priority => Some(prompt::ask_required("Priority")?),
        None => None,
    };
    let assignee = match args.assignee {
        Some(a) => Some(a),
        None if required.assignee => Some(prompt::ask_required("Assignee")?),
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
    if let Some(assignee) = assignee {
        ops.push(Operation::SetAssignee { assignee });
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

    let task_id = store.create(&author, ops)?;
    println!("created {} — {title}", id::display(&key, &task_id));
    Ok(())
}
