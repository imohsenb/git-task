use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::actor::Actor;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
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
    Add { child: String },
    /// Remove a task from this epic
    Rm { child: String },
}

pub fn run(args: EpicArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let epic_id = store.resolve(&args.epic)?;

    match args.action {
        EpicAction::Add { child } => {
            let child_id = store.resolve(&child)?;
            if child_id == epic_id {
                bail!("a task cannot be its own parent");
            }
            store.append(&child_id, &author, vec![Operation::SetParent { parent: epic_id.clone() }])?;
            println!(
                "{} is now a child of {}",
                id::display(&key, &child_id),
                id::display(&key, &epic_id)
            );
        }
        EpicAction::Rm { child } => {
            let child_id = store.resolve(&child)?;
            let task = store.load(&child_id)?;
            if task.parent.as_deref() != Some(epic_id.as_str()) {
                bail!(
                    "{} is not a child of {}",
                    id::display(&key, &child_id),
                    id::display(&key, &epic_id)
                );
            }
            store.append(&child_id, &author, vec![Operation::ClearParent])?;
            println!("{} removed from {}", id::display(&key, &child_id), id::display(&key, &epic_id));
        }
    }
    Ok(())
}
