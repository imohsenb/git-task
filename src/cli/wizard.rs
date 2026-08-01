use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};

/// Prompts on stdout, reads one line from stdin, trims trailing newline. Shared by every
/// interactive wizard (`init`, `automation add`) so they read/behave identically and can be
/// driven from tests via piped stdin.
pub fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush().context("flushing stdout")?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).context("reading stdin")?;
    Ok(line.trim().to_string())
}

pub fn prompt_default(label: &str, default: &str) -> Result<String> {
    let raw = prompt(&format!("{label} [{default}]"))?;
    Ok(if raw.is_empty() { default.to_string() } else { raw })
}

pub fn prompt_yn(label: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    let raw = prompt(&format!("{label} [{hint}]"))?.to_ascii_lowercase();
    Ok(match raw.as_str() {
        "" => default,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    })
}

/// Numbered menu; returns the chosen option's index. Reprompts on out-of-range/non-numeric
/// input rather than falling back silently, since a wizard picking the wrong event or scope
/// by accident is worse than one extra prompt.
pub fn prompt_choice(label: &str, options: &[&str], default_idx: usize) -> Result<usize> {
    println!("{label}:");
    for (i, opt) in options.iter().enumerate() {
        println!("  {}) {}", i + 1, opt);
    }
    loop {
        let raw = prompt_default("choice", &(default_idx + 1).to_string())?;
        if let Ok(n) = raw.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                return Ok(n - 1);
            }
        }
        println!("enter a number 1-{}", options.len());
    }
}
