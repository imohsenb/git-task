use std::fmt;

use anyhow::{bail, Result};
use clap::Args;
use git2::Repository;

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::{Operation, Priority, TaskKind};
use crate::domain::task::Task;
use crate::git;
use crate::identity;
use crate::logger::{task_ref, Logger};
use crate::output;
use crate::prompt;
use crate::store::git_store::Store;
use crate::ui;

#[derive(Args)]
pub struct EditArgs {
    id: String,
    #[arg(long = "title")]
    title: Option<String>,
    #[arg(long = "desc")]
    description: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<TaskKind>,
    #[arg(long, value_enum)]
    priority: Option<Priority>,
    #[arg(long)]
    assignee: Option<String>,
    #[arg(long)]
    due: Option<String>,
    #[arg(long)]
    milestone: Option<String>,
}

pub fn run(args: EditArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let task_id = store.resolve(&args.id)?;
    let key = project::effective_key_for(&repo)?;

    let any_flag = args.title.is_some()
        || args.description.is_some()
        || args.kind.is_some()
        || args.priority.is_some()
        || args.assignee.is_some()
        || args.due.is_some()
        || args.milestone.is_some();

    let ops = if any_flag {
        let mut ops = Vec::new();
        if let Some(title) = args.title {
            ops.push(Operation::SetTitle { title });
        }
        if let Some(description) = args.description {
            ops.push(Operation::SetDescription { description });
        }
        if let Some(kind) = args.kind {
            ops.push(Operation::SetKind { kind });
        }
        if let Some(priority) = args.priority {
            ops.push(Operation::SetPriority { priority });
        }
        if let Some(assignee) = args.assignee {
            let email = identity::validate_email(&assignee)?;
            ops.push(Operation::SetAssignee { email });
        }
        if let Some(due) = args.due {
            ops.push(Operation::SetDueDate { due });
        }
        if let Some(milestone) = args.milestone {
            ops.push(Operation::SetMilestone { milestone });
        }
        ops
    } else if prompt::is_interactive() {
        // No flags at all on a TTY: walk every field interactively instead of erroring —
        // blank answers keep the current value, so the user only touches what they mean to change.
        let task = store.load(&task_id)?;
        let display_id = id::display(&key, &task_id);
        interactive_ops(&repo, &task, &display_id)?
    } else {
        bail!(
            "nothing to edit — pass at least one of --title, --desc, --kind, --priority, --assignee, --due, --milestone"
        );
    };

    if ops.is_empty() {
        if output::is_json() {
            let task = store.load(&task_id)?;
            let directory = identity::contributor_directory(&repo)?;
            output::print_mutation(&task, &key, &directory, &[], Vec::new(), None);
        } else {
            println!("no changes.");
        }
        return Ok(());
    }

    store.append(&task_id, &author, ops.clone())?;
    let automation_events = automation::engine::run(&repo, &task_id, &ops)?;
    automation::engine::print_fired(&automation_events);

    if output::is_json() {
        let task = store.load(&task_id)?;
        let directory = identity::contributor_directory(&repo)?;
        output::print_mutation(&task, &key, &directory, &ops, automation_events, None);
        return Ok(());
    }

    let display_id = id::display(&key, &task_id);
    let task = store.load(&task_id)?;
    Logger::info(&format!("Updated {}", task_ref(&display_id, task.kind, &task.title)), None, &[]);
    Ok(())
}

