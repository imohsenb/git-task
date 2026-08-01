use anyhow::{bail, Result};
use clap::Args;

use crate::config::project::ProjectConfig;
use crate::git;

#[derive(Args)]
pub struct KeyArgs {
    /// New key to set (e.g. SRV). Omit to print the current effective key.
    new_key: Option<String>,
}

pub fn run(args: KeyArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let mut cfg = ProjectConfig::load(&workdir)?;

    match args.new_key {
        None => {
            let effective = cfg.effective_key(&workdir);
            match &cfg.key {
                Some(_) => println!("{effective} (from .gittask/config.toml)"),
                None => println!(
                    "{effective} (derived from repo name — run 'git task key {effective}' to pin it)"
                ),
            }
        }
        Some(raw) => {
            let key = raw.to_ascii_uppercase();
            let valid = key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
                && key.chars().all(|c| c.is_ascii_alphanumeric());
            if !valid {
                bail!("key must start with a letter and contain only letters/digits, got '{raw}'");
            }
            cfg.key = Some(key.clone());
            cfg.save(&workdir)?;
            println!("key set to {key} (.gittask/config.toml)");
        }
    }
    Ok(())
}
