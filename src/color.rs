use std::io::IsTerminal;
use std::sync::OnceLock;

use crate::domain::op::{Priority, TaskKind};


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

pub fn bold_green(s: &str) -> String {
    wrap("1;32", s)
}

pub fn bold_yellow(s: &str) -> String {
    wrap("1;33", s)
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
    /// A fifth bucket alongside the four severity-ish ones — for values that need to stand out
    /// from `Info` without implying "this is bad" or "watch this" (currently just `Story`, so
    /// its kind badge/table cell doesn't read identically to an `Epic`'s).
    Accent,
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

/// `Priority` is a closed low/medium/high enum (see `domain::op::Priority`), so unlike
/// `status_semantic` this is a straight match, not a best-effort guess over free text.
pub fn priority_semantic(p: Priority) -> Semantic {
    match p {
        Priority::High => Semantic::Danger,
        Priority::Medium => Semantic::Warn,
        Priority::Low => Semantic::Success,
    }
}

pub fn kind_semantic(k: TaskKind) -> Semantic {
    match k {
        TaskKind::Bug => Semantic::Danger,
        TaskKind::Epic => Semantic::Info,
        TaskKind::Story => Semantic::Accent,
        TaskKind::Task | TaskKind::Subtask => Semantic::Neutral,
    }
}

pub fn paint(sem: Semantic, s: &str) -> String {
    match sem {
        Semantic::Success => green(s),
        Semantic::Warn => yellow(s),
        Semantic::Danger => red(s),
        Semantic::Info => cyan(s),
        Semantic::Accent => magenta(s),
        Semantic::Neutral => s.to_string(),
    }
}

/// Glyph for a `Semantic` bucket — used for `STATUS` cells, since status stays free-form text
/// classified through `status_semantic` rather than a fixed enum with its own per-value icon.
pub fn semantic_icon(sem: Semantic) -> &'static str {
    match sem {
        Semantic::Success => "✓",
        Semantic::Warn => "◐",
        Semantic::Danger => "✗",
        Semantic::Info => "○",
        Semantic::Accent => "◆",
        Semantic::Neutral => "●",
    }
}

/// Per-tier glyph for `PRIORITY` cells. `Priority` is a fixed 3-value enum, so — unlike status —
/// it gets one glyph per variant (reading as a level) rather than riding on the generic
/// `semantic_icon`.
pub fn priority_icon(p: Priority) -> &'static str {
    match p {
        Priority::Low => "▼",
        Priority::Medium => "●",
        Priority::High => "▲",
    }
}
