use std::io::{IsTerminal, Write};

use anyhow::{bail, Result};

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Prompts on a TTY until a non-empty value is entered. Callers must check
/// `is_interactive()` first — this is for the interactive path only.
pub fn ask_required(label: &str) -> Result<String> {
    loop {
        print!("{label}: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        let bytes_read = std::io::stdin().read_line(&mut line)?;
        if bytes_read == 0 {
            bail!("input closed while waiting for {label}");
        }
        let value = line.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
        println!("{label} is required.");
    }
}
