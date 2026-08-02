use anyhow::{Context, Result};
use clap::Args;

use crate::actor::Actor;
use crate::color;
use crate::git;
use crate::logger::Logger;
use crate::output::{Classify, ClassifiedError};
use crate::store::git_store::{Store, CONFIG_ID};
use crate::store::merge::{self, Outcome};

#[derive(Args)]
pub struct PullArgs {
    /// Remote to pull from (defaults to "origin")
    remote: Option<String>,
}

pub fn run(args: PullArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let remote_name = args.remote.unwrap_or_else(|| "origin".to_string());

    let mut remote = repo.find_remote(&remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}'"),
    })?;

    let remote_prefix = format!("refs/remote-tasks/{remote_name}/");
    let fetch_refspec = format!("refs/tasks/*:{remote_prefix}*");
    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(git::repo::remote_callbacks());
    remote.fetch(&[&fetch_refspec], Some(&mut opts), None).classify_err(|| ClassifiedError::Remote {
        message: format!("fetching tasks from '{remote_name}'"),
    })?;

    let store = Store::new(&repo);
    let refs = repo.references_glob(&format!("{remote_prefix}*"))?;

    let (mut new_count, mut ff_count, mut merged_count, mut up_to_date_count) = (0, 0, 0, 0);
    // The reserved config ref reconciles with the same DAG logic, but it isn't a task — track it
    // separately so it never inflates the task tallies in the summary.
    let mut config_outcome: Option<Outcome> = None;

    for r in refs {
        let r = r?;
        let name = r.name().context("remote-tracking ref name is not valid utf-8")?;
        let Some(id) = name.strip_prefix(&remote_prefix) else { continue };
        let remote_tip = r
            .target()
            .with_context(|| format!("{name} is not a direct reference"))?;

        let outcome = merge::reconcile(&store, &id.to_string(), remote_tip, &author)?;
        if id == CONFIG_ID {
            config_outcome = Some(outcome);
            continue;
        }
        match outcome {
            Outcome::New => new_count += 1,
            Outcome::FastForwarded => ff_count += 1,
            Outcome::Merged => merged_count += 1,
            Outcome::UpToDate => up_to_date_count += 1,
        }
    }

    Logger::info(
        &format!(
            "Pulled from '{remote_name}': {new_count} new, {ff_count} fast-forwarded, {merged_count} merged, {up_to_date_count} up to date"
        ),
        None,
        &[],
    );
    match config_outcome {
        Some(Outcome::New) => Logger::plain(&color::dim(&format!("config: initialized from '{remote_name}'"))),
        Some(Outcome::FastForwarded) => Logger::plain(&color::dim("config: updated")),
        Some(Outcome::Merged) => Logger::plain(&color::dim("config: merged")),
        Some(Outcome::UpToDate) | None => {}
    }
    Ok(())
}
