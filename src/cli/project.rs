use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::global::GlobalConfig;

#[derive(Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    action: ProjectAction,
}

#[derive(Subcommand)]
enum ProjectAction {
    /// Create a new, initially empty project (repos join it via 'register --project')
    Create(NameArgs),
    /// Set the project used when 'register --project' is omitted
    SetDefault(NameArgs),
    /// Rename a project, re-tagging every repo registered under it
    Rename(RenameArgs),
    /// Delete an empty, non-default project
    Delete(NameArgs),
}

#[derive(Args)]
struct NameArgs {
    name: String,
}

#[derive(Args)]
struct RenameArgs {
    old_name: String,
    new_name: String,
}

pub fn run(args: ProjectArgs) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    match args.action {
        ProjectAction::Create(a) => {
            config.create_project(&a.name)?;
            config.save()?;
            println!("created project '{}'", a.name);
        }
        ProjectAction::SetDefault(a) => {
            config.set_default_project(&a.name)?;
            config.save()?;
            println!("default project set to '{}'", a.name);
        }
        ProjectAction::Rename(a) => {
            config.rename_project(&a.old_name, &a.new_name)?;
            config.save()?;
            println!("renamed project '{}' to '{}'", a.old_name, a.new_name);
        }
        ProjectAction::Delete(a) => {
            config.delete_project(&a.name)?;
            config.save()?;
            println!("deleted project '{}'", a.name);
        }
    }
    Ok(())
}
