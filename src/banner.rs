use crate::color;
use crate::git;
use crate::store::git_store::Store;

const GLYPH_HEIGHT: usize = 7;

const LIGHT_RED: (f64, f64, f64) = (255.0, 107.0, 107.0);
const YELLOW: (f64, f64, f64) = (255.0, 214.0, 10.0);

fn glyph(c: char) -> [&'static str; GLYPH_HEIGHT] {
    match c {
        'G' => [" ███ ", "█    ", "█ ███", "█   █", "█   █", "█  ██", " ███ "],
        'I' => ["█████", "  █  ", "  █  ", "  █  ", "  █  ", "  █  ", "█████"],
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  ", "  █  ", "  █  "],
        'A' => [" ███ ", "█   █", "█   █", "█████", "█   █", "█   █", "█   █"],
        'S' => [" ████", "█    ", "█    ", " ███ ", "    █", "    █", "████ "],
        'K' => ["█   █", "█  █ ", "█ █  ", "██   ", "█ █  ", "█  █ ", "█   █"],
        _ => ["     ", "     ", "     ", "     ", "     ", "     ", "     "],
    }
}

fn build_lines(word: &str) -> [String; GLYPH_HEIGHT] {
    let mut lines: [String; GLYPH_HEIGHT] = Default::default();
    let chars: Vec<char> = word.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            for row in lines.iter_mut() {
                row.push_str("   ");
            }
            continue;
        }
        let g = glyph(c);
        for (row, glyph_row) in lines.iter_mut().zip(g.iter()) {
            row.push_str(glyph_row);
        }
        if chars.get(i + 1).is_some_and(|next| *next != ' ') {
            for row in lines.iter_mut() {
                row.push(' ');
            }
        }
    }

    lines
}

/// Halves the glyph block's on-screen height by folding each pair of source rows into
/// one line of Unicode half-block characters (▀▄█) — same per-pixel resolution, packed
/// two rows to a terminal line instead of one.
fn compress_rows(lines: &[String]) -> Vec<String> {
    let width = lines.first().map(|l| l.chars().count()).unwrap_or(0);
    let rows: Vec<Vec<char>> = lines.iter().map(|l| l.chars().collect()).collect();

    let mut out = Vec::with_capacity(rows.len().div_ceil(2));
    let mut i = 0;
    while i < rows.len() {
        let top = &rows[i];
        let bottom = rows.get(i + 1);
        let mut line = String::with_capacity(width);
        for col in 0..width {
            let t = top.get(col).is_some_and(|c| *c != ' ');
            let b = bottom.and_then(|r| r.get(col)).is_some_and(|c| *c != ' ');
            line.push(match (t, b) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            });
        }
        out.push(line);
        i += 2;
    }
    out
}

fn gradient_line(line: &str, width: usize) -> String {
    let mut out = String::new();
    for (col, ch) in line.chars().enumerate() {
        if ch == ' ' {
            out.push(' ');
            continue;
        }
        let t = if width <= 1 { 0.0 } else { col as f64 / (width - 1) as f64 };
        let r = (LIGHT_RED.0 + (YELLOW.0 - LIGHT_RED.0) * t).round() as u8;
        let g = (LIGHT_RED.1 + (YELLOW.1 - LIGHT_RED.1) * t).round() as u8;
        let b = (LIGHT_RED.2 + (YELLOW.2 - LIGHT_RED.2) * t).round() as u8;
        out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{ch}"));
    }
    out.push_str("\x1b[0m");
    out
}

/// The compact wordmark lines alone (colorized when stdout is a terminal), with no
/// surrounding blank-line spacing — callers add their own margin.
fn art_lines() -> Vec<String> {
    let lines = build_lines("GIT TASK");
    let width = lines[0].chars().count();
    let compact = compress_rows(&lines);
    if color::enabled() {
        compact.iter().map(|line| gradient_line(line, width)).collect()
    } else {
        compact
    }
}

/// Tips for what to do next, tailored to whether the current repo (if any) already
/// has tasks — points at `new` for a first task, `ls` once there's something to list.
fn getting_started(bin_name: &str) -> Vec<String> {
    let has_tasks = git::repo::discover_current()
        .ok()
        .and_then(|repo| Store::new(&repo).list_ids().ok())
        .map(|ids| !ids.is_empty());

    match has_tasks {
        Some(true) => vec![
            format!("Run '{}' to see your tasks.", color::bold(&format!("{bin_name} ls"))),
            format!("Run '{}' to create another.", color::bold(&format!("{bin_name} new \"Title\""))),
        ],
        Some(false) => vec![format!(
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
        println!("{line}");
    }
    println!();

    println!("Git-native task manager");
    println!();
    println!("Tasks live inside your repo as git objects under refs/tasks/* — no external");
    println!("server, full history, push/pull like any other ref.");
    println!();
    for line in getting_started(bin_name) {
        println!("{line}");
    }
    println!();
    println!("https://github.com/imohsenb/git-task");
    println!("Run '{bin_name} --help' to see all commands.");
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
