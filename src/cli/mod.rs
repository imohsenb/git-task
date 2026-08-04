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
mod skills;
mod status;
mod target_repo;
mod unregister;
mod version;
mod whoami;
mod wizard;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::output;

#[derive(Parser)]
#[command(version, about = "Git-native task manager")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Suppress "Tip:" follow-up hints (also settable for good via GIT_TASK_NO_HINTS)
    #[arg(long = "no-hints", global = true)]
    no_hints: bool,
    /// Output format: human-readable text (default) or a single JSON document on stdout,
    /// suitable for another program to parse (see README for the response shape)
    #[arg(long = "format", value_enum, global = true, default_value = "text")]
    format: output::Format,
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
    /// Add or remove a fixed/affected version from a task
    Version(version::VersionArgs),
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
    /// Install this tool's bundled coding-agent skills (SKILL.md) into agent skill directories
    Skills(skills::SkillsArgs),
    /// Show what identity a write would be attributed to (repo/global/effective config layers)
    Whoami(whoami::WhoamiArgs),
}

impl Cli {
    pub fn run(self, bin_name: &'static str) -> Result<()> {
        crate::hints::init(bin_name, self.no_hints);
        output::init_format(self.format);

        let Some(command) = self.command else {
            crate::banner::print(bin_name);
            return Ok(());
        };

        // Sets the coarse invoked-command name (used by the `--format json` envelope) before
        // dispatching. Commands with sub-actions whose JSON payload differs by action (`config`,
        // `project`) refine this further themselves, since only they know which action ran.
        macro_rules! dispatch {
            ($name:literal, $call:expr) => {{
                output::set_command($name);
                $call
            }};
        }

        match command {
            Command::Init(args) => dispatch!("init", init::run(args)),
            Command::New(args) => dispatch!("new", new::run(args)),
            Command::Show(args) => dispatch!("show", show::run(args)),
            Command::Ls(args) => dispatch!("ls", ls::run(args)),
            Command::Edit(args) => dispatch!("edit", edit::run(args)),
            Command::Delete(args) => dispatch!("delete", delete::run(args)),
            Command::Drop(args) => dispatch!("drop", drop::run(args)),
            Command::Status(args) => dispatch!("status", status::run(args)),
            Command::Comment(args) => dispatch!("comment", comment::run(args)),
            Command::Label(args) => dispatch!("label", label::run(args)),
            Command::Version(args) => dispatch!("version", version::run(args)),
            Command::Log(args) => dispatch!("log", log::run(args)),
            Command::Export(args) => dispatch!("export", export::run(args)),
            Command::Epic(args) => dispatch!("epic", epic::run(args)),
            Command::Link(args) => dispatch!("link", link::run(args)),
            Command::Key(args) => dispatch!("key", key::run(args)),
            Command::Fields(args) => dispatch!("fields", fields::run(args)),
            Command::Automation(args) => dispatch!("automation", automation::run(args)),
            Command::Config(args) => dispatch!("config", config::run(args)),
            Command::Clone(args) => dispatch!("clone", clone::run(args)),
            Command::Push(args) => dispatch!("push", push::run(args)),
            Command::Pull(args) => dispatch!("pull", pull::run(args)),
            Command::Register(args) => dispatch!("register", register::run(args)),
            Command::Unregister(args) => dispatch!("unregister", unregister::run(args)),
            Command::Repos(args) => dispatch!("repos", repos::run(args)),
            Command::Projects(args) => dispatch!("projects", projects::run(args)),
            Command::Project(args) => dispatch!("project", project::run(args)),
            Command::Completions(args) => completions::run(args, bin_name),
            Command::Man(args) => man::run(args, bin_name),
            Command::Skills(args) => dispatch!("skills", skills::run(args)),
            Command::Whoami(args) => dispatch!("whoami", whoami::run(args)),
        }
    }
}
