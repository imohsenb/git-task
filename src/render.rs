use time::macros::format_description;
use time::OffsetDateTime;

use crate::domain::op::Operation;
use crate::domain::task::Task;

const TS_FORMAT: &[time::format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]");

fn fmt_ts(ts: i64) -> String {
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => dt.format(TS_FORMAT).unwrap_or_else(|_| ts.to_string()),
        Err(_) => ts.to_string(),
    }
}

pub fn to_text(task: &Task, display_id: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("ID        {display_id}\n"));
    out.push_str(&format!("Title     {}\n", task.title));
    out.push_str(&format!("Kind      {:?}\n", task.kind));
    out.push_str(&format!("Status    {}\n", task.status));
    if let Some(p) = &task.priority {
        out.push_str(&format!("Priority  {p}\n"));
    }
    if let Some(a) = &task.assignee {
        out.push_str(&format!("Assignee  {a}\n"));
    }
    if !task.labels.is_empty() {
        out.push_str(&format!("Labels    {}\n", join_labels(task)));
    }
    if let Some(d) = &task.due {
        out.push_str(&format!("Due       {d}\n"));
    }
    out.push_str(&format!("Created   {} by {}\n", fmt_ts(task.created), task.reporter.name));
    out.push_str(&format!("Updated   {}\n", fmt_ts(task.updated)));

    if !task.description.is_empty() {
        out.push_str(&format!("\nDescription\n  {}\n", task.description));
    }

    if !task.comments.is_empty() {
        out.push_str(&format!("\nComments ({})\n", task.comments.len()));
        for c in &task.comments {
            let edited = if c.edited { " (edited)" } else { "" };
            out.push_str(&format!(
                "  #{} {} ({}){}\n     {}\n",
                c.id, c.author.name, fmt_ts(c.timestamp), edited, c.text
            ));
        }
    }

    out
}

pub fn to_markdown(task: &Task, display_id: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", task.title));
    out.push_str(&format!("- **ID:** {display_id}\n"));
    out.push_str(&format!("- **Kind:** {:?}\n", task.kind));
    out.push_str(&format!("- **Status:** {}\n", task.status));
    if let Some(p) = &task.priority {
        out.push_str(&format!("- **Priority:** {p}\n"));
    }
    if let Some(a) = &task.assignee {
        out.push_str(&format!("- **Assignee:** {a}\n"));
    }
    if !task.labels.is_empty() {
        out.push_str(&format!("- **Labels:** {}\n", join_labels(task)));
    }
    if let Some(d) = &task.due {
        out.push_str(&format!("- **Due:** {d}\n"));
    }
    out.push_str(&format!(
        "- **Created:** {} by {}\n",
        fmt_ts(task.created),
        task.reporter.name
    ));
    out.push_str(&format!("- **Updated:** {}\n", fmt_ts(task.updated)));

    if !task.description.is_empty() {
        out.push_str(&format!("\n## Description\n\n{}\n", task.description));
    }

    if !task.comments.is_empty() {
        out.push_str("\n## Comments\n");
        for c in &task.comments {
            let edited = if c.edited { " (edited)" } else { "" };
            out.push_str(&format!(
                "\n### #{} — {} ({}){}\n\n{}\n",
                c.id, c.author.name, fmt_ts(c.timestamp), edited, c.text
            ));
        }
    }

    out
}

pub fn to_log(task: &Task, display_id: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("task {display_id} — {}\n", task.title));
    for env in &task.history {
        out.push_str(&format!(
            "{} {} {}\n",
            fmt_ts(env.timestamp),
            env.author.name,
            op_line(&env.op)
        ));
    }
    out
}

fn join_labels(task: &Task) -> String {
    task.labels.iter().cloned().collect::<Vec<_>>().join(", ")
}

fn op_line(op: &Operation) -> String {
    match op {
        Operation::CreateTask { title, kind, .. } => format!("created {kind:?} \"{title}\""),
        Operation::SetTitle { title } => format!("set title to \"{title}\""),
        Operation::SetDescription { .. } => "updated description".to_string(),
        Operation::SetKind { kind } => format!("set kind to {kind:?}"),
        Operation::SetStatus { status } => format!("set status to {status}"),
        Operation::SetPriority { priority } => format!("set priority to {priority}"),
        Operation::SetAssignee { assignee } => format!("assigned to {assignee}"),
        Operation::AddLabel { label } => format!("added label {label}"),
        Operation::RemoveLabel { label } => format!("removed label {label}"),
        Operation::AddComment { .. } => "added a comment".to_string(),
        Operation::EditComment { comment_id, .. } => format!("edited comment #{comment_id}"),
        Operation::SetDueDate { due } => format!("set due date to {due}"),
    }
}
