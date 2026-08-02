use anyhow::{bail, Result};
use clap::Args;

use crate::config::project;
use crate::domain::id;
use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct DropArgs {
    id: String,
    /// Required to confirm: `drop` deletes the local `refs/tasks/<id>` ref outright, with
    /// no event and no history entry. It does not sync — `push` has nothing to push once
    /// the ref is gone, and a later `pull`/`clone` from a peer that still has this task
    /// will bring it right back. Prefer `delete` unless you specifically want that.
    #[arg(long)]
    force: bool,
}

pub fn run(args: DropArgs) -> Result<()> {
    if !args.force {
        bail!(
            "drop permanently removes the local ref with no event and does not sync (a later \
             pull from a peer that still has this task brings it back). Pass --force to confirm, \
             or use `delete` for a synced, history-recorded removal."
        );
    }

    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;
    let display_id = id::display(&key, &task_id);

    store.drop(&task_id)?;
    println!("{display_id} dropped (local ref removed, not synced)");
    Ok(())
}
