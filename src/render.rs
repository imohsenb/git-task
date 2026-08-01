use comfy_table::{Attribute, Cell, Color, ContentArrangement};
use time::macros::format_description;
use time::OffsetDateTime;

use crate::color::{self, Semantic};
use crate::domain::id;
use crate::domain::op::Operation;
use crate::domain::task::Task;
use crate::table;
use crate::wrap;

/// Maps our own semantic classification to a comfy-table `Color` for `Cell::fg` — comfy-table
/// measures cell width from the plain string and applies its own escape codes afterward, so
/// styling has to go through this (like `ls`/`repos`/`projects` already do) rather than baking
/// raw ANSI into the cell text with `color::paint`, which would throw off the box's column
/// widths and border alignment.
fn semantic_color(sem: Semantic) -> Color {
    match sem {
        Semantic::Success => Color::Green,
        Semantic::Warn => Color::Yellow,
        Semantic::Danger => Color::Red,
        Semantic::Info => Color::Cyan,
        Semantic::Neutral => Color::Reset,
    }
}

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

/// One metadata field: label, plain (unstyled) value, and an optional comfy-table color for
/// the value cell.
type Field = (&'static str, String, Option<Color>);

/// Renders the metadata fields as a small bordered card via the same `table::new()` style
/// used by `ls`/`repos`/`projects` (rounded UTF8 borders, no per-row divider) — reuses
/// existing, already-compiled rendering machinery rather than adding anything new, and stays
/// just as fast: laying out a dozen rows is microseconds. `ContentArrangement::Dynamic` wraps
/// a long value (e.g. `Title`) to the terminal width instead of stretching the box off-screen.
fn field_card(fields: &[Field]) -> String {
    let mut t = table::new();
    t.set_content_arrangement(ContentArrangement::Dynamic);
    for (label_text, value, fg) in fields {
        let mut value_cell = Cell::new(value);
        if let Some(c) = fg {
            value_cell = value_cell.fg(*c);
        }
        t.add_row(vec![Cell::new(*label_text).add_attribute(Attribute::Bold), value_cell]);
    }
    t.to_string()
}

pub fn to_text(task: &Task, key: &str) -> String {
    let mut fields: Vec<Field> = vec![
        ("ID", id::display(key, &task.id), Some(Color::Cyan)),
        ("Title", task.title.clone(), None),
        ("Kind", format!("{:?}", task.kind), Some(semantic_color(color::kind_semantic(task.kind)))),
        ("Status", task.status.clone(), Some(semantic_color(color::status_semantic(&task.status)))),
    ];
    if let Some(p) = &task.priority {
        fields.push(("Priority", p.clone(), Some(semantic_color(color::priority_semantic(p)))));
    }
    if let Some(a) = &task.assignee {
        fields.push(("Assignee", a.clone(), None));
    }
    if !task.labels.is_empty() {
        fields.push(("Labels", join_labels(task), None));
    }
    if let Some(d) = &task.due {
        fields.push(("Due", d.clone(), None));
    }
    if let Some(m) = &task.milestone {
        fields.push(("Milestone", m.clone(), None));
    }
    if let Some(p) = &task.parent {
        fields.push(("Parent", id::display(key, p), None));
    }
    if !task.links.is_empty() {
        fields.push(("Links", join_links(task, key), None));
    }
    fields.push(("Created", format!("{} by {}", fmt_ts(task.created), task.reporter.name), None));
    fields.push(("Updated", fmt_ts(task.updated), None));

    let mut out = field_card(&fields);
    out.push('\n');

    if !task.description.is_empty() {
        out.push_str(&format!("\n{}\n", color::heading("Description")));
        let width = wrap::terminal_width().saturating_sub(2);
        for line in wrap::wrap(&task.description, width) {
            out.push_str(&format!("  {line}\n"));
        }
    }

    if !task.comments.is_empty() {
        out.push_str(&format!("\n{} ({})\n", color::heading("Comments"), task.comments.len()));
        let width = wrap::terminal_width().saturating_sub(5);
        for c in &task.comments {
            let edited = if c.edited { " (edited)" } else { "" };
            out.push_str(&format!(
                "  #{} {} ({}){}\n",
                c.id,
                color::bold(&c.author.name),
                color::dim(&fmt_ts(c.timestamp)),
                edited
            ));
            for line in wrap::wrap(&c.text, width) {
                out.push_str(&format!("     {line}\n"));
            }
        }
    }

    out
}

pub fn to_markdown(task: &Task, key: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", task.title));
    out.push_str(&format!("- **ID:** {}\n", id::display(key, &task.id)));
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
        Operation::SetPriority { priority } => format!("set priority to {priority}"),
        Operation::SetAssignee { assignee } => format!("assigned to {assignee}"),
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
