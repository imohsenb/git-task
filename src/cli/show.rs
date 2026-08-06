use anyhow::Result;
use clap::Args;
use git2::Repository;

use crate::config::global::GlobalConfig;
use crate::config::project;
use crate::domain::id;
use crate::domain::remote;
use crate::domain::task::Task;
use crate::git;
use crate::hints;
use crate::identity;
use crate::output::{self, ChildJson};
use crate::render;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct ShowArgs {
    id: String,
    /// Print markdown instead of the boxed detail view (the old `--format md`, now that
    /// `--format` is the global text/json switch — see `--format json` for machine output)
    #[arg(long)]
    markdown: bool,
}

pub fn run(args: ShowArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let full_id = store.resolve(&args.id)?;
    let task = store.load(&full_id)?;

    let key = project::effective_key_for(&repo)?;
    let directory = identity::contributor_directory(&repo)?;
    let children = collect_children(&repo, &store, &task, &key)?;

    if output::is_json() {
        let mut json = output::TaskJson::from_task(&task, &key, &directory, true);
        json.children = children;
        output::print_ok(json);
        return Ok(());
    }

    if args.markdown {
        println!("{}", render::to_markdown(&task, &key, &directory, &children));
    } else {
        println!();
        println!("{}", render::to_text(&task, &key, &directory, &children));
    }
    print_follow_up_hints(&args.id);
    Ok(())
}

/// Every task whose `parent` is `task`: same-repo children read straight off the local store,
/// plus — when this repo is registered — same-project cross-repo children found by scanning
/// every other repo registered under the same project for one whose `parent_repo` resolves back
/// to this repo (`cli::epic::resolve_cross_repo_epic` is what records that on the child's own
/// side at link time; nothing on the epic itself points at its children, so listing them always
/// means a scan, not a lookup). Best-effort throughout: an unregistered repo, or one whose
/// registered path can no longer be opened, is silently skipped rather than failing `show`.
fn collect_children(repo: &Repository, store: &Store, task: &Task, key: &str) -> Result<Vec<ChildJson>> {
    let mut children = Vec::new();
    for candidate_id in store.list_ids()? {
        if candidate_id == task.id {
            continue;
        }
        let candidate = store.load(&candidate_id)?;
        if candidate.deleted {
            continue;
        }
        if candidate.parent.as_deref() == Some(task.id.as_str()) && candidate.parent_repo.is_none() {
            children.push(ChildJson {
                id: Some(candidate_id.clone()),
                display_id: id::display(key, &candidate_id),
                title: candidate.title,
                kind: candidate.kind,
                status: candidate.status,
                repo: None,
            });
        }
    }

    let Ok(config) = GlobalConfig::load() else {
        return Ok(children);
    };
    let Ok(workdir) = git::repo::workdir(repo) else {
        return Ok(children);
    };
    let Some((_, my_entry)) = config.entry_for_path(&workdir) else {
        return Ok(children);
    };
    let self_identifier = my_entry.remote.clone().unwrap_or_else(|| my_entry.path.display().to_string());
    let project = my_entry.project.clone();

    for (name, entry) in &config.repos {
        if entry.project != project || entry.path == workdir {
            continue;
        }
        let Ok(other_repo) = git::repo::open(&entry.path) else {
            continue;
        };
        let Ok(other_key) = project::effective_key_for(&other_repo) else {
            continue;
        };
        let other_store = Store::new(&other_repo);
        let Ok(ids) = other_store.list_ids() else {
            continue;
        };
        for oid in ids {
            let Ok(candidate) = other_store.load(&oid) else {
                continue;
            };
            if candidate.deleted {
                continue;
            }
            if candidate.parent.as_deref() == Some(task.id.as_str())
                && remote::same(&candidate.parent_repo, &Some(self_identifier.clone()))
            {
                children.push(ChildJson {
                    id: None,
                    display_id: id::display(&other_key, &oid),
                    title: candidate.title,
                    kind: candidate.kind,
                    status: candidate.status,
                    repo: Some(name.clone()),
                });
            }
        }
    }

    Ok(children)
}

fn print_follow_up_hints(id: &str) {
    hints::print(&[
        (format!("status {id} <status>"), "change status".to_string()),
        (format!("comment {id} \"...\""), "add a comment".to_string()),
        (format!("edit {id} --title \"...\""), "edit fields".to_string()),
    ]);
}
