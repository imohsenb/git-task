use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const LOCK_FILE: &str = "sync.lock";
const DIRTY_FILE: &str = "sync.dirty";

/// Coordinates the background auto-sync worker (`sync::worker`) so at most one runs per repo at
/// a time. Lives under `<git-dir>/git-task/` — a transient runtime scratch directory *inside*
/// git's own directory, not the working-tree `.gittask/` footprint CLAUDE.md forbids for
/// persisted task/config data: nothing here is task/config data, it's discarded the moment the
/// worker exits, same spirit as git's own `.git/index.lock`.
///
/// Staleness is an mtime-age heuristic, not PID-liveness or an OS-level advisory lock (`flock`) —
/// deliberately, to avoid a new dependency (`fs2`/`nix`/`libc`; none are in `Cargo.toml` today,
/// and `std::fs::File::try_lock` isn't stable on this toolchain). The narrow race this leaves (two
/// workers both steal a just-expired lock at once) is tolerable because `store::remote::push_all`/
/// `pull_all` already handle concurrent writers safely (`store::merge::reconcile`'s
/// compare-and-swap retry) — worst case here is a wasted duplicate sync pass, not corruption.
pub struct SyncLock {
    dir: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Acquire {
    Acquired,
    AlreadyRunning,
}

impl SyncLock {
    pub fn new(git_dir: &Path) -> Self {
        Self { dir: git_dir.join("git-task") }
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE)
    }

    fn dirty_path(&self) -> PathBuf {
        self.dir.join(DIRTY_FILE)
    }

    /// Marks this repo as having a pending change to sync. Idempotent under rapid repeated
    /// calls — the worker only ever compares the marker's mtime, not a change count.
    pub fn mark_dirty(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        touch(&self.dirty_path())
    }

    pub fn dirty_mtime(&self) -> io::Result<Option<SystemTime>> {
        match fs::metadata(self.dirty_path()) {
            Ok(meta) => Ok(Some(meta.modified()?)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Claims the lock, atomically creating it if absent. If it already exists and is older
    /// than `stale_after`, treats it as abandoned by a crashed prior worker, steals it, and
    /// retries once — a second racer stealing it in that same instant just gets `AlreadyRunning`.
    pub fn try_acquire(&self, stale_after: Duration) -> io::Result<Acquire> {
        fs::create_dir_all(&self.dir)?;
        let path = self.lock_path();
        match create_new(&path) {
            Ok(()) => Ok(Acquire::Acquired),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let age = fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok());
                if age.is_some_and(|a| a > stale_after) {
                    let _ = fs::remove_file(&path);
                    Ok(match create_new(&path) {
                        Ok(()) => Acquire::Acquired,
                        Err(_) => Acquire::AlreadyRunning,
                    })
                } else {
                    Ok(Acquire::AlreadyRunning)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Refreshes the lock's mtime so a long-running pass isn't mistaken for stale by another
    /// trigger mid-sync.
    pub fn heartbeat(&self) -> io::Result<()> {
        touch(&self.lock_path())
    }

    pub fn release(&self) -> io::Result<()> {
        match fs::remove_file(self.lock_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

fn create_new(path: &Path) -> io::Result<()> {
    OpenOptions::new().create_new(true).write(true).open(path).map(drop)
}

/// Creates `path` if absent, else updates its mtime to now — used for both the dirty marker
/// (create-or-bump) and the lock's heartbeat (must already exist).
fn touch(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().create(true).write(true).truncate(false).open(path)?;
    file.set_modified(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn fresh_acquire_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        assert_eq!(lock.try_acquire(Duration::from_secs(120)).unwrap(), Acquire::Acquired);
    }

    #[test]
    fn second_acquire_while_held_is_already_running() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        lock.try_acquire(Duration::from_secs(120)).unwrap();
        assert_eq!(lock.try_acquire(Duration::from_secs(120)).unwrap(), Acquire::AlreadyRunning);
    }

    #[test]
    fn release_then_reacquire_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        lock.try_acquire(Duration::from_secs(120)).unwrap();
        lock.release().unwrap();
        assert_eq!(lock.try_acquire(Duration::from_secs(120)).unwrap(), Acquire::Acquired);
    }

    #[test]
    fn stale_lock_gets_stolen() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        lock.try_acquire(Duration::from_secs(120)).unwrap();

        // Backdate the lock file well past a short staleness window, simulating a crashed
        // prior worker that never released it.
        let old = SystemTime::now() - Duration::from_secs(999);
        File::open(lock.lock_path()).unwrap().set_modified(old).unwrap();

        assert_eq!(lock.try_acquire(Duration::from_millis(1)).unwrap(), Acquire::Acquired);
    }

    #[test]
    fn mark_dirty_then_dirty_mtime_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        assert!(lock.dirty_mtime().unwrap().is_none());
        lock.mark_dirty().unwrap();
        assert!(lock.dirty_mtime().unwrap().is_some());
    }

    #[test]
    fn heartbeat_bumps_lock_mtime_forward() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SyncLock::new(dir.path());
        lock.try_acquire(Duration::from_secs(120)).unwrap();

        let old = SystemTime::now() - Duration::from_secs(60);
        File::open(lock.lock_path()).unwrap().set_modified(old).unwrap();

        lock.heartbeat().unwrap();
        let age = fs::metadata(lock.lock_path()).unwrap().modified().unwrap().elapsed().unwrap();
        assert!(age < Duration::from_secs(5), "heartbeat should have refreshed the mtime to ~now");
    }
}
