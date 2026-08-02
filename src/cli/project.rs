use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::global::GlobalConfig;
use crate::logger::Logger;

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
            Logger::info(&format!("Created project '{}'", a.name), None, &[]);
        }
        ProjectAction::SetDefault(a) => {
            config.set_default_project(&a.name)?;
            config.save()?;
            Logger::info(&format!("Default project set to '{}'", a.name), None, &[]);
        }
        ProjectAction::Rename(a) => {
            config.rename_project(&a.old_name, &a.new_name)?;
            config.save()?;
            Logger::info(&format!("Renamed project '{}' → '{}'", a.old_name, a.new_name), None, &[]);
        }
        ProjectAction::Delete(a) => {
            config.delete_project(&a.name)?;
            config.save()?;
            Logger::info(&format!("Deleted project '{}'", a.name), None, &[]);
        }
    }
    Ok(())
}
