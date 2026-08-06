use std::path::Path;
use std::time::Duration;

use git2::Repository;

use crate::actor::Actor;
use crate::store::remote;
use crate::sync::lock::{Acquire, SyncLock};

const STALE_AFTER: Duration = Duration::from_secs(120);
const MAX_ITERATIONS: usize = 5;

/// Runs one coalesced `auto-sync` pass for the repo at `git_dir`: pull then push against
/// `"origin"`, looping while another trigger marked the repo dirty again while this pass was
/// running, so N rapid mutations collapse into one extra pass instead of N overlapping
/// network round-trips. Called only from the detached `__sync-worker` process (see
/// `cli::sync_worker`) — never inline in a foreground command.
///
/// Every failure is swallowed. This runs detached from any terminal with nothing waiting on its
/// `Result`, so there is nowhere to report an error to and no CLI command whose exit code it
/// could affect even if it wanted to (see `sync::trigger`, the confirmed "silent, always" design).
pub fn run_once(git_dir: &Path) {
    let lock = SyncLock::new(git_dir);
    let _ = lock.mark_dirty();
    if lock.try_acquire(STALE_AFTER).ok() != Some(Acquire::Acquired) {
        return;
    }

    let mut seen = lock.dirty_mtime().ok().flatten();
    for _ in 0..MAX_ITERATIONS {
        sync_once(git_dir);
        let _ = lock.heartbeat();

        let now = lock.dirty_mtime().ok().flatten();
        if now == seen {
            break;
        }
        seen = now;
    }
    let _ = lock.release();
}

fn sync_once(git_dir: &Path) {
    let Ok(repo) = Repository::open(git_dir) else { return };
    let Ok(author) = Actor::from_repo(&repo) else { return };
    let _ = remote::pull_all(&repo, "origin", &author);
    let _ = remote::push_all(&repo, "origin");
}
