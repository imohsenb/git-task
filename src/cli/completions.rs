use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};

use crate::cli::Cli;
use crate::logger::Logger;

#[derive(Args)]
#[command(after_help = "\
Prints a completion script to stdout by default — pass --install to write it straight \
to its default location under your home directory instead (bash/zsh/fish only;
powershell/elvish don't have a single well-known drop-in path, pipe those yourself):

  git task completions zsh --install

Manual install, or for powershell/elvish:

  bash        git task completions bash > /etc/bash_completion.d/git-task
  zsh         git task completions zsh > \"${fpath[1]}/_git-task\"
  fish        git task completions fish > ~/.config/fish/completions/git-task.fish
  powershell  git task completions powershell >> $PROFILE
  elvish      git task completions elvish >> ~/.elvish/rc.elv

Either way, restart your shell (or re-source its rc file) for tab-completion to kick in.")]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    shell: Shell,
    /// Write the script to its default location instead of printing to stdout
    #[arg(long)]
    install: bool,
    /// Write the script to this path instead (implies --install)
    #[arg(long)]
    path: Option<PathBuf>,
}

/// Whether a completion dir is already on the shell's default search path — if not,
/// the user still has a one-time rc edit to do after we write the file.
struct InstallDir {
    path: PathBuf,
    already_loaded: bool,
}

pub fn run(args: CompletionsArgs, bin_name: &str) -> Result<()> {
    // Shell completion functions need a valid single-word identifier — "git task"
    // (the two-word dispatch form) isn't one and crashes the bash generator, so use
    // the real on-PATH executable name ("git-task") instead of the display bin_name.
    let exe_name = bin_name.replace(' ', "-");
    let mut cmd = Cli::command();

    if !args.install && args.path.is_none() {
        generate(args.shell, &mut cmd, exe_name, &mut std::io::stdout());
        return Ok(());
    }

    let dir = match args.path {
        Some(path) => InstallDir { path, already_loaded: true },
        None => default_install_dir(args.shell)?,
    };

    if let Some(parent) = dir.path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = fs::File::create(&dir.path).with_context(|| format!("writing {}", dir.path.display()))?;
    generate(args.shell, &mut cmd, exe_name, &mut file);

    Logger::info(&format!("Installed {:?} completions to {}", args.shell, dir.path.display()), None, &[]);
    if dir.already_loaded {
        println!("Restart your shell (or exec $SHELL) for tab-completion to kick in.");
    } else if let Some(parent) = dir.path.parent() {
        println!(
            "'{}' isn't on {:?}'s default completion path — add this once to its rc file, then restart your shell:",
            parent.display(),
            args.shell
        );
        match args.shell {
            Shell::Zsh => {
                println!("  fpath=({} $fpath)", parent.display());
                println!("  autoload -Uz compinit && compinit");
            }
            _ => println!("  source {}", dir.path.display()),
        }
    }

    Ok(())
}

/// Every default lives under the user's home directory — never a shared system prefix
/// (e.g. Homebrew's `/opt/homebrew/share/zsh/site-functions`, which is technically
/// often writable without sudo but is still outside `$HOME` and shared by every other
/// tool on the machine) — `--install` with no `--path` shouldn't touch anything outside
/// the user's own directory without being asked.
fn default_install_dir(shell: Shell) -> Result<InstallDir> {
    let home = directories::BaseDirs::new()
        .context("could not determine home directory")?
        .home_dir()
        .to_path_buf();

    match shell {
        // XDG bash-completion v2 user dir — auto-loaded by the `bash-completion` package
        // with no rc edit, on both macOS (Homebrew) and most Linux distros.
        Shell::Bash => Ok(InstallDir {
            path: home.join(".local/share/bash-completion/completions/git-task"),
            already_loaded: true,
        }),
        // fish scans this directory on startup unconditionally.
        Shell::Fish => Ok(InstallDir {
            path: home.join(".config/fish/completions/git-task.fish"),
            already_loaded: true,
        }),
        // zsh has no per-user drop-in dir that's on `fpath` out of the box, so this always
        // needs the one-time fpath/compinit line `run` prints after writing the file.
        Shell::Zsh => Ok(InstallDir { path: home.join(".zsh/completions/_git-task"), already_loaded: false }),
        other => bail!(
            "--install doesn't know a default location for {other:?} — pipe the script yourself, see 'git task completions {other} --help'"
        ),
    }
}
