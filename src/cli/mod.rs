mod comment;
mod edit;
mod export;
mod log;
mod ls;
mod new;
mod projects;
mod register;
mod repos;
mod show;
mod status;
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
    /// Create a new task
    New(new::NewArgs),
    /// Show a task
    Show(show::ShowArgs),
    /// List tasks in the current repo
    Ls(ls::LsArgs),
    /// Edit fields on a task
    Edit(edit::EditArgs),
    /// Set a task's status
    Status(status::StatusArgs),
    /// Add or edit a comment on a task
    Comment(comment::CommentArgs),
    /// Show a task's operation history
    Log(log::LogArgs),
    /// Export one or all tasks
    Export(export::ExportArgs),
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
            Command::New(args) => new::run(args),
            Command::Show(args) => show::run(args),
            Command::Ls(args) => ls::run(args),
            Command::Edit(args) => edit::run(args),
            Command::Status(args) => status::run(args),
            Command::Comment(args) => comment::run(args),
            Command::Log(args) => log::run(args),
            Command::Export(args) => export::run(args),
            Command::Register(args) => register::run(args),
            Command::Unregister(args) => unregister::run(args),
            Command::Repos(args) => repos::run(args),
            Command::Projects(args) => projects::run(args),
        }
    }
}
