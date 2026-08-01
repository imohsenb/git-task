use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::fields::FieldMap;

const CONFIG_FILE: &str = "config.toml";
const DEFAULT_PROJECT: &str = "main";

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_project")]
    pub default_project: String,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoEntry>,
    /// Default required-field schema, overridable per-project via .gittask/config.toml.
    #[serde(default, skip_serializing_if = "FieldMap::is_empty")]
    pub fields: FieldMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub project: String,
}

fn default_project() -> String {
    DEFAULT_PROJECT.to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            default_project: default_project(),
            repos: BTreeMap::new(),
            fields: FieldMap::new(),
        }
    }
}

impl GlobalConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(CONFIG_FILE);
        let text = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Registers `path` under `name`, returning the project it landed in.
    pub fn register(&mut self, name: String, path: PathBuf, project: Option<String>) -> Result<String> {
        if self.repos.contains_key(&name) {
            bail!(
                "a repo named '{name}' is already registered; pass a different name or run 'git task unregister {name}' first"
            );
        }
        let project = project.unwrap_or_else(|| self.default_project.clone());
        self.repos.insert(
            name,
            RepoEntry {
                path,
                project: project.clone(),
            },
        );
        Ok(project)
    }

    pub fn unregister(&mut self, name: &str) -> bool {
        self.repos.remove(name).is_some()
    }

    pub fn projects(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, entry) in &self.repos {
            map.entry(entry.project.clone()).or_default().push(name.clone());
        }
        map
    }
}

/// `${GIT_TASK_CONFIG_DIR}` > `${XDG_CONFIG_HOME}/git-task` > `~/.config/git-task`.
/// Deliberately XDG-style on macOS too (not `~/Library/Application Support`), so the
/// same config path and docs apply on both platforms.
pub fn config_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("GIT_TASK_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Ok(PathBuf::from(xdg).join("git-task"));
        }
    }
    let home = directories::BaseDirs::new()
        .context("could not determine home directory")?
        .home_dir()
        .to_path_buf();
    Ok(home.join(".config").join("git-task"))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILE))
}
