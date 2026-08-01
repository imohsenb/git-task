use anyhow::Result;
use git2::Oid;

use crate::actor::Actor;
use crate::domain::id::TaskId;
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

/// Reconciles one task's local ref against a fetched remote tip.
pub fn reconcile(store: &Store, id: &TaskId, remote_tip: Oid, author: &Actor) -> Result<Outcome> {
    let Some(local_tip) = store.find_tip(id)? else {
        store.set_ref(id, remote_tip, false)?;
        return Ok(Outcome::New);
    };

    if local_tip == remote_tip {
        return Ok(Outcome::UpToDate);
    }
    if store.is_ancestor(local_tip, remote_tip)? {
        store.set_ref(id, remote_tip, true)?;
        return Ok(Outcome::FastForwarded);
    }
    if store.is_ancestor(remote_tip, local_tip)? {
        return Ok(Outcome::UpToDate);
    }

    store.merge(id, local_tip, remote_tip, author)?;
    Ok(Outcome::Merged)
}
