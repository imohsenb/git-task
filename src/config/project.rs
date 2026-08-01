use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::config::fields::FieldMap;

const PROJECT_DIR: &str = ".gittask";
const PROJECT_FILE: &str = "config.toml";

/// Per-repo config, tracked in git under `.gittask/config.toml` so it's the
/// same for every clone — unlike the user-level global config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub fields: FieldMap,
}

impl ProjectConfig {
    pub fn load(workdir: &Path) -> Result<Self> {
        let path = config_path(workdir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, workdir: &Path) -> Result<()> {
        let dir = workdir.join(PROJECT_DIR);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(PROJECT_FILE);
        let text = toml::to_string_pretty(self).context("serializing project config")?;
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn effective_key(&self, workdir: &Path) -> String {
        self.key.clone().unwrap_or_else(|| default_key(workdir))
    }
}

/// Convenience for CLI commands that only need the display key for the
/// current repo, without dealing with `ProjectConfig` directly.
pub fn effective_key_for(repo: &Repository) -> Result<String> {
    let workdir = crate::git::repo::workdir(repo)?;
    let cfg = ProjectConfig::load(&workdir)?;
    Ok(cfg.effective_key(&workdir))
}

fn config_path(workdir: &Path) -> PathBuf {
    workdir.join(PROJECT_DIR).join(PROJECT_FILE)
}

fn default_key(workdir: &Path) -> String {
    let name = workdir.file_name().and_then(|n| n.to_str()).unwrap_or("task");
    let key: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if key.is_empty() {
        "TASK".to_string()
    } else {
        key
    }
}
