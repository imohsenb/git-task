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

// Slate-grey truecolor, standing in for `color::dim`'s ANSI faint code (which renders
// inconsistently — some terminals barely darken text under it) in the banner's
// secondary text (version/commit, tagline, hint lines).
const SLATE: (u8, u8, u8) = (100, 116, 139);

fn dim(s: &str) -> String {
    if color::enabled() {
        let (r, g, b) = SLATE;
        format!("\x1b[38;2;{r};{g};{b}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// The "GIT TASK" wordmark, fixed pixel-for-pixel (including the half-block edge
/// antialiasing) rather than composed from a generic per-letter font — this is the
/// exact block art the banner was asked to use, just recolored per `bevel_color`.
const WORDMARK: [&str; 11] = [
    "  ▄▄▄▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄▄▄▄▄▄      ▄▄▄▄▄▄▄▄▄   ▄▄▄▄▄▄     ▄▄▄▄▄▄   ▄▄▄▄▄ ▄▄▄▄",
    "▄▀▒░     ▓ █▓▒░▓ █▓▒░    ▒      █▓▒░    ▒ ▄▀▒░    ▀▄ ▄▀▒░    ▀▄ █▓▒░▓ ▓▓▒█",
    "█▒░ ▓▀▒  ▒ █▒░ ▒ ▀▀█   █▀▀      ▀▀█   █▀▀ █▒░ ▄▀▄  ▓ █▒░ █▀▄  ▓ █▒░ ▒ ▒▒░▓",
    "█░  ▒ ▀▀▀▀ █░  ░   █   █          █   █   █░  ▒ ▒  ▓ █░  ▒ ▒▄▄▓ █░  ░ ▓░ ▒",
    "█   ▀▀▀▀▀▓ █   █   █   █          █   █   █   ░▄▀  ▒ █   ▓▄▄▄   █   ▀▀  ▄▀",
    "▓   █▄▄  ░ ▓   █   ▓   █          ▓   █   ▓        ▒  ▀▄▄    ▀▄ ▓   ▄▄ ▀▄ ",
    "▒   ▓ ░  ░ ▒   ▓   ▒   ▓          ▒   ▓   ▒   ▓▀░  ░     ▀▀░  ░ ▒   ▓ ░  ▓",
    "░   ▒ ▒  ▒ ░   ▒   ░   ▒          ░   ▒   ░   ░ ▒  ▒ ▓▀▀▀▒ ▒  ▒ ░   ▒ ▓  ▒",
    "█   ░▄▀ ░░ █  ░░   █  ░░          █  ░░   █  ░░ ▓ ░░ ▒▒░ ░▄▓ ░░ █  ░░ ▒ ░░",
    "▀▄     ░▓█ █ ░▒    █ ░▒           █ ░▒    █ ░▒█ ▓░▒█ ░      ░▒█ █ ░▓█ ░░▓█",
    "  ▀▀▀▀▀▀▀▀ ▀▀▀▀▀   ▀▀▀▀▀          ▀▀▀▀▀   ▀▀▀▀▀ ▀▀▀▀ ▀▀▀▀▀▀▀▀▀  ▀▀▀▀▀ ▀▀▀▀",
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

/// Current repo's name/branch and task counts, plus the project group it's registered
/// under (if any). `None` for the whole struct means not in a git repo; `.project` is
/// separately `None` when the repo just isn't registered — both are valid, silent
/// states, not errors.
struct RepoStatus {
    repo_name: String,
    branch: Option<String>,
    project: Option<String>,
    total: usize,
    open: usize,
    in_progress: usize,
    done: usize,
}

fn repo_status() -> Option<RepoStatus> {
    let repo = git::repo::discover_current().ok()?;
    let workdir = git::repo::workdir(&repo).ok()?;
    let store = Store::new(&repo);
    let ids = store.list_ids().ok()?;

    let project = GlobalConfig::load()
        .ok()
        .and_then(|cfg| cfg.repos.values().find(|e| e.path == workdir).map(|e| e.project.clone()));

    let repo_name = workdir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".to_string());
    let branch = repo.head().ok().and_then(|head| head.shorthand().map(str::to_string));

    let mut open = 0;
    let mut in_progress = 0;
    let mut done = 0;
    for id in &ids {
        if let Ok(task) = store.load(id) {
            match color::status_semantic(&task.status) {
                color::Semantic::Info => open += 1,
                color::Semantic::Warn => in_progress += 1,
                color::Semantic::Success => done += 1,
                _ => {}
            }
        }
    }

    Some(RepoStatus { repo_name, branch, project, total: ids.len(), open, in_progress, done })
}

/// Top border of a titled box: dim rule, bold title inline in the rule itself (like a
/// fieldset legend) rather than as a first content row.
fn box_top(border_width: usize, title: &str) -> String {
    let prefix_len = 3 + title.chars().count(); // "─ " + title + " "
    let dashes = border_width.saturating_sub(prefix_len).max(1);
    format!("{}{}{}", dim("╭─ "), color::bold(title), dim(&format!(" {}╮", "─".repeat(dashes))))
}

fn box_bottom(border_width: usize) -> String {
    dim(&format!("╰{}╯", "─".repeat(border_width)))
}

/// One content row, padded to `border_width` using `plain`'s visible length — `colored`
/// carries the same text with ANSI codes added, which don't count toward that length.
fn box_row(border_width: usize, plain: &str, colored: &str) -> String {
    let pad = border_width.saturating_sub(3 + plain.chars().count());
    format!("{}  {}{} {}", dim("│"), colored, " ".repeat(pad), dim("│"))
}

/// The bordered "PROJECT CONTEXT" card: repo name/branch, registered project group (if
/// any), and the open/in-progress/done/total breakdown once the repo has tasks.
fn project_box(status: &RepoStatus) -> Vec<String> {
    let mut plain_rows = Vec::new();
    let mut colored_rows = Vec::new();

    match &status.branch {
        Some(b) => {
            plain_rows.push(format!("Repo: {}  [{b}]", status.repo_name));
            colored_rows.push(format!("{}{}  [{}]", dim("Repo: "), color::bold(&status.repo_name), color::cyan(b)));
        }
        None => {
            plain_rows.push(format!("Repo: {}", status.repo_name));
            colored_rows.push(format!("{}{}", dim("Repo: "), color::bold(&status.repo_name)));
        }
    }

    if let Some(project) = &status.project {
        plain_rows.push(format!("Project: {project}"));
        colored_rows.push(format!("{}{}", dim("Project: "), color::bold(project)));
    }

    if status.total > 0 {
        plain_rows.push(format!(
            "Status: ○ {} open   ◐ {} in progress   ✓ {} done   ● {} total",
            status.open, status.in_progress, status.done, status.total
        ));
        colored_rows.push(format!(
            "{}{}   {}   {}   ● {} total",
            dim("Status: "),
            color::cyan(&format!("○ {} open", status.open)),
            color::yellow(&format!("◐ {} in progress", status.in_progress)),
            color::green(&format!("✓ {} done", status.done)),
            status.total
        ));
    } else {
        plain_rows.push("Status: no tasks yet".to_string());
        colored_rows.push(dim("Status: no tasks yet"));
    }

    let title = "PROJECT CONTEXT";
    let max_content = plain_rows.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let border_width = (max_content + 3).max(title.chars().count() + 4);

    let mut lines = vec![box_top(border_width, title)];
    for (plain, colored) in plain_rows.iter().zip(colored_rows.iter()) {
        lines.push(box_row(border_width, plain, colored));
    }
    lines.push(box_bottom(border_width));
    lines
}

/// `(command, description)` pairs for the "QUICK COMMANDS" list, tailored to whether
/// the current repo (if any) already has tasks — points at `new` for a first task,
/// `ls` once there's something to list. `--help` is always valid, repo or not.
fn quick_commands(bin_name: &str, status: Option<&RepoStatus>) -> Vec<(String, String)> {
    match status {
        Some(s) if s.total > 0 => vec![
            (format!("{bin_name} ls"), "View current tasks".to_string()),
            (format!("{bin_name} new \"Title\""), "Create a new task".to_string()),
            (format!("{bin_name} --help"), "Show all commands".to_string()),
        ],
        Some(_) => vec![
            (format!("{bin_name} new \"Title\""), "Create your first task".to_string()),
            (format!("{bin_name} --help"), "Show all commands".to_string()),
        ],
        None => vec![
            (format!("{bin_name} new \"Title\""), "Create your first task (inside a git repo)".to_string()),
            (format!("{bin_name} --help"), "Show all commands".to_string()),
        ],
    }
}

/// Bolds a quick-command string, picking out a `"..."` placeholder (e.g. `"Title"`) in
/// cyan so it reads as "fill this in" rather than literal syntax.
fn highlight_cmd(cmd: &str) -> String {
    if let Some(start) = cmd.find('"') {
        if let Some(end_rel) = cmd[start + 1..].find('"') {
            let end = start + 1 + end_rel;
            let (before, rest) = cmd.split_at(start);
            let (quoted, after) = rest.split_at(end - start + 1);
            return format!("{}{}{}", color::bold(before), color::cyan(quoted), color::bold(after));
        }
    }
    color::bold(cmd)
}

/// Shown when the CLI is invoked with no subcommand — `bin_name` picks the right
/// hint ("git task --help" vs "ght --help") for whichever entrypoint was used.
pub fn print(bin_name: &str) {
    println!();
    for line in art_lines() {
        println!("{PADDING}{line}");
    }
    println!();
    println!(
        "{PADDING}{} {}",
        color::bold(&format!("Version {}", env!("CARGO_PKG_VERSION"))),
        dim(&format!("· Commit {}", env!("GIT_TASK_COMMIT_HASH")))
    );
    println!("{PADDING}{}", dim("Distributed Git task manager • https://github.com/imohsenb/git-task"));
    println!();

    let status = repo_status();
    if let Some(s) = status.as_ref() {
        for line in project_box(s) {
            println!("{PADDING}{line}");
        }
        println!();
    }

    println!("{PADDING}{}", dim("QUICK COMMANDS"));
    let rows = quick_commands(bin_name, status.as_ref());
    let width = rows.iter().map(|(c, _)| c.chars().count()).max().unwrap_or(0);
    for (cmd, desc) in &rows {
        let pad = " ".repeat(width.saturating_sub(cmd.chars().count()) + 2);
        println!("{PADDING}  {}{pad}{}", highlight_cmd(cmd), dim(desc));
    }
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
