use crate::color;
use crate::config::global::GlobalConfig;
use crate::git;
use crate::store::git_store::Store;

// Vertical bevel, top to bottom: a pale highlight face easing into the light-red base,
// then easing down into a dark-red shadow — the emboss look of chunky 3D block letters,
// done as flat per-row bands instead of true per-pixel shading.
const HIGHLIGHT: (f64, f64, f64) = (255.0, 214.0, 214.0);
const LIGHT_RED: (f64, f64, f64) = (255.0, 107.0, 107.0);
const SHADOW: (f64, f64, f64) = (139.0, 0.0, 0.0);

const PADDING: &str = "   ";

/// The "GIT TASK" wordmark, fixed pixel-for-pixel (including the half-block edge
/// antialiasing) rather than composed from a generic per-letter font — this is the
/// exact block art the banner was asked to use, just recolored per `bevel_color`.
const WORDMARK: [&str; 9] = [
    "███████▌█  ████ ██████████▌█      ██████████▌█ ███████████  ██████████ ████▌  ████",
    "▓▓██       ▓▓██     ▓▓██              ▓▓██     ▓▓██   ▓▓██ ▓▓██   ▓▓██ ▓▓██   ▓▓██",
    "▒▒██       ▒▒█▌     ▒▒██              ▒▒██     ▒▒█▌   ▒▒█▌ ▒▒█▌        ▒▒█▌  ▄▒▒█▌",
    "░░█▌░░░░█▓ ░░█▌     ░░█▌              ░░█▌     ░░▌▓▓▓▌░░█▌  ░▒▓▓▓▓▒█▌  ░░▌▓▓▓▓▀▀▌ ",
    "▀▀▀   ▀▀▀▀ ▀▀▀      ▀▀▀               ▀▀▀      ▀▀▀    ▀▀▀         ▀▀▀  ▀▀▀   ▀▀▀  ",
    "███   ███▌ ███      ███▌              ███▌     ███    ███         ███  ███    ███ ",
    "▓▓█   ▓▓▓  ▓▓█      ▓▓█               ▓▓█      ▓▓█    ▓▓█  ▓▓█    ▓▓█  ▓▓█    ▓▓█ ",
    "▒▒▌   ▒▒▌  ▒▒▌      ▒▒▓               ▒▒▓       ▒▌    ▒▒▌  ▒▒▌    ▒▒▌   ▒▌    ▒▒▌ ",
    "░░░░░░░░   ░░       ░░▌               ░░▌             ░░   ░░░░░░░░           ░░  ",
];

/// Blends two RGB stops at `t` (0.0 = `from`, 1.0 = `to`).
fn lerp(from: (f64, f64, f64), to: (f64, f64, f64), t: f64) -> (u8, u8, u8) {
    (
        (from.0 + (to.0 - from.0) * t).round() as u8,
        (from.1 + (to.1 - from.1) * t).round() as u8,
        (from.2 + (to.2 - from.2) * t).round() as u8,
    )
}

/// The three-stop bevel color for row `t` (0.0 = top, 1.0 = bottom): highlight easing
/// into the light-red base over the first half, then down into shadow over the second.
fn bevel_color(t: f64) -> (u8, u8, u8) {
    if t <= 0.5 {
        lerp(HIGHLIGHT, LIGHT_RED, t * 2.0)
    } else {
        lerp(LIGHT_RED, SHADOW, (t - 0.5) * 2.0)
    }
}

fn solid_line(line: &str, (r, g, b): (u8, u8, u8)) -> String {
    let mut out = String::new();
    for ch in line.chars() {
        if ch == ' ' {
            out.push(' ');
            continue;
        }
        out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{ch}"));
    }
    out.push_str("\x1b[0m");
    out
}

/// The wordmark lines alone (colorized when stdout is a terminal), with no surrounding
/// blank-line spacing — callers add their own margin.
fn art_lines() -> Vec<String> {
    if color::enabled() {
        let rows = WORDMARK.len();
        WORDMARK
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let t = if rows <= 1 { 0.0 } else { i as f64 / (rows - 1) as f64 };
                solid_line(line, bevel_color(t))
            })
            .collect()
    } else {
        WORDMARK.iter().map(|s| s.to_string()).collect()
    }
}

