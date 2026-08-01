use std::io::{self, BufRead, Write};

use anyhow::{bail, Context, Result};

use crate::config::global::GlobalConfig;

/// Prompts on stdout, reads one line from stdin, trims trailing newline. Shared by every
/// interactive wizard (`init`, `automation add`, `register`) so they read/behave identically
/// and can be driven from tests via piped stdin. Bails on a closed stdin (0 bytes read) rather
/// than looping forever on the empty string a dead pipe hands back — mirrors
/// `prompt::ask_required`'s guard for the same reason: callers only reach here after checking
/// `prompt::is_interactive()`, so a real EOF here means stdin died mid-wizard, not "no answer".
pub fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush().context("flushing stdout")?;
    let mut line = String::new();
    let bytes_read = io::stdin().lock().read_line(&mut line).context("reading stdin")?;
    if bytes_read == 0 {
        bail!("input closed while waiting for a response to '{label}'");
    }
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

/// Lists every known project (numbered, default marked) and accepts either a list number or a
/// freshly typed name — registration has always allowed implicitly creating a project by typing
/// a name that doesn't exist yet, so this keeps that path open rather than forcing
/// `project create` first. Blank keeps `default`.
pub fn prompt_project(config: &GlobalConfig, label: &str, default: &str) -> Result<String> {
    let known: Vec<String> = config.known_projects().into_iter().collect();
    println!("known projects:");
    for (i, p) in known.iter().enumerate() {
        let marker = if p == default { "  (default)" } else { "" };
        println!("  {}) {p}{marker}", i + 1);
    }
    loop {
        let raw = prompt(&format!("{label} [{default}] (number or new name)"))?;
        if raw.is_empty() {
            return Ok(default.to_string());
        }
        if let Ok(n) = raw.parse::<usize>() {
            if n >= 1 && n <= known.len() {
                return Ok(known[n - 1].clone());
            }
            println!("no project #{n} in the list — type a name to create one, or pick 1-{}", known.len());
            continue;
        }
        return Ok(raw);
    }
}
