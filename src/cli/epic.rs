use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use git2::Repository;

use crate::actor::Actor;
use crate::automation;
use crate::cli::target_repo;
use crate::config::global::GlobalConfig;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::domain::remote;
use crate::git;
use crate::identity;
use crate::logger::{task_ref, Logger};
use crate::output::{self, ClassifiedError};
use crate::store::git_store::Store;

#[derive(Args)]
pub struct EpicArgs {
    /// The epic (parent) task
    epic: String,
    #[command(subcommand)]
    action: EpicAction,
}

#[derive(Subcommand)]
enum EpicAction {
    /// Make a task a child of this epic
    Add {
        child: String,
        /// The epic lives in another repo, in the same project as this one: a registered repo
        /// name, a local filesystem path, or a remote URL. Omit for a same-repo epic (the
        /// default). The child is always resolved in the current repo.
        #[arg(long)]
        repo: Option<String>,
    },
    /// Remove a task from this epic
    Rm {
        child: String,
        #[arg(long)]
        repo: Option<String>,
    },
}

/// This repo's own registered project — a cross-repo epic requires both sides registered
/// under the *same* project, so there has to be one to compare against.
fn current_project(repo: &Repository, config: &GlobalConfig) -> Result<String> {
    let workdir = git::repo::workdir(repo)?;
    config.entry_for_path(&workdir).map(|(_, e)| e.project.clone()).ok_or_else(|| {
        anyhow::Error::new(ClassifiedError::Validation {
            message: "this repo isn't registered under a project — cross-repo epics require both repos registered under the same project; run 'git task register' here first".to_string(),
            field: Some("repo".to_string()),
            missing: Vec::new(),
        })
    })
}

fn unregistered_target(repo_arg: &str, current: &str) -> anyhow::Error {
    anyhow::Error::new(ClassifiedError::Validation {
        message: format!(
            "epic repo '{repo_arg}' isn't registered under any project — cross-repo epics require it registered under this repo's project ('{current}'); run 'git task register' there first"
        ),
        field: Some("repo".to_string()),
        missing: Vec::new(),
    })
}

fn project_mismatch(repo_arg: &str, current: &str, target: &str) -> anyhow::Error {
    anyhow::Error::new(ClassifiedError::Validation {
        message: format!(
            "epic repo '{repo_arg}' is registered under project '{target}', but this repo is under '{current}' — cross-repo epics require both repos in the same project"
        ),
        field: Some("repo".to_string()),
        missing: Vec::new(),
    })
}

/// Resolves and validates a cross-repo epic's `--repo` argument against `repo`'s own project,
/// then opens it and confirms the epic itself actually exists there — returning its real id
/// (not just a normalized hash prefix, unlike a cross-repo `link`) so `parent` always stores
/// something exact enough to match against later, and a typo'd epic id fails loudly here
/// rather than recording a dead reference.
fn resolve_cross_repo_epic(repo: &Repository, epic: &str, repo_arg: &str) -> Result<(String, target_repo::ResolvedRepo)> {
    let mut global_cfg = GlobalConfig::load()?;
    let resolved = target_repo::resolve(repo_arg, &mut global_cfg)?;
    let current = current_project(repo, &global_cfg)?;
    let target_project = resolved.project.clone().ok_or_else(|| unregistered_target(repo_arg, &current))?;
    if target_project != current {
        return Err(project_mismatch(repo_arg, &current, &target_project));
    }
    let local_path = resolved
        .local_path
        .clone()
        .expect("target_repo::resolve always pairs a known project with a known local path");
    let other_repo = git::repo::open(&local_path)?;
    let other_store = Store::new(&other_repo);
    let epic_full_id = other_store.resolve(epic)?;
    Ok((epic_full_id, resolved))
}

pub fn run(args: EpicArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;

    match args.action {
        EpicAction::Add { child, repo: repo_arg } => match repo_arg {
            None => {
                let epic_id = store.resolve(&args.epic)?;
                let child_id = store.resolve(&child)?;
                if child_id == epic_id {
                    bail!("a task cannot be its own parent");
                }
                let child_task = store.load(&child_id)?;
                let ops = vec![Operation::SetParent { parent: epic_id.clone(), parent_repo: None, parent_label: None }];
                store.append(&child_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &child_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let task = store.load(&child_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&task, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let child_display = id::display(&key, &child_id);
                let epic_display = id::display(&key, &epic_id);
                Logger::info(
                    &format!("Linked to epic {}", task_ref(&child_display, child_task.kind, &child_task.title)),
                    Some(&format!("child of {epic_display}")),
                    &[],
                );
            }
            Some(repo_arg) => {
                let (epic_full_id, resolved) = resolve_cross_repo_epic(&repo, &args.epic, &repo_arg)?;
                let child_id = store.resolve(&child)?;
                let child_task = store.load(&child_id)?;
                let ops = vec![Operation::SetParent {
                    parent: epic_full_id,
                    parent_repo: Some(resolved.identifier.clone()),
                    parent_label: Some(args.epic.clone()),
                }];
                store.append(&child_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &child_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let task = store.load(&child_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&task, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let child_display = id::display(&key, &child_id);
                Logger::info(
                    &format!("Linked to epic {}", task_ref(&child_display, child_task.kind, &child_task.title)),
                    Some(&format!("child of {} @ {}", args.epic, resolved.identifier)),
                    &[],
                );
            }
        },
        EpicAction::Rm { child, repo: repo_arg } => match repo_arg {
            None => {
                let epic_id = store.resolve(&args.epic)?;
                let child_id = store.resolve(&child)?;
                let task = store.load(&child_id)?;
                if task.parent.as_deref() != Some(epic_id.as_str()) || task.parent_repo.is_some() {
                    bail!(
                        "{} is not a child of {}",
                        id::display(&key, &child_id),
                        id::display(&key, &epic_id)
                    );
                }
                let ops = vec![Operation::ClearParent];
                store.append(&child_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &child_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&child_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let child_display = id::display(&key, &child_id);
                let epic_display = id::display(&key, &epic_id);
                Logger::info(
                    &format!("Removed from epic {}", task_ref(&child_display, task.kind, &task.title)),
                    Some(&format!("was child of {epic_display}")),
                    &[],
                );
            }
            Some(repo_arg) => {
                let (epic_full_id, resolved) = resolve_cross_repo_epic(&repo, &args.epic, &repo_arg)?;
                let child_id = store.resolve(&child)?;
                let task = store.load(&child_id)?;
                let matches = task.parent.as_deref() == Some(epic_full_id.as_str())
                    && remote::same(&task.parent_repo, &Some(resolved.identifier.clone()));
                if !matches {
                    bail!(
                        "{} is not a child of {} @ {}",
                        id::display(&key, &child_id),
                        args.epic,
                        resolved.identifier
                    );
                }
                let ops = vec![Operation::ClearParent];
                store.append(&child_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &child_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&child_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let child_display = id::display(&key, &child_id);
                Logger::info(
                    &format!("Removed from epic {}", task_ref(&child_display, task.kind, &task.title)),
                    Some(&format!("was child of {} @ {}", args.epic, resolved.identifier)),
                    &[],
                );
            }
        },
    }
    Ok(())
}
