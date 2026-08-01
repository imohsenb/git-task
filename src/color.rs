use std::io::IsTerminal;
use std::sync::OnceLock;

use crate::domain::op::TaskKind;


const SLATE: (u8, u8, u8) = (100, 116, 139);
const MUTED_ASH: (u8, u8, u8) = (156, 163, 175);

/// Whether ANSI styling should be emitted at all. Cached after the first check —
/// stdout doesn't change from a terminal to a pipe mid-process.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::io::stdout().is_terminal())
}

fn wrap(code: &str, s: &str) -> String {
    if enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    wrap("1", s)
}

pub fn red(s: &str) -> String {
    wrap("31", s)
}

pub fn green(s: &str) -> String {
    wrap("32", s)
}

pub fn yellow(s: &str) -> String {
    wrap("33", s)
}

pub fn dim(s: &str) -> String {
    let (r, g, b) = SLATE;
    wrap(&format!("38;2;{r};{g};{b}"), s)
}

pub fn dim_bold(s: &str) -> String {
    let (r, g, b) = SLATE;
    wrap(&format!("1;38;2;{r};{g};{b}"), s)
}

pub fn light(s: &str) -> String {
    let (r, g, b) = MUTED_ASH;
    wrap(&format!("38;2;{r};{g};{b}"), s)
}

pub fn light_bold(s: &str) -> String {
    let (r, g, b) = MUTED_ASH;
    wrap(&format!("1;38;2;{r};{g};{b}"), s)
}

/// Sky-blue accent (`#38bdf8`) used for IDs and anything else classified `Semantic::Info` —
/// the single source of truth so `cyan()` here and the `comfy_table::Color::Rgb` copies in
/// `ls`/`repos` (which need their own color type, not a raw ANSI string) can't drift apart.
pub const CYAN_RGB: (u8, u8, u8) = (56, 189, 248);

pub fn cyan(s: &str) -> String {
    let (r, g, b) = CYAN_RGB;
    wrap(&format!("38;2;{r};{g};{b}"), s)
}

pub fn magenta(s: &str) -> String {
    wrap("35", s)
}

pub fn bold_red(s: &str) -> String {
    wrap("1;31", s)
}

/// Section headings (help text, etc.) — bold plus the banner's accent yellow.
pub fn heading(s: &str) -> String {
    wrap("1", s)
}


/// The small palette task/priority/kind values get painted with. Kept separate from
/// the raw ANSI helpers above so callers rendering into a `comfy_table` (which needs
/// its own `Color` enum, not raw escape codes, or column widths miscompute) can map
/// the same classification to a table cell color instead of a painted string.
#[derive(Clone, Copy)]
pub enum Semantic {
    Success,
    Warn,
    Danger,
    Info,
    Neutral,
}

/// Classifies a free-form status string. There's no fixed status vocabulary in this
/// codebase (`domain/task.rs::DEFAULT_STATUS` is just `"todo"`, anything else is
/// whatever the user typed) — unrecognized values fall back to `Neutral`, i.e. plain.
pub fn status_semantic(s: &str) -> Semantic {
    match s.to_ascii_lowercase().as_str() {
        "done" | "closed" | "resolved" | "completed" | "complete" => Semantic::Success,
        "doing" | "in-progress" | "in_progress" | "started" | "wip" | "review" | "in review" => Semantic::Warn,
        "blocked" | "stuck" => Semantic::Danger,
        "todo" | "open" | "backlog" | "new" | "planned" => Semantic::Info,
        _ => Semantic::Neutral,
    }
}

/// Classifies a free-form priority string; same "no fixed vocabulary" caveat as
/// `status_semantic`.
pub fn priority_semantic(s: &str) -> Semantic {
    match s.to_ascii_lowercase().as_str() {
        "critical" | "urgent" | "high" => Semantic::Danger,
        "medium" | "normal" => Semantic::Warn,
        "low" => Semantic::Success,
        _ => Semantic::Neutral,
    }
}

pub fn kind_semantic(k: TaskKind) -> Semantic {
    match k {
        TaskKind::Bug => Semantic::Danger,
        TaskKind::Epic => Semantic::Info,
        TaskKind::Story => Semantic::Info,
        TaskKind::Task | TaskKind::Subtask => Semantic::Neutral,
    }
}

pub fn paint(sem: Semantic, s: &str) -> String {
    match sem {
        Semantic::Success => green(s),
        Semantic::Warn => yellow(s),
        Semantic::Danger => red(s),
        Semantic::Info => cyan(s),
        Semantic::Neutral => s.to_string(),
    }
}
