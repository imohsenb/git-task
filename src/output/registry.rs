use serde::Serialize;

use crate::config::global::{config_dir, GlobalConfig, RepoEntry};
use crate::config::project;
use crate::git;
use crate::output::identity::{self, IdentityJson};
use crate::store::git_store::Store;

#[derive(Serialize)]
pub struct RemoteJson {
    name: String,
    url: Option<String>,
    push_url: Option<String>,
}

#[derive(Serialize)]
pub struct RepoEntryJson {
    name: String,
    path: String,
    project: String,
    exists: Option<bool>,
    openable: Option<bool>,
    key: Option<String>,
    branch: Option<String>,
    task_count: Option<usize>,
    open_task_count: Option<usize>,
    remotes: Option<Vec<RemoteJson>>,
    identity: Option<IdentityJson>,
    error: Option<String>,
}

/// Shallow by default (`deep: false` — only the bare registry entries; every probed field is
/// explicit `null`, not omitted, so a consumer can tell "not probed" from "probed and empty").
/// Used by `repos` itself and by `register`/`unregister`/`project`'s mutation responses (always
/// shallow there — returning the complete new registry in one round trip is the point, not a
/// full re-probe of every repo on every mutation).
#[derive(Serialize)]
pub struct ReposJson {
    config_dir: String,
    default_project: String,
    projects: Vec<String>,
    repos: Vec<RepoEntryJson>,
}

/// Builds the `ReposJson` payload without printing it.
pub fn build(config: &GlobalConfig, deep: bool) -> ReposJson {
    let resolved_config_dir = config_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let projects: Vec<String> = config.known_projects().into_iter().collect();
    let repos = config.repos.iter().map(|(name, entry)| build_repo_entry(name, entry, deep)).collect();
    ReposJson { config_dir: resolved_config_dir, default_project: config.default_project.clone(), projects, repos }
}

fn empty_entry(name: &str, entry: &RepoEntry, exists: Option<bool>, openable: Option<bool>, error: Option<String>) -> RepoEntryJson {
    RepoEntryJson {
        name: name.to_string(),
        path: entry.path.display().to_string(),
        project: entry.project.clone(),
        exists,
        openable,
        key: None,
        branch: None,
        task_count: None,
        open_task_count: None,
        remotes: None,
        identity: None,
        error,
    }
}

fn build_repo_entry(name: &str, entry: &RepoEntry, deep: bool) -> RepoEntryJson {
    if !deep {
        return empty_entry(name, entry, None, None, None);
    }

    if !entry.path.exists() {
        return empty_entry(name, entry, Some(false), Some(false), Some("path does not exist".to_string()));
    }

    let repo = match git::repo::open(&entry.path) {
        Ok(r) => r,
        Err(err) => {
            let message = format!("{err:#}");
            super::collect_warning(&format!("skipping deep probe of '{name}'"), Some(&message), Some(name));
            return empty_entry(name, entry, Some(true), Some(false), Some(message));
        }
    };

    let key = project::effective_key_for(&repo).ok();
    let branch = git::repo::current_branch(&repo);

    let store = Store::new(&repo);
    let (task_count, open_task_count) = match store.list_ids() {
        Ok(ids) => {
            let mut total = 0usize;
            let mut open = 0usize;
            for id in &ids {
                if let Ok(task) = store.load(id) {
                    total += 1;
                    if !task.deleted {
                        open += 1;
                    }
                }
            }
            (Some(total), Some(open))
        }
        Err(_) => (None, None),
    };

    let remotes = repo.remotes().ok().map(|names| {
        names
            .iter()
            .flatten()
            .filter_map(|remote_name| repo.find_remote(remote_name).ok())
            .map(|r| RemoteJson {
                name: r.name().unwrap_or_default().to_string(),
                url: r.url().map(str::to_string),
                push_url: r.pushurl().map(str::to_string),
            })
            .collect()
    });

    let identity = identity::effective(&repo);

    RepoEntryJson {
        name: name.to_string(),
        path: entry.path.display().to_string(),
        project: entry.project.clone(),
        exists: Some(true),
        openable: Some(true),
        key,
        branch,
        task_count,
        open_task_count,
        remotes,
        identity: Some(identity),
        error: None,
    }
}

/// The shape `register`, `unregister`, and `project create|rename|set-default|delete` all
/// return: what happened (`action`), the name it happened to (a repo name for
/// register/unregister, a project name for `project`'s subactions), and the complete new
/// registry so a caller refreshes in one round trip. `project`/`previous_project` are
/// deliberately reused across both repo- and project-shaped actions rather than adding a second
/// pair of fields — for `project rename`/`set-default`, `previous_project` holds the old
/// project name (there being no repo involved to have a "previous project" of its own).
#[derive(Serialize)]
pub struct RegistryMutationJson {
    pub action: &'static str,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_project: Option<String>,
    pub registry: ReposJson,
}

pub fn print_mutation(
    action: &'static str,
    name: impl Into<String>,
    project: Option<String>,
    previous_project: Option<String>,
    config: &GlobalConfig,
) {
    super::print_ok(RegistryMutationJson {
        action,
        name: name.into(),
        project,
        previous_project,
        registry: build(config, false),
    });
}
