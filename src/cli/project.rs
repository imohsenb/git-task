use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::global::GlobalConfig;
use crate::logger::Logger;
use crate::output;

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
    /// Delete a non-default project (must be empty unless --force)
    Delete(DeleteArgs),
}

#[derive(Args)]
struct NameArgs {
    name: String,
}

#[derive(Args)]
struct DeleteArgs {
    name: String,
    /// Unregister any repos still under this project instead of refusing to delete it
    #[arg(long)]
    force: bool,
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
            if output::is_json() {
                output::registry::print_mutation("project_created", a.name, None, None, &config);
            } else {
                Logger::info(&format!("Created project '{}'", a.name), None, &[]);
            }
        }
        ProjectAction::SetDefault(a) => {
            let previous = config.default_project.clone();
            config.set_default_project(&a.name)?;
            config.save()?;
            if output::is_json() {
                output::registry::print_mutation("default_set", a.name, None, Some(previous), &config);
            } else {
                Logger::info(&format!("Default project set to '{}'", a.name), None, &[]);
            }
        }
        ProjectAction::Rename(a) => {
            config.rename_project(&a.old_name, &a.new_name)?;
            config.save()?;
            if output::is_json() {
                output::registry::print_mutation("project_renamed", a.new_name, None, Some(a.old_name), &config);
            } else {
                Logger::info(&format!("Renamed project '{}' → '{}'", a.old_name, a.new_name), None, &[]);
            }
        }
        ProjectAction::Delete(a) => {
            let unregistered = config.delete_project(&a.name, a.force)?;
            config.save()?;
            if output::is_json() {
                output::registry::print_mutation("project_deleted", a.name, None, None, &config);
            } else {
                if !unregistered.is_empty() {
                    Logger::info(
                        &format!("Unregistered {} repo(s): {}", unregistered.len(), unregistered.join(", ")),
                        None,
                        &[],
                    );
                }
                Logger::info(&format!("Deleted project '{}'", a.name), None, &[]);
            }
        }
    }
    Ok(())
}
