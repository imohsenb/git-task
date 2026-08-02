use anyhow::{bail, Result};
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
pub struct DeleteArgs {
    id: String,
}

/// Soft delete: appends `Operation::DeleteTask` like any other mutation, so it's recorded
/// in history and syncs via the normal push/pull/merge path — a peer who already has this
/// task locally picks up the deletion on their next `pull`, same as any other edit. There
/// is no `restore`; for a local-only hard delete that doesn't sync, see `drop`.
pub fn run(args: DeleteArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;
    let display_id = id::display(&key, &task_id);

    let task = store.load(&task_id)?;
    if task.deleted {
        bail!("{display_id} is already deleted");
    }

    let ops = vec![Operation::DeleteTask];
    store.append(&task_id, &author, ops.clone())?;
    automation::engine::run(&repo, &task_id, &ops)?;
    Logger::info(
        &format!("Deleted {}", task_ref(&display_id, task.kind, &task.title)),
        None,
        &[
            ("ls --deleted".to_string(), "view deleted tasks".to_string()),
            (format!("drop {display_id} --force"), "permanently remove it locally instead".to_string()),
        ],
    );
    Ok(())
}
