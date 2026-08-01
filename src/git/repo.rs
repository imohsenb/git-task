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

pub fn workdir(repo: &Repository) -> Result<std::path::PathBuf> {
    let dir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no working directory (bare repo?)"))?;
    dir.canonicalize()
        .with_context(|| format!("resolving {}", dir.display()))
}