/// Tips for what to do next, tailored to whether the current repo (if any) already
/// has tasks — points at `new` for a first task, `ls` once there's something to list.
/// Current repo's task counts and, if this repo is registered, the project group it's
/// under. `None` for either the whole struct (not in a git repo) or `.project` (in a repo
/// that isn't registered) — both are valid, silent states, not errors.
struct RepoStatus {
    project: Option<String>,
    total: usize,
    open: usize,
    in_progress: usize,
}

fn repo_status() -> Option<RepoStatus> {
    let repo = git::repo::discover_current().ok()?;
    let workdir = git::repo::workdir(&repo).ok()?;
    let store = Store::new(&repo);
    let ids = store.list_ids().ok()?;

    let project = GlobalConfig::load()
        .ok()
        .and_then(|cfg| cfg.repos.values().find(|e| e.path == workdir).map(|e| e.project.clone()));

    let mut open = 0;
    let mut in_progress = 0;
    for id in &ids {
        if let Ok(task) = store.load(id) {
            match color::status_semantic(&task.status) {
                color::Semantic::Info => open += 1,
                color::Semantic::Warn => in_progress += 1,
                _ => {}
            }
        }
    }

    Some(RepoStatus { project, total: ids.len(), open, in_progress })
}

/// One colorful line naming the repo's project group and, once it has tasks, a quick
/// open/in-progress/total breakdown. `None` (nothing printed) if the repo isn't
/// registered under a project — there'd be nothing to label the line with.
fn project_line(status: &RepoStatus) -> Option<String> {
    let project = status.project.as_ref()?;
    let label = color::bold(&format!("Project {project}"));
    if status.total == 0 {
        return Some(label);
    }
    let open = color::cyan(&format!("○ {} open", status.open));
    let in_progress = color::yellow(&format!("◐ {} in progress", status.in_progress));
    let total = color::bold(&format!("● {} total", status.total));
    Some(format!("{label}  {open}   {in_progress}   {total}"))
}

fn getting_started(bin_name: &str, status: Option<&RepoStatus>) -> Vec<String> {
    match status {
        Some(s) if s.total > 0 => vec![
            format!("Run '{}' to see your tasks.", color::bold(&format!("{bin_name} ls"))),
            format!("Run '{}' to create another.", color::bold(&format!("{bin_name} new \"Title\""))),
        ],
        Some(_) => vec![format!(
            "No tasks yet — run '{}' to create your first one.",
            color::bold(&format!("{bin_name} new \"Title\""))
        )],
        None => vec![format!(
            "Run '{}' inside a git repo to create your first task.",
            color::bold(&format!("{bin_name} new \"Title\""))
        )],
    }
}

/// Shown when the CLI is invoked with no subcommand — `bin_name` picks the right
/// hint ("git task --help" vs "ght --help") for whichever entrypoint was used.
pub fn print(bin_name: &str) {
    println!();
    for line in art_lines() {
        println!("{PADDING}{line}");
    }
    println!();
    println!("{PADDING}{}", color::dim(&format!("Version {} · Commit {}", env!("CARGO_PKG_VERSION"), env!("GIT_TASK_COMMIT_HASH"))));

    println!();

    println!("{PADDING}Distributed Git task manager");
    println!("{PADDING}{}", color::dim(&format!("https://github.com/imohsenb/git-task")));
    println!();

    let status = repo_status();
    if let Some(line) = status.as_ref().and_then(project_line) {
        println!("{PADDING}{line}");
    }

    println!();
    for line in getting_started(bin_name, status.as_ref()) {
        println!("{PADDING}{line}");
    }
    println!("{PADDING}Run '{bin_name} --help' to see all commands.");
    println!();
    println!();
}

/// The art block alone, prefixed above the categorized `--help` output built in
/// `cli/help.rs`. No tagline here — `cli::help::render` pulls the about text straight
/// from clap's own command metadata right after this, so it isn't duplicated — and no
/// repo blurb or "get started" tips either, `--help` output is dense enough already.
pub fn help_banner() -> String {
    let mut out = String::new();
    out.push('\n');
    for line in art_lines() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}
