use crate::color;
use crate::domain::op::TaskKind;
use crate::hints;

enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    /// The icon+label prefix for this level, already colored — the only thing that actually
    /// differs between `info`/`warn`/`error` once you strip away the message/detail/tips
    /// skeleton they all share.
    fn prefix(&self) -> String {
        match self {
            Level::Info => color::paint(color::Semantic::Info, color::semantic_icon(color::Semantic::Info)),
            Level::Warning => color::bold_yellow("▲ Warning:"),
            Level::Error => color::bold_red("✖ Error:"),
        }
    }

    /// Warnings and errors are exceptional — stderr, so they survive `2>/dev/null` and don't
    /// pollute output piped elsewhere. Info is the normal-operation channel: stdout.
    fn to_stderr(&self) -> bool {
        matches!(self, Level::Warning | Level::Error)
    }
}

/// The one shape every terminal message in this CLI uses outside of `render`'s task detail
/// views and `table`'s list boxes: an icon-prefixed line, an optional dim `└─ <detail>`
/// sub-line (callers embed their own label — `"Cause: ..."`, `"Details: ..."`, or nothing —
/// this stays agnostic to that vocabulary), and the existing `hints::print` "Tips:" block.
/// Three levels (`info`/`warn`/`error`) cover everything that needs an icon and a stream
/// choice; `plain` is the escape hatch for incidental follow-up lines (a shell snippet, a
/// dimmed aside) that want none of that — just a line, still funneled through one module so
/// it's obvious that's a deliberate choice and not a stray `println!`.
/// Colors only ever come from `color`'s existing palette — no raw ANSI/RGB literals here.
pub struct Logger;

impl Logger {
    fn log(level: Level, message: &str, detail: Option<&str>, tips: &[(String, String)]) {
        let line = format!("{} {message}", level.prefix());
        let detail_line = detail.map(|d| color::dim(&format!("└─ {d}")));
        if level.to_stderr() {
            eprintln!("{line}");
            if let Some(d) = &detail_line {
                eprintln!("{d}");
            }
        } else {
            println!("{line}");
            if let Some(d) = &detail_line {
                println!("{d}");
            }
        }
        hints::print(tips);
    }

    pub fn info(message: &str, detail: Option<&str>, tips: &[(String, String)]) {
        Self::log(Level::Info, message, detail, tips);
    }

    pub fn warn(message: &str, detail: Option<&str>, tips: &[(String, String)]) {
        Self::log(Level::Warning, message, detail, tips);
    }

    pub fn error(message: &str, detail: Option<&str>, tips: &[(String, String)]) {
        Self::log(Level::Error, message, detail, tips);
    }

    /// No icon, no color, no stream logic — just a line. For incidental follow-ups (a copy-paste
    /// shell snippet, a dimmed aside) that shouldn't carry a severity of their own.
    pub fn plain(message: &str) {
        println!("{message}");
    }
}

/// `#<id> [<KIND>] "<title>"` — the task-identifying fragment every mutation's `Logger::info`
/// message embeds, with the kind badge colored via the same palette `render`/`style` use for
/// it. A plain function, not a method on `Logger`, because it's string formatting, not another
/// message type: compose it into `message` with an ordinary `format!`.
pub fn task_ref(id: &str, kind: TaskKind, title: &str) -> String {
    let badge = color::paint(color::kind_semantic(kind), &format!("[{}]", kind.as_str().to_ascii_uppercase()));
    format!("{} {badge} \"{title}\"", color::cyan(&format!("#{id}")))
}
