use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
use crate::logger::{task_ref, Logger};
use crate::store::git_store::Store;

#[derive(Args)]
pub struct LabelArgs {
    id: String,
    #[command(subcommand)]
    action: LabelAction,
}

#[derive(Subcommand)]
enum LabelAction {
    Add { label: String },
    Rm { label: String },
}

pub fn run(args: LabelArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;

    let task = store.load(&task_id)?;
    let (op, action, label) = match args.action {
        LabelAction::Add { label } => {
            if task.labels.contains(&label) {
                bail!("{} already has label '{label}'", id::display(&key, &task_id));
            }
            (Operation::AddLabel { label: label.clone() }, "Added label", label)
        }
        LabelAction::Rm { label } => {
            if !task.labels.contains(&label) {
                bail!("{} has no label '{label}'", id::display(&key, &task_id));
            }
            (Operation::RemoveLabel { label: label.clone() }, "Removed label", label)
        }
    };

    store.append(&task_id, &author, vec![op.clone()])?;
    let automation_events = automation::engine::run(&repo, &task_id, &[op])?;
    automation::engine::print_fired(&automation_events);
    let display_id = id::display(&key, &task_id);
    Logger::info(
        &format!("{action} {}", task_ref(&display_id, task.kind, &task.title)),
        Some(&format!("label \"{label}\"")),
        &[],
    );
    Ok(())
}
