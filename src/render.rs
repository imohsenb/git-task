use std::collections::HashMap;

use time::macros::format_description;
use time::OffsetDateTime;

use crate::color;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::domain::task::Task;
use crate::identity;
use crate::table::{
    bold_seg, boxed_blank, boxed_row, boxed_titled_border, dim_seg, field_row, field_row2, plain_seg,
    spaces_seg, text_row, wrap_width_for, Seg, BOX_INDENT,
};
use crate::wrap;
use crate::style;

const TS_FORMAT: &[time::format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]");

fn fmt_ts(ts: i64) -> String {
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => dt.format(TS_FORMAT).unwrap_or_else(|_| ts.to_string()),
        Err(_) => ts.to_string(),
    }
}

fn join_links(task: &Task, key: &str) -> String {
    task.links
        .iter()
        .map(|l| format!("{:?} {}", l.kind, id::display(key, &l.target)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn cyan_seg(text: &str) -> Seg {
    Seg { colored: color::cyan(text), plain: text.to_string() }
}

pub fn to_text(task: &Task, key: &str, directory: &HashMap<String, String>) -> String {
    let width = wrap::terminal_width();
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };

    line(boxed_titled_border("╭", "╮", Some("TASK DETAILS"), width));
    line(boxed_blank(width));

    line(field_row("ID", cyan_seg(&id::display(key, &task.id)), width));
    line(field_row("Title", bold_seg(&task.title), width));

    if !task.labels.is_empty() {
        line(field_row("Labels", dim_seg(&join_labels(task)), width));
    }

    line(boxed_blank(width));

    line(field_row2(
        "Kind",
        style::kind(&task),
        "Author", 
        plain_seg(&task.reporter.name),
        width,
    ));

    let assignee_val = task
            .assignee
            .as_deref()
            .map(|email| plain_seg(&identity::full_display(directory, email)))
            .unwrap_or_else(|| plain_seg("-"));

    line(field_row2(
        "Status",
        style::status(task),
        "Assignee", 
        assignee_val,
        width,
    ));

    if task.priority.is_some() {
        let priority_val = style::priority(&task);
        
        line(field_row("Priority", priority_val, width));
    }

    line(boxed_blank(width));

    if task.due.is_some() || task.milestone.is_some() {
        let due_val = task.due.as_deref().map(plain_seg).unwrap_or_else(|| plain_seg("-"));
        let milestone_val = task.milestone.as_deref().map(plain_seg).unwrap_or_else(|| plain_seg("-"));
        line(field_row2("Due", due_val, "Milestone", milestone_val, width));
    }

    
    if let Some(p) = &task.parent {
        line(field_row("Parent", plain_seg(&id::display(key, p)), width));
    }
    if !task.links.is_empty() {
        line(field_row("Links", plain_seg(&join_links(task, key)), width));
    }

    line(field_row2("Created", dim_seg(&fmt_ts(task.created)), "Updated", dim_seg(&fmt_ts(task.updated)), width));
    line(boxed_blank(width));

    if !task.description.is_empty() {
        line(boxed_titled_border("├", "┤", Some("DESCRIPTION"), width));
        line(boxed_blank(width));
        let desc_width = wrap_width_for(BOX_INDENT, width);
        for wrapped in wrap::wrap(&task.description, desc_width) {
            line(text_row(&wrapped, width));
        }
        line(boxed_blank(width));
    }

    if !task.comments.is_empty() {
        line(boxed_titled_border("├", "┤", Some(&format!("COMMENTS ({})", task.comments.len())), width));
        line(boxed_blank(width));
        let body_width = wrap_width_for(BOX_INDENT + 3, width);
        for (i, c) in task.comments.iter().enumerate() {
            let edited = if c.edited { " (edited)" } else { "" };
            line(boxed_row(
                &[
                    spaces_seg(BOX_INDENT),
                    plain_seg(&format!("#{} ", c.id)),
                    bold_seg(&c.author.name),
                    plain_seg(" ("),
                    dim_seg(&fmt_ts(c.timestamp)),
                    plain_seg(&format!("){edited}")),
                ],
                width,
            ));
            for wrapped in wrap::wrap(&c.text, body_width) {
                line(boxed_row(&[spaces_seg(BOX_INDENT + 3), plain_seg(&wrapped)], width));
            }
            if i + 1 < task.comments.len() {
                line(boxed_blank(width));
            }
        }
        line(boxed_blank(width));
    }

    line(boxed_titled_border("╰", "╯", None, width));

    out
}

pub fn to_markdown(task: &Task, key: &str, directory: &HashMap<String, String>) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", task.title));
    out.push_str(&format!("- **ID:** {}\n", id::display(key, &task.id)));
    out.push_str(&format!("- **Kind:** {:?}\n", task.kind));
    out.push_str(&format!("- **Status:** {}\n", task.status));
    if let Some(p) = task.priority {
        out.push_str(&format!("- **Priority:** {}\n", p.as_str()));
    }
    if let Some(email) = &task.assignee {
        out.push_str(&format!("- **Assignee:** {}\n", identity::full_display(directory, email)));
    }
    if !task.labels.is_empty() {
        out.push_str(&format!("- **Labels:** {}\n", join_labels(task)));
    }
    if let Some(d) = &task.due {
        out.push_str(&format!("- **Due:** {d}\n"));
    }
    if let Some(m) = &task.milestone {
        out.push_str(&format!("- **Milestone:** {m}\n"));
    }
    if let Some(p) = &task.parent {
        out.push_str(&format!("- **Parent:** {}\n", id::display(key, p)));
    }
    if !task.links.is_empty() {
        out.push_str(&format!("- **Links:** {}\n", join_links(task, key)));
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

pub fn to_log(task: &Task, key: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} — {}\n",
        color::bold("task"),
        color::cyan(&id::display(key, &task.id)),
        task.title
    ));
    for env in &task.history {
        out.push_str(&format!(
            "{} {} {}\n",
            color::dim(&fmt_ts(env.timestamp)),
            color::bold(&env.author.name),
            op_line(&env.op, key)
        ));
    }
    out
}

fn join_labels(task: &Task) -> String {
    task.labels.iter().cloned().collect::<Vec<_>>().join(", ")
}

fn op_line(op: &Operation, key: &str) -> String {
    match op {
        Operation::CreateTask { title, kind, .. } => format!("created {kind:?} \"{title}\""),
        Operation::SetTitle { title } => format!("set title to \"{title}\""),
        Operation::SetDescription { .. } => "updated description".to_string(),
        Operation::SetKind { kind } => format!("set kind to {kind:?}"),
        Operation::SetStatus { status } => format!("set status to {status}"),
        Operation::SetPriority { priority } => format!("set priority to {}", priority.as_str()),
        Operation::SetAssignee { email } => format!("assigned to {email}"),
        Operation::AddLabel { label } => format!("added label {label}"),
        Operation::RemoveLabel { label } => format!("removed label {label}"),
        Operation::AddComment { .. } => "added a comment".to_string(),
        Operation::EditComment { comment_id, .. } => format!("edited comment #{comment_id}"),
        Operation::SetDueDate { due } => format!("set due date to {due}"),
        Operation::SetParent { parent } => format!("set parent to {}", id::display(key, parent)),
        Operation::ClearParent => "cleared parent".to_string(),
        Operation::SetMilestone { milestone } => format!("set milestone to {milestone}"),
        Operation::AddLink { kind, target } => {
            format!("added {kind:?} link to {}", id::display(key, target))
        }
        Operation::RemoveLink { kind, target } => {
            format!("removed {kind:?} link to {}", id::display(key, target))
        }
    }
}
