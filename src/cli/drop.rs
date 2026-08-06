use std::cell::RefCell;

use anyhow::{bail, Result};
use clap::Args;
use serde::Serialize;

use crate::color;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::TaskKind;
use crate::git;
use crate::logger::{task_ref, Logger};
use crate::output::{self, Classify, ClassifiedError};
use crate::store::git_store::Store;

/// `drop`'s own shape, not `MutationJson` — the task no longer exists after this, so there is no
/// final `Task` state to report, just what got removed and (if `--remote` was given and
/// succeeded) which remote it was also deleted from.
#[derive(Serialize)]
struct DropJson {
    id: String,
    display_id: String,
    title: String,
    kind: TaskKind,
    remote_deleted: Option<String>,
}

#[derive(Args)]
pub struct DropArgs {
    id: String,
    /// Required to confirm: `drop` deletes the local `refs/tasks/<id>` ref outright, with
    /// no event and no history entry. Prefer `delete` unless you specifically want that.
    #[arg(long)]
    force: bool,
    /// Also delete the ref on this remote (defaults to "origin" if given with no name).
    /// Note this only reaches the one remote named — any *other* clone that already fetched
    /// this task still has it locally and will happily recreate the remote ref (or your local
    /// one, via `pull`) the next time it pushes. There is no way to force-delete from clones
    /// this command doesn't know about; `delete` (a synced tombstone event) is the only way to
    /// make a removal actually propagate to everyone.
    #[arg(long, num_args = 0..=1, default_missing_value = "origin")]
    remote: Option<String>,
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
    let task = store.load(&task_id)?;

    store.drop(&task_id)?;

    let Some(remote_name) = args.remote else {
        if output::is_json() {
            output::print_ok(DropJson {
                id: task_id,
                display_id,
                title: task.title,
                kind: task.kind,
                remote_deleted: None,
            });
            return Ok(());
        }
        Logger::info(
            &format!("Dropped {}", task_ref(&display_id, task.kind, &task.title)),
            Some("local ref removed, not synced"),
            &[],
        );
        return Ok(());
    };

    let mut remote = repo.find_remote(&remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}' (local ref was already removed)"),
    })?;

    // Empty src side of the refspec is git's delete-on-remote form (`git push origin
    // :refs/tasks/<id>`), not an append-side no-op.
    let delete_refspec = format!(":refs/tasks/{task_id}");

    let rejected = RefCell::new(None::<String>);
    let mut callbacks = git::repo::remote_callbacks();
    callbacks.push_update_reference(|refname, status| {
        if let Some(msg) = status {
            *rejected.borrow_mut() = Some(format!("{refname}: {msg}"));
        }
        Ok(())
    });
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(callbacks);

    let push_result = remote.push(&[delete_refspec.as_str()], Some(&mut opts)).classify_err(|| ClassifiedError::Remote {
        message: format!("deleting task ref on '{remote_name}' (local ref was already removed)"),
    });
    drop(opts);
    push_result?;

    if let Some(msg) = rejected.into_inner() {
        let refname = msg.split_once(":").map(|(r, _)| r).unwrap_or(&msg).to_string();
        return Err(anyhow::Error::new(ClassifiedError::Rejected {
            message: format!("'{remote_name}' rejected the delete (local ref was already removed): {msg}"),
            refs: vec![refname],
        }));
    }

    if output::is_json() {
        output::print_ok(DropJson {
            id: task_id,
            display_id,
            title: task.title,
            kind: task.kind,
            remote_deleted: Some(remote_name),
        });
        return Ok(());
    }

    Logger::info(
        &format!("Dropped {}", task_ref(&display_id, task.kind, &task.title)),
        Some(&format!("local ref removed, deleted on '{remote_name}'")),
        &[],
    );
    Logger::plain(&color::dim("note: any other clone that already has this task can still bring it back on its next push"));
    Ok(())
}
