use anyhow::Result;
use clap::Args;

use crate::actor::Actor;
use crate::domain::id;
use crate::domain::op::{Operation, TaskKind};
use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct NewArgs {
    /// Task title
    title: String,
    #[arg(long, value_enum, default_value = "task")]
    kind: TaskKind,
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
}

pub fn run(args: NewArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);

    let mut ops = vec![Operation::CreateTask {
        title: args.title.clone(),
        kind: args.kind,
        description: args.description.unwrap_or_default(),
    }];
    if let Some(assignee) = args.assignee {
        ops.push(Operation::SetAssignee { assignee });
    }
    if let Some(priority) = args.priority {
        ops.push(Operation::SetPriority { priority });
    }
    if let Some(due) = args.due {
        ops.push(Operation::SetDueDate { due });
    }
    for label in args.labels {
        ops.push(Operation::AddLabel { label });
    }

    let task_id = store.create(&author, ops)?;
    println!("created {} — {}", id::short(&task_id), args.title);
    Ok(())
}
