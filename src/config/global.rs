use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::fields::FieldMap;
use crate::output::ClassifiedError;

fn conflict(message: String) -> anyhow::Error {
    anyhow::Error::new(ClassifiedError::Conflict { message })
}

fn not_found(message: String, query: String) -> anyhow::Error {
    anyhow::Error::new(ClassifiedError::NotFound { message, query, entity: "project".to_string() })
}

const CONFIG_FILE: &str = "config.toml";
const DEFAULT_PROJECT: &str = "main";

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_project")]
    pub default_project: String,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoEntry>,
    /// Projects that exist but currently have no repo registered under them yet — created via
    /// `git task project create`. A project with repos doesn't need an entry here; `known_projects`
    /// unions this set with every `RepoEntry::project` tag and `default_project` so callers never
    /// have to check both places.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub projects: BTreeSet<String>,
    /// Default required-field schema, overridable per-project via `git task config field`.
    #[serde(default, skip_serializing_if = "FieldMap::is_empty")]
    pub fields: FieldMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub project: String,
    /// This repo's `origin` remote URL, captured at `register` time — gives cross-repo
    /// task links (`domain::op::Operation::AddLink::target_repo`) a portable identity: any
    /// machine that has this same repo registered can look its local path back up via
    /// `path_for_remote`, regardless of the local path it happens to live at there.
    #[serde(default)]
    pub remote: Option<String>,
}

fn default_project() -> String {
    DEFAULT_PROJECT.to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            default_project: default_project(),
            repos: BTreeMap::new(),
            projects: BTreeSet::new(),
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
    pub fn register(&mut self, name: String, path: PathBuf, project: Option<String>, remote: Option<String>) -> Result<String> {
        if self.repos.contains_key(&name) {
            return Err(conflict(format!(
                "a repo named '{name}' is already registered; pass a different name or run 'git task unregister {name}' first"
            )));
        }
        let project = project.unwrap_or_else(|| self.default_project.clone());
        self.repos.insert(
            name,
            RepoEntry {
                path,
                project: project.clone(),
                remote,
            },
        );
        Ok(project)
    }

    /// The local path of a registered repo whose `origin` remote matches `url`, comparing
    /// through `domain::remote::normalize` so ssh/https/scp-like forms of the same URL all
    /// match — the "per-machine map" a cross-repo link's `target_repo` URL resolves through.
    pub fn path_for_remote(&self, url: &str) -> Option<&Path> {
        let target = crate::domain::remote::normalize(url);
        self.repos
            .values()
            .find(|e| e.remote.as_deref().is_some_and(|r| crate::domain::remote::normalize(r) == target))
            .map(|e| e.path.as_path())
    }

    /// The registered name/entry whose `path` matches `path` exactly (both must already be
    /// canonicalized — `git::repo::workdir` and `RepoEntry::path` both are) — "is this repo
    /// registered, and under which project" for whichever call site needs to know (the current
    /// repo's own project for a same-project cross-repo check, a banner, an `ls` label, ...).
    pub fn entry_for_path(&self, path: &Path) -> Option<(&str, &RepoEntry)> {
        self.repos.iter().find(|(_, e)| e.path == path).map(|(n, e)| (n.as_str(), e))
    }

    pub fn unregister(&mut self, name: &str) -> bool {
        self.repos.remove(name).is_some()
    }

    /// Every project name currently in play: explicitly created (possibly empty) ones, every
    /// repo's assigned project, and `default_project` (always valid even before anyone runs
    /// `project create`, since fresh configs implicitly have a "main" project).
    pub fn known_projects(&self) -> BTreeSet<String> {
        let mut set = self.projects.clone();
        set.insert(self.default_project.clone());
        for entry in self.repos.values() {
            set.insert(entry.project.clone());
        }
        set
    }

    pub fn projects(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for name in self.known_projects() {
            map.entry(name).or_default();
        }
        for (name, entry) in &self.repos {
            map.entry(entry.project.clone()).or_default().push(name.clone());
        }
        map
    }

    /// Creates an empty project so it shows up (and can be targeted by `--project`) before any
    /// repo joins it. Registering a repo under a not-yet-existing project name still works and
    /// creates it implicitly — this is only for setting one up ahead of time.
    pub fn create_project(&mut self, name: &str) -> Result<()> {
        if self.known_projects().contains(name) {
            return Err(conflict(format!("project '{name}' already exists")));
        }
        self.projects.insert(name.to_string());
        Ok(())
    }

    pub fn set_default_project(&mut self, name: &str) -> Result<()> {
        if !self.known_projects().contains(name) {
            return Err(not_found(
                format!("no such project '{name}'; run 'git task project create {name}' first"),
                name.to_string(),
            ));
        }
        self.default_project = name.to_string();
        Ok(())
    }

    /// Renames a project and re-tags every repo registered under it, so a rename never leaves
    /// repos pointing at a name that no longer exists.
    pub fn rename_project(&mut self, old: &str, new: &str) -> Result<()> {
        if !self.known_projects().contains(old) {
            return Err(not_found(format!("no such project '{old}'"), old.to_string()));
        }
        if old == new {
            return Ok(());
        }
        if self.known_projects().contains(new) {
            return Err(conflict(format!("project '{new}' already exists")));
        }
        self.projects.remove(old);
        self.projects.insert(new.to_string());
        for entry in self.repos.values_mut() {
            if entry.project == old {
                entry.project = new.to_string();
            }
        }
        if self.default_project == old {
            self.default_project = new.to_string();
        }
        Ok(())
    }

    /// Refuses to delete the default project or one that still has repos, rather than silently
    /// reassigning or unregistering them — that's a decision the user should make explicitly.
    pub fn delete_project(&mut self, name: &str) -> Result<()> {
        if !self.known_projects().contains(name) {
            return Err(not_found(format!("no such project '{name}'"), name.to_string()));
        }
        if self.default_project == name {
            return Err(conflict(format!(
                "'{name}' is the default project; set a different default first ('git task project set-default <name>')"
            )));
        }
        let repo_count = self.repos.values().filter(|e| e.project == name).count();
        if repo_count > 0 {
            return Err(conflict(format!(
                "project '{name}' still has {repo_count} repo(s) registered; unregister them (or re-register under another project) first"
            )));
        }
        self.projects.remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_for_remote_matches_across_url_forms() {
        let mut config = GlobalConfig::default();
        config
            .register("backend".to_string(), PathBuf::from("/repos/backend"), None, Some("git@github.com:org/backend.git".to_string()))
            .unwrap();

        assert_eq!(config.path_for_remote("https://github.com/org/backend.git"), Some(Path::new("/repos/backend")));
        assert_eq!(config.path_for_remote("ssh://git@github.com/org/backend"), Some(Path::new("/repos/backend")));
    }

    #[test]
    fn path_for_remote_none_when_no_repo_has_that_remote() {
        let mut config = GlobalConfig::default();
        config.register("backend".to_string(), PathBuf::from("/repos/backend"), None, Some("git@github.com:org/backend.git".to_string())).unwrap();

        assert_eq!(config.path_for_remote("https://github.com/org/other.git"), None);
    }

    #[test]
    fn path_for_remote_skips_repos_with_no_remote() {
        let mut config = GlobalConfig::default();
        config.register("local-only".to_string(), PathBuf::from("/repos/local-only"), None, None).unwrap();

        assert_eq!(config.path_for_remote("https://github.com/org/local-only.git"), None);
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
