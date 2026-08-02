use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::color;
use crate::config::global::{config_dir, GlobalConfig, RepoEntry};
use crate::config::project;
use crate::git;
use crate::output::{self, identity::IdentityJson};
use crate::store::git_store::Store;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct ReposArgs {
    /// `--format json` only: also open each repo and probe it (key, branch, task counts,
    /// remotes, identity) instead of just listing the bare registry entry. Never fails the
    /// command because one repo is unopenable — that repo gets `openable: false` and an `error`
    /// instead.
    #[arg(long)]
    deep: bool,
}

#[derive(Serialize)]
struct RemoteJson {
    name: String,
    url: Option<String>,
    push_url: Option<String>,
}

#[derive(Serialize)]
struct RepoEntryJson {
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

#[derive(Serialize)]
struct ReposJson {
    config_dir: String,
    default_project: String,
    projects: Vec<String>,
    repos: Vec<RepoEntryJson>,
}

pub fn run(args: ReposArgs) -> Result<()> {
    let config = GlobalConfig::load()?;

    if output::is_json() {
        print_repos_json(&config, args.deep);
        return Ok(());
    }

    if config.repos.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let headers = ["NAME", "PROJECT", "PATH"];
    let rows: Vec<Vec<Seg>> = config
        .repos
        .iter()
        .map(|(name, entry)| {
            vec![
                Seg { colored: color::cyan(name), plain: name.clone() },
                Seg { colored: color::dim(&entry.project), plain: entry.project.clone() },
                Seg { colored: entry.path.display().to_string(), plain: entry.path.display().to_string() },
            ]
        })
        .collect();

    let title = format!("REPOS ({})", rows.len());
    for line in table::list_box(&title, &headers, rows) {
        println!("{line}");
    }
    Ok(())
}

fn print_repos_json(config: &GlobalConfig, deep: bool) {
    let resolved_config_dir = config_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let projects: Vec<String> = config.known_projects().into_iter().collect();
    let repos = config.repos.iter().map(|(name, entry)| build_repo_entry(name, entry, deep)).collect();

    output::print_ok(ReposJson {
        config_dir: resolved_config_dir,
        default_project: config.default_project.clone(),
        projects,
        repos,
    });
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
            output::collect_warning(&format!("skipping deep probe of '{name}'"), Some(&message), Some(name));
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

    let identity = output::identity::effective(&repo);

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
