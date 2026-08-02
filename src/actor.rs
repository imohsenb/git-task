use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::output::ClassifiedError;

/// The identity attached to every operation an author records (git user.name/user.email).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub name: String,
    pub email: String,
}

impl Actor {
    #[allow(dead_code)] // wired into op authorship in the store layer (phase 2)
    pub fn from_repo(repo: &Repository) -> Result<Self> {
        let config = repo.config().context("reading git config")?;
        let name = config.get_string("user.name").ok();
        let email = config.get_string("user.email").ok();
        if let (Some(name), Some(email)) = (&name, &email) {
            return Ok(Self { name: name.clone(), email: email.clone() });
        }

        let mut missing = Vec::new();
        if name.is_none() {
            missing.push("user.name".to_string());
        }
        if email.is_none() {
            missing.push("user.email".to_string());
        }
        let path = repo.workdir().unwrap_or_else(|| repo.path()).display().to_string();
        let config_files = consulted_config_files(repo);
        let message = format!("git config {} not set", missing.join(" and "));
        Err(anyhow::Error::new(ClassifiedError::IdentityMissing { message, path, missing, config_files }))
    }
}

/// Every config file libgit2 actually consults for `user.name`/`user.email`, in the order it
/// reads them — so the frontend can point the user at the exact file to edit rather than a
/// generic "git config is missing" message.
fn consulted_config_files(repo: &Repository) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(path) = git2::Config::find_global() {
        files.push(path.display().to_string());
    }
    if let Ok(path) = git2::Config::find_system() {
        files.push(path.display().to_string());
    }
    files.push(repo.path().join("config").display().to_string());
    files
}
