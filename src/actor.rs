use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};

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
        let name = config
            .get_string("user.name")
            .context("git config user.name is not set")?;
        let email = config
            .get_string("user.email")
            .context("git config user.email is not set")?;
        Ok(Self { name, email })
    }
}
