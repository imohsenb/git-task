use std::io::IsTerminal;

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

/// Shown when the CLI is invoked with no subcommand — `bin_name` picks the right
/// hint ("git task --help" vs "ght --help") for whichever entrypoint was used.
pub fn print(bin_name: &str) {
    let lines = build_lines("GIT TASK");
    let width = lines[0].chars().count();
    let colorize = std::io::stdout().is_terminal();

    for line in &lines {
        if colorize {
            println!("{}", gradient_line(line, width));
        } else {
            println!("{line}");
        }
    }

    println!();
    println!("Git-native task manager");
    println!();
    println!("Tasks live inside your repo as git objects under refs/tasks/* — no external");
    println!("server, full history, push/pull like any other ref.");
    println!();
    println!("https://github.com/imohsenb/git-task");
    println!();
    println!("Run '{bin_name} --help' to see all commands.");
}
