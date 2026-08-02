use anyhow::{bail, Result};
use clap::Args;
use git2::Repository;

use crate::actor::Actor;
use crate::automation;
use crate::cli::wizard;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::{Operation, Priority, TaskKind};
use crate::domain::task::Task;
use crate::git;
use crate::identity;
use crate::prompt;
use crate::store::git_store::Store;

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
        interactive_ops(&repo, &task)?
    } else {
        bail!(
            "nothing to edit — pass at least one of --title, --desc, --kind, --priority, --assignee, --due, --milestone"
        );
    };

    if ops.is_empty() {
        println!("no changes.");
        return Ok(());
    }

    store.append(&task_id, &author, ops.clone())?;
    automation::engine::run(&repo, &task_id, &ops)?;
    let key = project::effective_key_for(&repo)?;
    println!("updated {}", id::display(&key, &task_id));
    Ok(())
}

/// Walks every editable field, showing the current value as the default so pressing enter
/// leaves it untouched. Status and kind get a numbered menu instead of free text since both
/// are small closed-ish vocabularies (`color::status_semantic` documents the same status
/// presets used here) — picking a number beats retyping the exact spelling.
fn interactive_ops(repo: &Repository, task: &Task) -> Result<Vec<Operation>> {
    println!("editing {} — enter keeps the current value", task.title);

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
    if let Some(due) = ask_optional_text("Due date", task.due.as_deref())? {
        ops.push(Operation::SetDueDate { due });
    }
    if let Some(milestone) = ask_optional_text("Milestone", task.milestone.as_deref())? {
        ops.push(Operation::SetMilestone { milestone });
    }
    Ok(ops)
}

const STATUS_PRESETS: &[&str] = &["todo", "doing", "blocked", "done"];

fn select_status(current: &str) -> Result<Option<String>> {
    let mut options: Vec<&str> = STATUS_PRESETS.to_vec();
    if !options.contains(&current) {
        options.push(current);
    }
    options.push("custom...");

    let default_idx = options.iter().position(|o| *o == current).unwrap_or(options.len() - 1);
    let choice = wizard::prompt_choice(&format!("Status (current: {current})"), &options, default_idx)?;
    let chosen = if options[choice] == "custom..." {
        wizard::prompt_default("  custom status", current)?
    } else {
        options[choice].to_string()
    };
    Ok(if chosen == current { None } else { Some(chosen) })
}

const KIND_OPTIONS: &[&str] = &["bug", "story", "task", "epic", "subtask"];

fn select_kind(current: TaskKind) -> Result<Option<TaskKind>> {
    let default_idx = KIND_OPTIONS.iter().position(|o| *o == current.as_str()).unwrap_or(0);
    let choice = wizard::prompt_choice(&format!("Kind (current: {})", current.as_str()), KIND_OPTIONS, default_idx)?;
    let chosen = TaskKind::from_str_loose(KIND_OPTIONS[choice]).expect("prompt_choice returns a valid index into KIND_OPTIONS");
    Ok(if chosen == current { None } else { Some(chosen) })
}

const PRIORITY_OPTIONS: &[&str] = &["low", "medium", "high"];

/// Same shape as `select_kind`: priority is a closed low/medium/high enum, so it gets a
/// numbered menu instead of `ask_optional_text`'s free-text prompt. There's no "clear" choice
/// here — same as before this field became an enum, priority has no `Operation` to unset it
/// once set, so this only ever moves it to a different tier.
fn select_priority(current: Option<Priority>) -> Result<Option<Priority>> {
    let current_str = current.map(|p| p.as_str());
    let default_idx = current_str.and_then(|c| PRIORITY_OPTIONS.iter().position(|o| *o == c)).unwrap_or(1);
    let label = match current_str {
        Some(c) => format!("Priority (current: {c})"),
        None => "Priority (current: none)".to_string(),
    };
    let choice = wizard::prompt_choice(&label, PRIORITY_OPTIONS, default_idx)?;
    let chosen =
        Priority::from_str_loose(PRIORITY_OPTIONS[choice]).expect("prompt_choice returns a valid index into PRIORITY_OPTIONS");
    Ok(if Some(chosen) == current { None } else { Some(chosen) })
}

fn ask_text(label: &str, current: &str) -> Result<Option<String>> {
    let answer = wizard::prompt_default(label, current)?;
    Ok(if answer == current { None } else { Some(answer) })
}

/// Assignee needs a real email (`identity::validate_email`), not free text, so it gets its own
/// prompt instead of `ask_optional_text`: lists every email `identity::contributor_directory`
/// already knows about (the closest thing to autocomplete these line-based prompts can do) as a
/// numbered menu, and still accepts a freshly typed email for someone not in it yet. Blank keeps
/// the current value, same as every other optional field here.
fn ask_assignee(repo: &Repository, current: Option<&str>) -> Result<Option<String>> {
    let contributors = identity::sorted_contributors(repo)?;
    if !contributors.is_empty() {
        println!("known contributors:");
        for (i, (email, name)) in contributors.iter().enumerate() {
            let marker = if Some(email.as_str()) == current { " (current)" } else { "" };
            println!("  {}) {name} <{email}>{marker}", i + 1);
        }
    }

    let label = match current {
        Some(cur) => format!("Assignee [{cur}] (number, email, or enter to keep)"),
        None => "Assignee (none — number, email, or enter to skip)".to_string(),
    };
    loop {
        let raw = wizard::prompt(&label)?;
        if raw.is_empty() {
            return Ok(None);
        }
        let chosen = match raw.parse::<usize>() {
            Ok(n) if n >= 1 && n <= contributors.len() => contributors[n - 1].0.clone(),
            Ok(n) => {
                println!("no contributor #{n} — pick 1-{}, or type an email", contributors.len());
                continue;
            }
            Err(_) => match identity::validate_email(&raw) {
                Ok(email) => email,
                Err(err) => {
                    println!("{err:#}");
                    continue;
                }
            },
        };
        return Ok(if Some(chosen.as_str()) == current { None } else { Some(chosen) });
    }
}

/// Same as `ask_text` but for the `Option<String>` fields (due/milestone),
/// which have no "current" value to default to until they're first set.
fn ask_optional_text(label: &str, current: Option<&str>) -> Result<Option<String>> {
    match current {
        Some(cur) if !cur.is_empty() => {
            let answer = wizard::prompt_default(label, cur)?;
            Ok(if answer == cur { None } else { Some(answer) })
        }
        _ => {
            let answer = wizard::prompt(&format!("{label} (none — enter to skip)"))?;
            Ok(if answer.is_empty() { None } else { Some(answer) })
        }
    }
}
