use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::git;
use crate::logger::Logger;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct CloneArgs {
    /// URL of the remote repo to clone tasks from
    url: String,
    /// Directory to create (defaults to the repo name from the URL, suffixed "-tasks")
    dir: Option<PathBuf>,
}

/// Fetches `refs/tasks/*` from a remote into a brand-new repo — no working-tree checkout,
/// no source history. Unlike `pull` (which reconciles a fetched remote tip against a local
/// one that may have diverged), a fresh clone has nothing to reconcile against, so this
/// writes straight into `refs/tasks/*` instead of going through `merge::reconcile`.
pub fn run(args: CloneArgs) -> Result<()> {
    let dir = args.dir.unwrap_or_else(|| default_dir(&args.url));

    if dir.exists() {
        let mut entries = dir.read_dir().with_context(|| format!("reading '{}'", dir.display()))?;
        if entries.next().is_some() {
            bail!("'{}' already exists and is not empty", dir.display());
        }
    }

    let repo = git2::Repository::init(&dir).with_context(|| format!("initializing repo at '{}'", dir.display()))?;
    let mut remote = repo
        .remote("origin", &args.url)
        .with_context(|| format!("adding remote 'origin' -> '{}'", args.url))?;

    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(git::repo::remote_callbacks());
    remote
        .fetch(&["refs/tasks/*:refs/tasks/*"], Some(&mut opts), None)
        .with_context(|| format!("fetching tasks from '{}'", args.url))?;

    let count = Store::new(&repo).list_ids()?.len();
    Logger::info(&format!("Cloned {count} task(s) from '{}' into '{}'", args.url, dir.display()), None, &[]);
    Logger::plain(&format!("tip: cd {} && git task ls", dir.display()));
    Ok(())
}

/// Mirrors `git clone`'s own directory-name derivation (strip a trailing slash, strip a
/// `.git` suffix, take the last path segment) but appends "-tasks" so this never collides
/// with a real source clone of the same repo sitting in the same parent directory.
fn default_dir(url: &str) -> PathBuf {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let name = trimmed.rsplit(['/', ':']).next().filter(|s| !s.is_empty()).unwrap_or("repo");
    PathBuf::from(format!("{name}-tasks"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_from_ssh_url() {
        assert_eq!(default_dir("git@github.com:imohsenb/git-task.git"), PathBuf::from("git-task-tasks"));
    }

    #[test]
    fn default_dir_from_https_url_with_trailing_slash() {
        assert_eq!(default_dir("https://github.com/imohsenb/git-task/"), PathBuf::from("git-task-tasks"));
    }
}
