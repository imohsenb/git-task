use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;

pub fn discover(start: &Path) -> Result<Repository> {
    Repository::discover(start)
        .with_context(|| format!("no git repository found from {}", start.display()))
}

pub fn discover_current() -> Result<Repository> {
    let cwd = std::env::current_dir().context("reading current directory")?;
    discover(&cwd)
}

/// Opens a repo at a known path (e.g. a registered repo's stored workdir), no upward search.
pub fn open(path: &Path) -> Result<Repository> {
    Repository::open(path).with_context(|| format!("opening repo at {}", path.display()))
}

/// The current branch's short name, for display purposes only (e.g. the `ls` empty-state
/// message). `repo.head()` errors on an unborn branch (freshly `git init`'d, no commits yet —
/// common for a tasks-only clone with no source checkout), so this falls back to reading HEAD's
/// symbolic target directly rather than surfacing that as an error the caller has to handle.
pub fn current_branch(repo: &Repository) -> Option<String> {
    if let Ok(head) = repo.head() {
        return head.shorthand().map(str::to_string);
    }
    let head_ref = repo.find_reference("HEAD").ok()?;
    let target = head_ref.symbolic_target()?;
    Some(target.strip_prefix("refs/heads/").unwrap_or(target).to_string())
}

pub fn workdir(repo: &Repository) -> Result<std::path::PathBuf> {
    let dir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory (bare repo?)"))?;
    dir.canonicalize()
        .with_context(|| format!("resolving {}", dir.display()))
}

/// Standard libgit2 auth: try the SSH agent for SSH remotes, otherwise fall back to
/// whatever the platform's default credential lookup finds (e.g. a stored HTTPS
/// credential helper). Shared by push and pull so both authenticate the same way.
pub fn remote_callbacks<'a>() -> git2::RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            if let Some(username) = username_from_url {
                if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }
            }
        }
        git2::Cred::default()
    });
    callbacks
}
