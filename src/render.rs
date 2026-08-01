use time::macros::format_description;
use time::OffsetDateTime;

use crate::color::{self, Semantic};
use crate::domain::id;
use crate::domain::op::Operation;
use crate::domain::task::Task;
use crate::wrap;

const BOX_INDENT: usize = 2;
const BOX_LABEL_WIDTH: usize = 9;
const BOX_COL_GAP: usize = 2;
const BOX_HALF_COL: usize = 34;

/// One piece of a box row: the plain text (used to compute how much padding the row needs so
/// the right border lines up) paired with its already-ANSI-wrapped form. Colors are applied
/// once, when a `Seg` is built, and never touched again — so padding math only ever measures
/// plain strings and never has to strip escape codes back out of colored ones.
struct Seg {
    plain: String,
    colored: String,
}

fn plain_seg(text: &str) -> Seg {
    Seg { colored: text.to_string(), plain: text.to_string() }
}

fn bold_seg(text: &str) -> Seg {
    Seg { colored: color::bold(text), plain: text.to_string() }
}

fn dim_seg(text: &str) -> Seg {
    Seg { colored: color::dim(text), plain: text.to_string() }
}

fn label_seg(text: &str) -> Seg {
    dim_seg(text)
}

fn spaces_seg(n: usize) -> Seg {
    let s = " ".repeat(n);
    Seg { colored: s.clone(), plain: s }
}

/// `[● TODO]` / `[TASK]` style badge, colored as a whole (brackets included) via the same
/// `status_semantic`/`kind_semantic`/`priority_semantic` classification used everywhere else
/// in the app (ls's table, the banner's open/in-progress counts) — not a new palette.
fn badge_seg(text: &str, sem: Semantic, bullet: bool) -> Seg {
    let plain = if bullet { format!("[● {text}]") } else { format!("[{text}]") };
    Seg { colored: color::paint(sem, &plain), plain }
}

fn box_border(s: &str) -> String {
    color::dim(s)
}

fn boxed_row(segs: &[Seg], width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    let plain_len: usize = segs.iter().map(|s| s.plain.chars().count()).sum();
    let pad = inner_width.saturating_sub(plain_len);
    let content: String = segs.iter().map(|s| s.colored.as_str()).collect();
    format!("{}{content}{}{}", box_border("│"), " ".repeat(pad), box_border("│"))
}

fn boxed_blank(width: usize) -> String {
    boxed_row(&[], width)
}

/// `╭── TITLE ──────...──╮` (or `├─…─┤` mid-box, `╰─…─╯` for the close, when `left`/`right`
/// are the matching corner/tee characters) with the title itself sized off its plain text so
/// the dash count comes out right regardless of the heading color codes wrapped around it.
fn boxed_titled_border(left: &str, right: &str, title: Option<&str>, width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    match title {
        Some(title) => {
            let head = format!("── {title} ");
            let dashes = inner_width.saturating_sub(head.chars().count());
            format!(
                "{}{}{}{}{}{}",
                box_border(left),
                box_border("── "),
                color::heading(title),
                box_border(" "),
                box_border(&"─".repeat(dashes)),
                box_border(right),
            )
        }
        None => format!("{}{}{}", box_border(left), box_border(&"─".repeat(inner_width)), box_border(right)),
    }
}

/// A label:value row, e.g. `  ID        SRV-1f2dce54`, label left-padded to a fixed column so
/// every single-field row in the card lines up regardless of label length.
fn field_row(label_text: &str, value: Seg, width: usize) -> String {
    let gap = BOX_LABEL_WIDTH.saturating_sub(label_text.chars().count()) + BOX_COL_GAP;
    boxed_row(&[spaces_seg(BOX_INDENT), label_seg(label_text), spaces_seg(gap), value], width)
}

/// Two label:value pairs on one row (e.g. `Status`/`Kind`, `Author`/`Created`) — the first
/// pair is padded out to `BOX_HALF_COL` so the second pair's label always starts at the same
/// column no matter how long the first value is.
fn field_row2(label1: &str, value1: Seg, label2: &str, value2: Seg, width: usize) -> String {
    let gap1 = BOX_LABEL_WIDTH.saturating_sub(label1.chars().count()) + BOX_COL_GAP;
    let left_len = BOX_INDENT + label1.chars().count() + gap1 + value1.plain.chars().count();
    let mid_pad = BOX_HALF_COL.saturating_sub(left_len);
    let gap2 = BOX_LABEL_WIDTH.saturating_sub(label2.chars().count()) + BOX_COL_GAP;
    boxed_row(
        &[
            spaces_seg(BOX_INDENT),
            label_seg(label1),
            spaces_seg(gap1),
            value1,
            spaces_seg(mid_pad),
            label_seg(label2),
            spaces_seg(gap2),
            value2,
        ],
        width,
    )
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

fn cyan_seg(text: &str) -> Seg {
    Seg { colored: color::cyan(text), plain: text.to_string() }
}

fn text_row(text: &str, width: usize) -> String {
    boxed_row(&[spaces_seg(BOX_INDENT), plain_seg(text)], width)
}

/// Max plain-text length for a wrapped line indented by `indent` spaces inside the box, so it
/// never fills the row exactly flush to the right border — leaves at least one column of
/// breathing room before the closing `│`, matching the left-hand indent's margin.
fn wrap_width_for(indent: usize, width: usize) -> usize {
    width.saturating_sub(2 + indent + 1)
}

pub fn to_text(task: &Task, key: &str) -> String {
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
    line(boxed_blank(width));

    line(field_row2(
        "Status",
        badge_seg(&task.status.to_ascii_uppercase(), color::status_semantic(&task.status), true),
        "Kind",
        badge_seg(&format!("{:?}", task.kind).to_ascii_uppercase(), color::kind_semantic(task.kind), false),
        width,
    ));

    if task.priority.is_some() || task.assignee.is_some() {
        let priority_val = match &task.priority {
            Some(p) => badge_seg(&p.to_ascii_uppercase(), color::priority_semantic(p), false),
            None => plain_seg("-"),
        };
        let assignee_val = task.assignee.as_deref().map(plain_seg).unwrap_or_else(|| plain_seg("-"));
        line(field_row2("Priority", priority_val, "Assignee", assignee_val, width));
    }

    if task.due.is_some() || task.milestone.is_some() {
        let due_val = task.due.as_deref().map(plain_seg).unwrap_or_else(|| plain_seg("-"));
        let milestone_val = task.milestone.as_deref().map(plain_seg).unwrap_or_else(|| plain_seg("-"));
        line(field_row2("Due", due_val, "Milestone", milestone_val, width));
    }

    if !task.labels.is_empty() {
        line(field_row("Labels", plain_seg(&join_labels(task)), width));
    }
    if let Some(p) = &task.parent {
        line(field_row("Parent", plain_seg(&id::display(key, p)), width));
    }
    if !task.links.is_empty() {
        line(field_row("Links", plain_seg(&join_links(task, key)), width));
    }

    line(field_row2("Author", plain_seg(&task.reporter.name), "Created", dim_seg(&fmt_ts(task.created)), width));
    line(field_row("Updated", dim_seg(&fmt_ts(task.updated)), width));
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
