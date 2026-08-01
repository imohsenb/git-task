use anyhow::{bail, Result};
use clap::Args;

use crate::actor::Actor;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::{Operation, TaskKind};
use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct EditArgs {
    id: String,
    #[arg(long = "title")]
    title: Option<String>,
    #[arg(long = "desc")]
    description: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<TaskKind>,
    #[arg(long)]
    priority: Option<String>,
    #[arg(long)]
    assignee: Option<String>,
    #[arg(long)]
    due: Option<String>,
}

pub fn run(args: EditArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let task_id = store.resolve(&args.id)?;

    let mut ops = Vec::new();
    if let Some(title) = args.title {
        ops.push(Operation::SetTitle { title });
    }
    if let Some(description) = args.description {
        ops.push(Operation::SetDescription { description });
    }
    if let Some(kind) = args.kind {
        ops.push(Operation::SetKind { kind });
    }
    if let Some(priority) = args.priority {
        ops.push(Operation::SetPriority { priority });
    }
    if let Some(assignee) = args.assignee {
        ops.push(Operation::SetAssignee { assignee });
    }
    if let Some(due) = args.due {
        ops.push(Operation::SetDueDate { due });
    }

    if ops.is_empty() {
        bail!("nothing to edit — pass at least one of --title, --desc, --kind, --priority, --assignee, --due");
    }

    store.append(&task_id, &author, ops)?;
    let key = project::effective_key_for(&repo)?;
    println!("updated {}", id::display(&key, &task_id));
    Ok(())
}
