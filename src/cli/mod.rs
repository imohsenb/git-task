mod automation;
mod comment;
mod edit;
mod epic;
mod export;
mod fields;
mod key;
mod label;
mod link;
mod log;
mod ls;
mod new;
mod projects;
mod pull;
mod push;
mod register;
mod repos;
mod show;
mod status;
mod unregister;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Git-native task manager")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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
    /// Add or remove a label from a task
    Label(label::LabelArgs),
    /// Show a task's operation history
    Log(log::LogArgs),
    /// Export one or all tasks
    Export(export::ExportArgs),
    /// Add or remove a task from an epic
    Epic(epic::EpicArgs),
    /// Add or remove a link between two tasks
    Link(link::LinkArgs),
    /// Show or set this repo's short address key (e.g. SRV, used as SRV-9057e58a)
    Key(key::KeyArgs),
    /// Show the effective required-field schema for this repo
    Fields(fields::FieldsArgs),
    /// Inspect automation rules
    Automation(automation::AutomationArgs),
    /// Push refs/tasks/* to a remote
    Push(push::PushArgs),
    /// Fetch refs/tasks/* from a remote and merge into local tasks
    Pull(pull::PullArgs),
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
    pub fn run(self, bin_name: &str) -> Result<()> {
        let Some(command) = self.command else {
            crate::banner::print(bin_name);
            return Ok(());
        };

        match command {
            Command::New(args) => new::run(args),
            Command::Show(args) => show::run(args),
            Command::Ls(args) => ls::run(args),
            Command::Edit(args) => edit::run(args),
            Command::Status(args) => status::run(args),
            Command::Comment(args) => comment::run(args),
            Command::Label(args) => label::run(args),
            Command::Log(args) => log::run(args),
            Command::Export(args) => export::run(args),
            Command::Epic(args) => epic::run(args),
            Command::Link(args) => link::run(args),
            Command::Key(args) => key::run(args),
            Command::Fields(args) => fields::run(args),
            Command::Automation(args) => automation::run(args),
            Command::Push(args) => push::run(args),
            Command::Pull(args) => pull::run(args),
            Command::Register(args) => register::run(args),
            Command::Unregister(args) => unregister::run(args),
            Command::Repos(args) => repos::run(args),
            Command::Projects(args) => projects::run(args),
        }
    }
}
