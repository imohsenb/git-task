use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
use crate::identity;
use crate::logger::{task_ref, Logger};
use crate::output;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct VersionArgs {
    id: String,
    #[command(subcommand)]
    action: VersionAction,
}

#[derive(Subcommand)]
enum VersionAction {
    /// Add a fixed version (the version this task's fix ships in)
    FixedAdd { version: String },
    /// Remove a fixed version
    FixedRm { version: String },
    /// Add an affected version (a version impacted by this task)
    AffectedAdd { version: String },
    /// Remove an affected version
    AffectedRm { version: String },
}

pub fn run(args: VersionArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let key = project::effective_key_for(&repo)?;
    let task_id = store.resolve(&args.id)?;

    let task = store.load(&task_id)?;
    let (op, action, field, version) = match args.action {
        VersionAction::FixedAdd { version } => {
            if task.fixed_versions.contains(&version) {
                bail!("{} already has fixed version '{version}'", id::display(&key, &task_id));
            }
            (Operation::AddFixedVersion { version: version.clone() }, "Added fixed version", "fixed version", version)
        }
        VersionAction::FixedRm { version } => {
            if !task.fixed_versions.contains(&version) {
                bail!("{} has no fixed version '{version}'", id::display(&key, &task_id));
            }
            (
                Operation::RemoveFixedVersion { version: version.clone() },
                "Removed fixed version",
                "fixed version",
                version,
            )
        }
        VersionAction::AffectedAdd { version } => {
            if task.affected_versions.contains(&version) {
                bail!("{} already has affected version '{version}'", id::display(&key, &task_id));
            }
            (
                Operation::AddAffectedVersion { version: version.clone() },
                "Added affected version",
                "affected version",
                version,
            )
        }
        VersionAction::AffectedRm { version } => {
            if !task.affected_versions.contains(&version) {
                bail!("{} has no affected version '{version}'", id::display(&key, &task_id));
            }
            (
                Operation::RemoveAffectedVersion { version: version.clone() },
                "Removed affected version",
                "affected version",
                version,
            )
        }
    };

    store.append(&task_id, &author, vec![op.clone()])?;
    let automation_events = automation::engine::run(&repo, &task_id, &[op.clone()])?;
    automation::engine::print_fired(&automation_events);

    if output::is_json() {
        let task = store.load(&task_id)?;
        let directory = identity::contributor_directory(&repo)?;
        output::print_mutation(&task, &key, &directory, &[op], automation_events, None);
        return Ok(());
    }

    let display_id = id::display(&key, &task_id);
    Logger::info(
        &format!("{action} {}", task_ref(&display_id, task.kind, &task.title)),
        Some(&format!("{field} \"{version}\"")),
        &[],
    );
    Ok(())
}
