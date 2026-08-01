mod projects;
mod register;
mod repos;
mod unregister;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-task", bin_name = "git task", version, about = "Git-native task manager")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Register the current repo in the user-level config
    Register(register::RegisterArgs),
    /// Remove a repo registration
    Unregister(unregister::UnregisterArgs),
    /// List registered repos
    Repos(repos::ReposArgs),
    /// List projects and the repos grouped under them
    Projects(projects::ProjectsArgs),
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Register(args) => register::run(args),
            Command::Unregister(args) => unregister::run(args),
            Command::Repos(args) => repos::run(args),
            Command::Projects(args) => projects::run(args),
        }
    }
}
