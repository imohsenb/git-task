use anyhow::Result;
use clap::Args;

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
use crate::logger::{task_ref, Logger};
use crate::store::git_store::Store;

#[derive(Args)]
pub struct StatusArgs {
    id: String,
    /// New status (free-form — workflows are not enforced in v1)
    status: String,
}

pub fn run(args: StatusArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let task_id = store.resolve(&args.id)?;

    let ops = vec![Operation::SetStatus { status: args.status.clone() }];
    store.append(&task_id, &author, ops.clone())?;
    let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
    automation::engine::print_fired(&automation_events);
    let key = project::effective_key_for(&repo)?;
    let display_id = id::display(&key, &task_id);
    let task = store.load(&task_id)?;
    Logger::info(
        &format!("Status changed {}", task_ref(&display_id, task.kind, &task.title)),
        Some(&format!("now {}", args.status)),
        &[(format!("show {display_id}"), "view the updated task".to_string())],
    );
    Ok(())
}
