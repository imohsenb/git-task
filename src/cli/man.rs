use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, CommandFactory};

use crate::cli::Cli;
use crate::logger::Logger;

#[derive(Args)]
#[command(after_help = "\
Prints a roff man page to stdout by default — pass --install to write it straight to
its default location under your home directory instead:

  git task man --install

This exists because git intercepts bare `git task --help` (no subcommand after it) and
runs `man git-task` instead of the binary itself — without a man page installed, that
prints \"No manual entry for git-task\". `git task -h`, `git task help`, and
`git task <command> --help` all bypass this and work regardless.")]
pub struct ManArgs {
    /// Write the page to its default location instead of printing to stdout
    #[arg(long)]
    install: bool,
    /// Write the page to this path instead (implies --install)
    #[arg(long)]
    path: Option<PathBuf>,
}

pub fn run(args: ManArgs, bin_name: &'static str) -> Result<()> {
    // `man <name>` and the `.1` filename must match the real on-PATH executable
    // ("git-task") — the two-word display bin_name ("git task") only belongs in the
    // SYNOPSIS line, same split `completions.rs` makes for its exe_name.
    let exe_name = bin_name.replace(' ', "-");
    let cmd = Cli::command().name(exe_name.clone()).bin_name(bin_name);
    let man = clap_mangen::Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf).context("rendering man page")?;

    if !args.install && args.path.is_none() {
        use std::io::Write;
        std::io::stdout().write_all(&buf).context("writing man page to stdout")?;
        return Ok(());
    }

    let path = match args.path {
        Some(path) => path,
        None => default_install_path(&exe_name)?,
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, &buf).with_context(|| format!("writing {}", path.display()))?;

    Logger::info(&format!("Installed man page to {}", path.display()), None, &[]);
    println!("If `man {exe_name}` still can't find it, add this to your shell rc file:");
    println!("  export MANPATH=\"{}:$MANPATH\"", path.parent().unwrap().parent().unwrap().display());

    Ok(())
}

/// Under `$HOME` only, matching `completions`'s default-install rule — never write
/// outside the user's own directory without `--path` being asked for explicitly.
fn default_install_path(exe_name: &str) -> Result<PathBuf> {
    let home = directories::BaseDirs::new().context("could not determine home directory")?.home_dir().to_path_buf();
    Ok(home.join(".local/share/man/man1").join(format!("{exe_name}.1")))
}
