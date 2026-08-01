use anyhow::Result;
use clap::Args;

use crate::actor::Actor;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
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

    store.append(
        &task_id,
        &author,
        vec![Operation::SetStatus { status: args.status.clone() }],
    )?;
    let key = project::effective_key_for(&repo)?;
    println!("{} -> {}", id::display(&key, &task_id), args.status);
    Ok(())
}
