use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;

use crate::actor::Actor;
use crate::automation::rules;
use crate::color;
use crate::config::project;
use crate::domain::id;
use crate::git;
use crate::logger::Logger;
use crate::output::{self, Classify, ClassifiedError};
use crate::store::git_store::{Store, CONFIG_ID};
use crate::store::merge::{self, Outcome};

#[derive(Args)]
pub struct PullArgs {
    /// Remote to pull from (defaults to "origin")
    remote: Option<String>,
}

#[derive(Serialize)]
struct PullCounts {
    new: usize,
    fast_forwarded: usize,
    merged: usize,
    up_to_date: usize,
}

#[derive(Serialize)]
struct PullTaskJson {
    id: String,
    display_id: String,
    outcome: &'static str,
}

#[derive(Serialize)]
struct PullJson {
    remote: String,
    counts: PullCounts,
    config: Option<&'static str>,
    tasks: Vec<PullTaskJson>,
}

fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::New => "new",
        Outcome::FastForwarded => "fast_forwarded",
        Outcome::Merged => "merged",
        Outcome::UpToDate => "up_to_date",
    }
}

pub fn run(args: PullArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let remote_name = args.remote.unwrap_or_else(|| "origin".to_string());

    let mut remote = repo.find_remote(&remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}'"),
    })?;

    // Captured before the fetch/reconcile below can move the config ref, so it reflects the
    // project automation rules as they stood prior to this pull.
    let old_rules = project::ProjectConfig::load(&repo)?.rules;

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
    let mut tasks = Vec::new();

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
        tasks.push((id.to_string(), outcome));
    }

    if matches!(config_outcome, Some(Outcome::FastForwarded) | Some(Outcome::Merged)) {
        let new_rules = project::ProjectConfig::load(&repo)?.rules;
        let changed = rules::changed_or_added(&old_rules, &new_rules);
        if !changed.is_empty() {
            let names = changed.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ");
            Logger::warn(
                &format!("project automation rule(s) changed: {names}"),
                Some("these run automatically on your next mutating command"),
                &[("config show".to_string(), "review the effective rules".to_string())],
            );
        }
    }

    if output::is_json() {
        let key = project::effective_key_for(&repo)?;
        let tasks_json = tasks
            .into_iter()
            .map(|(task_id, outcome)| PullTaskJson {
                display_id: id::display(&key, &task_id),
                id: task_id,
                outcome: outcome_str(outcome),
            })
            .collect();
        output::print_ok(PullJson {
            remote: remote_name,
            counts: PullCounts { new: new_count, fast_forwarded: ff_count, merged: merged_count, up_to_date: up_to_date_count },
            config: config_outcome.map(outcome_str),
            tasks: tasks_json,
        });
        return Ok(());
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