/// Walks every editable field, showing the current value as the default so pressing enter
/// leaves it untouched. Status and kind get an arrow-key menu instead of free text since both
/// are small closed-ish vocabularies (`color::status_semantic` documents the same status
/// presets used here) — picking one beats retyping the exact spelling.
fn interactive_ops(repo: &Repository, task: &Task, display_id: &str) -> Result<Vec<Operation>> {
    ui::render_header_card(
        &format!("EDIT TASK #{display_id}"),
        "Enter keeps current value  ·  ↑/↓ to navigate  ·  Esc to cancel",
    );

    let mut ops = Vec::new();
    if let Some(title) = ask_text("Title", &task.title)? {
        ops.push(Operation::SetTitle { title });
    }
    if let Some(description) = ask_text("Description", &task.description)? {
        ops.push(Operation::SetDescription { description });
    }
    if let Some(kind) = select_kind(task.kind)? {
        ops.push(Operation::SetKind { kind });
    }
    if let Some(status) = select_status(&task.status)? {
        ops.push(Operation::SetStatus { status });
    }
    if let Some(priority) = select_priority(task.priority)? {
        ops.push(Operation::SetPriority { priority });
    }
    if let Some(email) = ask_assignee(repo, task.assignee.as_deref())? {
        ops.push(Operation::SetAssignee { email });
    }
    if let Some(due) = select_due_date(task.due.as_deref())? {
        ops.push(Operation::SetDueDate { due });
    }
    if let Some(milestone) = ask_optional_text("Milestone", task.milestone.as_deref())? {
        ops.push(Operation::SetMilestone { milestone });
    }
    Ok(ops)
}

const STATUS_PRESETS: &[&str] = &["todo", "doing", "blocked", "done"];
const CUSTOM_STATUS: &str = "custom...";

fn select_status(current: &str) -> Result<Option<String>> {
    let mut options: Vec<String> = STATUS_PRESETS.iter().map(|s| s.to_string()).collect();
    if !options.iter().any(|o| o == current) {
        options.push(current.to_string());
    }
    options.push(CUSTOM_STATUS.to_string());

    let default_idx = options.iter().position(|o| o == current).unwrap_or(options.len() - 1);
    let choice = ui::prompt_select(&format!("Status (current: {current})"), options, default_idx)?;
    let chosen = if choice == CUSTOM_STATUS { ui::prompt_text("  custom status", current, None)? } else { choice };
    Ok(if chosen == current { None } else { Some(chosen) })
}

const KIND_VARIANTS: [TaskKind; 5] = [TaskKind::Bug, TaskKind::Story, TaskKind::Task, TaskKind::Epic, TaskKind::Subtask];

fn select_kind(current: TaskKind) -> Result<Option<TaskKind>> {
    let options = KIND_VARIANTS.to_vec();
    let default_idx = options.iter().position(|k| *k == current).unwrap_or(0);
    let chosen = ui::prompt_select(&format!("Kind (current: {current})"), options, default_idx)?;
    Ok(if chosen == current { None } else { Some(chosen) })
}

const PRIORITY_VARIANTS: [Priority; 3] = [Priority::Low, Priority::Medium, Priority::High];

/// Same shape as `select_kind`: priority is a closed low/medium/high enum, so it gets an
/// arrow-key menu instead of `ask_optional_text`'s free-text prompt. There's no "clear" choice
/// here — same as before this field became an enum, priority has no `Operation` to unset it
/// once set, so this only ever moves it to a different tier.
fn select_priority(current: Option<Priority>) -> Result<Option<Priority>> {
    let options = PRIORITY_VARIANTS.to_vec();
    let default_idx = current.and_then(|c| options.iter().position(|p| *p == c)).unwrap_or(1);
    let label = match current {
        Some(c) => format!("Priority (current: {c})"),
        None => "Priority (current: none)".to_string(),
    };
    let chosen = ui::prompt_select(&label, options, default_idx)?;
    Ok(if Some(chosen) == current { None } else { Some(chosen) })
}

fn ask_text(label: &str, current: &str) -> Result<Option<String>> {
    let answer = ui::prompt_text(label, current, None)?;
    Ok(if answer == current { None } else { Some(answer) })
}

/// One entry in the assignee picker: either a known contributor (pre-selected when they're
/// already the assignee) or the escape hatch into a freshly typed email. A small `Display`
/// wrapper rather than a `String` because `prompt_select` needs the chosen option to still
/// carry its email back out — a `Vec<String>` of rendered labels can't do that round trip.
enum AssigneeChoice {
    Known { email: String, name: String, is_current: bool },
    NewEmail,
}

