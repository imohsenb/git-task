mod automation;
mod clone;
mod comment;
mod completions;
mod config;
mod delete;
mod drop;
mod edit;
mod epic;
mod export;
mod fields;
pub(crate) mod help;
mod init;
mod key;
mod label;
mod link;
mod log;
mod ls;
mod man;
mod new;
mod project;
mod projects;
mod pull;
mod push;
mod register;
mod repos;
mod show;
mod status;
mod unregister;
mod wizard;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Git-native task manager")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Suppress "Tip:" follow-up hints (also settable for good via GIT_TASK_NO_HINTS)
    #[arg(long = "no-hints", global = true)]
    no_hints: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive wizard to set up this repo (key, required fields, registration, automation)
    Init(init::InitArgs),
    /// Create a new task
    New(new::NewArgs),
    /// Show a task
    Show(show::ShowArgs),
    /// List tasks in the current repo
    Ls(ls::LsArgs),
    /// Edit fields on a task
    Edit(edit::EditArgs),
    /// Soft delete a task (records an event, syncs, no restore)
    Delete(delete::DeleteArgs),
    /// Permanently remove a task's local ref (no event, does not sync — see `delete`)
    Drop(drop::DropArgs),
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
    /// Show or edit this repo's config (key, required fields, automation rules)
    Config(config::ConfigArgs),
    /// Clone refs/tasks/* from a remote into a fresh directory (no source checkout)
    Clone(clone::CloneArgs),
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
    /// Create, rename, delete, or set the default project (repo grouping)
    Project(project::ProjectArgs),
    /// Generate shell completions (bash, zsh, fish, powershell, elvish)
    Completions(completions::CompletionsArgs),
    /// Generate/install a man page (fixes `git task --help`'s "No manual entry" error)
    Man(man::ManArgs),
}

impl Cli {
    pub fn run(self, bin_name: &'static str) -> Result<()> {
        crate::hints::init(bin_name, self.no_hints);

        let Some(command) = self.command else {
            crate::banner::print(bin_name);
            return Ok(());
        };

        match command {
            Command::Init(args) => init::run(args),
            Command::New(args) => new::run(args),
            Command::Show(args) => show::run(args),
            Command::Ls(args) => ls::run(args),
            Command::Edit(args) => edit::run(args),
            Command::Delete(args) => delete::run(args),
            Command::Drop(args) => drop::run(args),
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
            Command::Config(args) => config::run(args),
            Command::Clone(args) => clone::run(args),
            Command::Push(args) => push::run(args),
            Command::Pull(args) => pull::run(args),
            Command::Register(args) => register::run(args),
            Command::Unregister(args) => unregister::run(args),
            Command::Repos(args) => repos::run(args),
            Command::Projects(args) => projects::run(args),
            Command::Project(args) => project::run(args),
            Command::Completions(args) => completions::run(args, bin_name),
            Command::Man(args) => man::run(args, bin_name),
        }
    }
}
