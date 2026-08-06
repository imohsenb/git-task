use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::actor::Actor;
use crate::automation::rules;
use crate::color;
use crate::config::project;
use crate::domain::id;
use crate::git;
use crate::logger::Logger;
use crate::output;
use crate::store::merge::Outcome;
use crate::store::remote;

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

    // Captured before the fetch/reconcile below can move the config ref, so it reflects the
    // project automation rules as they stood prior to this pull.
    let old_rules = project::ProjectConfig::load(&repo)?.rules;

    let result = remote::pull_all(&repo, &remote_name, &author)?;

    let (mut new_count, mut ff_count, mut merged_count, mut up_to_date_count) = (0, 0, 0, 0);
    for (_, outcome) in &result.tasks {
        match outcome {
            Outcome::New => new_count += 1,
            Outcome::FastForwarded => ff_count += 1,
            Outcome::Merged => merged_count += 1,
            Outcome::UpToDate => up_to_date_count += 1,
        }
    }

    if matches!(result.config_outcome, Some(Outcome::FastForwarded) | Some(Outcome::Merged)) {
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
        let tasks_json = result
            .tasks
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
            config: result.config_outcome.map(outcome_str),
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
    match result.config_outcome {
        Some(Outcome::New) => Logger::plain(&color::dim(&format!("config: initialized from '{remote_name}'"))),
        Some(Outcome::FastForwarded) => Logger::plain(&color::dim("config: updated")),
        Some(Outcome::Merged) => Logger::plain(&color::dim("config: merged")),
        Some(Outcome::UpToDate) | None => {}
    }
    Ok(())
}