impl fmt::Display for AssigneeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssigneeChoice::Known { email, name, is_current } => {
                write!(f, "{name} <{email}>{}", if *is_current { " (current)" } else { "" })
            }
            AssigneeChoice::NewEmail => write!(f, "+ enter a new email"),
        }
    }
}

/// Assignee needs a real email (`identity::validate_email`), not free text, so it gets its own
/// prompt instead of `ask_optional_text`: an arrow-key menu of every email
/// `identity::contributor_directory` already knows about, cursor starting on the current
/// assignee (if any), plus a "+ enter a new email" entry for someone not in it yet.
fn ask_assignee(repo: &Repository, current: Option<&str>) -> Result<Option<String>> {
    let contributors = identity::sorted_contributors(repo)?;
    if contributors.is_empty() {
        return ask_assignee_new_email(current);
    }

    let mut options: Vec<AssigneeChoice> = contributors
        .iter()
        .map(|(email, name)| AssigneeChoice::Known {
            email: email.clone(),
            name: name.clone(),
            is_current: Some(email.as_str()) == current,
        })
        .collect();
    let new_email_idx = options.len();
    options.push(AssigneeChoice::NewEmail);

    let default_idx =
        contributors.iter().position(|(email, _)| Some(email.as_str()) == current).unwrap_or(new_email_idx);

    let label = match current {
        Some(cur) => format!("Assignee (current: {cur})"),
        None => "Assignee (current: none)".to_string(),
    };
    match ui::prompt_select(&label, options, default_idx)? {
        AssigneeChoice::Known { email, .. } => Ok(if Some(email.as_str()) == current { None } else { Some(email) }),
        AssigneeChoice::NewEmail => ask_assignee_new_email(current),
    }
}

/// Free-text fallback for an email not in the contributor list — loops until a valid email is
/// entered, same as the picker it's reached from, blank keeps the current assignee untouched.
fn ask_assignee_new_email(current: Option<&str>) -> Result<Option<String>> {
    loop {
        let raw = ui::prompt_text("  email", "", Some("enter to cancel"))?;
        if raw.is_empty() {
            return Ok(None);
        }
        match identity::validate_email(&raw) {
            Ok(email) => return Ok(if Some(email.as_str()) == current { None } else { Some(email) }),
            Err(err) => println!("{err:#}"),
        }
    }
}

/// Due date gets a "keep current / pick a date" menu instead of free text: picking a date opens
/// `ui::prompt_date`'s arrow-key calendar, stored as `YYYY-MM-DD`. An unparseable existing value
/// (from back when this field took free text) is shown as-is in the "keep current" label but
/// otherwise ignored — the calendar always opens on today, it never tries to preselect it.
fn select_due_date(current: Option<&str>) -> Result<Option<String>> {
    let keep_label = match current {
        Some(cur) if !cur.is_empty() => format!("Keep current ({cur})"),
        _ => "Keep current (not set)".to_string(),
    };
    let pick_label = "Pick a date...".to_string();
    let options = vec![keep_label.clone(), pick_label];
    let choice = ui::prompt_select("Due date", options, 0)?;
    if choice == keep_label {
        return Ok(None);
    }
    let picked = ui::prompt_date("  due date")?.format("%Y-%m-%d").to_string();
    Ok(if Some(picked.as_str()) == current { None } else { Some(picked) })
}

/// Same as `ask_text` but for the `Milestone` field, which has no "current" value to default to
/// until it's first set.
fn ask_optional_text(label: &str, current: Option<&str>) -> Result<Option<String>> {
    match current {
        Some(cur) if !cur.is_empty() => {
            let answer = ui::prompt_text(label, cur, None)?;
            Ok(if answer == cur { None } else { Some(answer) })
        }
        _ => {
            let answer = ui::prompt_text(label, "", Some("enter to skip"))?;
            Ok(if answer.is_empty() { None } else { Some(answer) })
        }
    }
}
