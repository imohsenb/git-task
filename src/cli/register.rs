use anyhow::Result;
use clap::Args;

use crate::cli::wizard;
use crate::config::global::GlobalConfig;
use crate::git;
use crate::prompt;

#[derive(Args)]
pub struct RegisterArgs {
    /// Name to register the repo under (defaults to the repo directory name)
    name: Option<String>,
    /// Project to assign. First registration: defaults to your default project (prompts to
    /// pick one instead, if running interactively). Already registered: moves the repo there.
    #[arg(long)]
    project: Option<String>,
}

pub fn run(args: RegisterArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let path = git::repo::workdir(&repo)?;

    let name = args.name.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string())
    });

    let mut config = GlobalConfig::load()?;

    // Already registered: reassign in place instead of the old "run unregister first" error —
    // rerunning `register` is now how you move a repo between projects.
    if let Some(entry) = config.repos.get(&name) {
        let current = entry.project.clone();
        let project = match args.project {
            Some(p) => p,
            None if !prompt::is_interactive() => {
                println!(
                    "'{name}' is already registered in project '{current}' — pass --project to move it"
                );
                return Ok(());
            }
            None => wizard::prompt_project(
                &config,
                &format!("'{name}' is in project '{current}'. Move to"),
                &current,
            )?,
        };

        if project == current {
            println!("'{name}' is already in project '{project}'; nothing to do");
            return Ok(());
        }
        config.repos.get_mut(&name).expect("checked above").project = project.clone();
        config.save()?;
        println!("moved '{name}' to project '{project}'");
        return Ok(());
    }

    let project = match args.project {
        Some(p) => Some(p),
        None if !prompt::is_interactive() => None,
        None => {
            let default_project = config.default_project.clone();
            Some(wizard::prompt_project(&config, "Project for this repo", &default_project)?)
        }
    };

    let project = config.register(name.clone(), path.clone(), project)?;
    config.save()?;

    println!("registered '{name}' ({}) in project '{project}'", path.display());
    Ok(())
}
