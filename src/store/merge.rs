use anyhow::Result;
use git2::Oid;

use crate::actor::Actor;
use crate::domain::id::TaskId;
use crate::output::ClassifiedError;
use crate::store::git_store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Task didn't exist locally; ref created pointing straight at the remote tip.
    New,
    /// Local tip already equal to, or a descendant of, the remote tip — nothing to do.
    UpToDate,
    /// Remote was strictly ahead; local ref moved straight to the remote tip.
    FastForwarded,
    /// Local and remote had each moved on independently; reconciled with a merge commit.
    Merged,
}

/// Reconciles one task's local ref against a fetched remote tip. The `New` case is already
/// race-safe on its own (`set_ref(force: false)` only creates the ref if it's still absent).
/// The fast-forward and merge cases both act on a `local_tip` read at the top of this call, so a
/// second writer (a concurrent `pull`, or a local `append`) that moves the ref in between would
/// otherwise get silently overwritten by this one's force-update — guarded here the same way
/// `Store::append` guards its own race, by compare-and-swapping against the `local_tip` this call
/// actually observed and retrying once, against a freshly re-read tip, before giving up.
pub fn reconcile(store: &Store, id: &TaskId, remote_tip: Oid, author: &Actor) -> Result<Outcome> {
    let Some(mut local_tip) = store.find_tip(id)? else {
        store.set_ref(id, remote_tip, false)?;
        return Ok(Outcome::New);
    };

    for attempt in 0..2 {
        if local_tip == remote_tip {
            return Ok(Outcome::UpToDate);
        }
        if store.is_ancestor(local_tip, remote_tip)? {
            match store.set_ref_cas(id, remote_tip, local_tip) {
                Ok(()) => return Ok(Outcome::FastForwarded),
                Err(_) if attempt == 0 => {
                    local_tip = store.tip(id)?;
                    continue;
                }
                Err(_) => return Err(anyhow::Error::new(reconcile_conflict(id))),
            }
        }
        if store.is_ancestor(remote_tip, local_tip)? {
            return Ok(Outcome::UpToDate);
        }

        match store.merge(id, local_tip, remote_tip, author) {
            Ok(()) => return Ok(Outcome::Merged),
            Err(_) if attempt == 0 => {
                local_tip = store.tip(id)?;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns within its two attempts")
}

fn reconcile_conflict(id: &TaskId) -> ClassifiedError {
    ClassifiedError::Conflict {
        message: format!(
            "task {id} was updated concurrently during sync — retried once and still lost the race; run pull again"
        ),
    }
}
