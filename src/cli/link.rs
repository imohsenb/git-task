use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::{LinkKind, Operation};
use crate::git;
use crate::logger::{task_ref, Logger};
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
    },
    /// Remove an existing link
    Rm {
        #[arg(value_enum)]
        kind: LinkKind,
        other: String,
    },
}

pub fn run(args: LinkArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;

    match args.action {
        LinkAction::Add { kind, other } => {
            let other_id = store.resolve(&other)?;
            if other_id == task_id {
                bail!("a task cannot link to itself");
            }
            let task = store.load(&task_id)?;
            if task.links.iter().any(|l| l.kind == kind && l.target == other_id) {
                bail!(
                    "{} already has a {kind:?} link to {}",
                    id::display(&key, &task_id),
                    id::display(&key, &other_id)
                );
            }
            let ops = vec![Operation::AddLink { kind, target: other_id.clone() }];
            store.append(&task_id, &author, ops.clone())?;
            automation::engine::run(&repo, &task_id, &ops)?;
            let display_id = id::display(&key, &task_id);
            let other_display = id::display(&key, &other_id);
            Logger::info(
                &format!("Linked {}", task_ref(&display_id, task.kind, &task.title)),
                Some(&format!("{kind:?} → {other_display}")),
                &[],
            );
        }
        LinkAction::Rm { kind, other } => {
            let other_id = store.resolve(&other)?;
            let task = store.load(&task_id)?;
            if !task.links.iter().any(|l| l.kind == kind && l.target == other_id) {
                bail!(
                    "no {kind:?} link from {} to {}",
                    id::display(&key, &task_id),
                    id::display(&key, &other_id)
                );
            }
            let ops = vec![Operation::RemoveLink { kind, target: other_id.clone() }];
            store.append(&task_id, &author, ops.clone())?;
            automation::engine::run(&repo, &task_id, &ops)?;
            let display_id = id::display(&key, &task_id);
            let other_display = id::display(&key, &other_id);
            Logger::info(
                &format!("Unlinked {}", task_ref(&display_id, task.kind, &task.title)),
                Some(&format!("no longer {kind:?} → {other_display}")),
                &[],
            );
        }
    }
    Ok(())
}
