use crate::color;
use crate::table::{Seg};
use crate::domain::task::Task;
use crate::wrap;

pub fn status(task: &Task) -> Seg {
    let status_sem = color::status_semantic(&task.status);
    let status_clean = wrap::sanitize(&task.status);
    let status_plain = format!("{} {}", color::semantic_icon(status_sem), status_clean.to_ascii_uppercase());
    Seg { colored: color::paint(status_sem, &status_plain), plain: status_plain }
}

pub fn priority(task: &Task) -> Seg {
    let priority_seg = match task.priority {
        Some(p) => {
            let text = format!("{} {}", color::priority_icon(p), p.as_str().to_ascii_uppercase());
            Seg { colored: color::paint(color::priority_semantic(p), &text), plain: text }
        }
        None => Seg { colored: String::new(), plain: String::new() },
    };
    priority_seg
}

pub fn kind(task: &Task) -> Seg {
    let kind_plain = format!("{:?}", task.kind).to_ascii_uppercase();
    Seg { colored: color::paint(color::kind_semantic(task.kind), &kind_plain), plain: kind_plain }
}