use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::actor::Actor;
use crate::automation;
use crate::cli::target_repo;
use crate::config::global::GlobalConfig;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::{LinkKind, Operation};
use crate::domain::task::Link;
use crate::git;
use crate::identity;
use crate::logger::{task_ref, Logger};
use crate::output;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct LinkArgs {
    id: String,
    #[command(subcommand)]
    action: LinkAction,
}

#[derive(Subcommand)]
enum LinkAction {
    /// Add a link from this task to another
    Add {
        #[arg(value_enum)]
        kind: LinkKind,
        other: String,
        /// Target repo for a cross-repo link: a registered repo name, a local filesystem
        /// path, or a remote URL. Omit for a same-repo link (the default).
        #[arg(long)]
        repo: Option<String>,
    },
    /// Remove an existing link
    Rm {
        #[arg(value_enum)]
        kind: LinkKind,
        other: String,
        #[arg(long)]
        repo: Option<String>,
    },
}

pub fn run(args: LinkArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;

    match args.action {
        LinkAction::Add { kind, other, repo: repo_arg } => match repo_arg {
            None => {
                let other_id = store.resolve(&other)?;
                if other_id == task_id {
                    bail!("a task cannot link to itself");
                }
                let task = store.load(&task_id)?;
                if task.links.iter().any(|l| l.kind == kind && l.target == other_id && l.target_repo.is_none()) {
                    bail!(
                        "{} already has a {kind:?} link to {}",
                        id::display(&key, &task_id),
                        id::display(&key, &other_id)
                    );
                }
                let ops = vec![Operation::AddLink { kind, target: other_id.clone(), target_repo: None, target_label: None }];
                store.append(&task_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&task_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let display_id = id::display(&key, &task_id);
                let other_display = id::display(&key, &other_id);
                Logger::info(
                    &format!("Linked {}", task_ref(&display_id, task.kind, &task.title)),
                    Some(&format!("{kind:?} → {other_display}")),
                    &[],
                );
            }
            Some(repo_arg) => {
                let mut global_cfg = GlobalConfig::load()?;
                let target_repo = target_repo::resolve(&repo_arg, &mut global_cfg)?.identifier;
                let target = id::normalize_ref_input(&other).to_string();
                let task = store.load(&task_id)?;
                if task
                    .links
                    .iter()
                    .any(|l| l.kind == kind && l.target == target && Link::same_target_repo(&l.target_repo, &Some(target_repo.clone())))
                {
                    bail!(
                        "{} already has a {kind:?} link to {} @ {}",
                        id::display(&key, &task_id),
                        other,
                        target_repo
                    );
                }
                let ops = vec![Operation::AddLink {
                    kind,
                    target: target.clone(),
                    target_repo: Some(target_repo.clone()),
                    target_label: Some(other.clone()),
                }];
                store.append(&task_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&task_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let display_id = id::display(&key, &task_id);
                Logger::info(
                    &format!("Linked {}", task_ref(&display_id, task.kind, &task.title)),
                    Some(&format!("{kind:?} → {other} @ {target_repo}")),
                    &[],
                );
            }
        },
        LinkAction::Rm { kind, other, repo: repo_arg } => match repo_arg {
            None => {
                let other_id = store.resolve(&other)?;
                let task = store.load(&task_id)?;
                if !task.links.iter().any(|l| l.kind == kind && l.target == other_id && l.target_repo.is_none()) {
                    bail!(
                        "no {kind:?} link from {} to {}",
                        id::display(&key, &task_id),
                        id::display(&key, &other_id)
                    );
                }
                let ops = vec![Operation::RemoveLink { kind, target: other_id.clone(), target_repo: None }];
                store.append(&task_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&task_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let display_id = id::display(&key, &task_id);
                let other_display = id::display(&key, &other_id);
                Logger::info(
                    &format!("Unlinked {}", task_ref(&display_id, task.kind, &task.title)),
                    Some(&format!("no longer {kind:?} → {other_display}")),
                    &[],
                );
            }
            Some(repo_arg) => {
                let mut global_cfg = GlobalConfig::load()?;
                let target_repo = target_repo::resolve(&repo_arg, &mut global_cfg)?.identifier;
                let target = id::normalize_ref_input(&other).to_string();
                let task = store.load(&task_id)?;
                if !task
                    .links
                    .iter()
                    .any(|l| l.kind == kind && l.target == target && Link::same_target_repo(&l.target_repo, &Some(target_repo.clone())))
                {
                    bail!(
                        "no {kind:?} link from {} to {} @ {}",
                        id::display(&key, &task_id),
                        other,
                        target_repo
                    );
                }
                let ops = vec![Operation::RemoveLink { kind, target: target.clone(), target_repo: Some(target_repo.clone()) }];
                store.append(&task_id, &author, ops.clone())?;
                let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
                automation::engine::print_fired(&automation_events);

                if output::is_json() {
                    let reloaded = store.load(&task_id)?;
                    let directory = identity::contributor_directory(&repo)?;
                    output::print_mutation(&reloaded, &key, &directory, &ops, automation_events, None);
                    return Ok(());
                }

                let display_id = id::display(&key, &task_id);
                Logger::info(
                    &format!("Unlinked {}", task_ref(&display_id, task.kind, &task.title)),
                    Some(&format!("no longer {kind:?} → {other} @ {target_repo}")),
                    &[],
                );
            }
        },
    }
    Ok(())
}
