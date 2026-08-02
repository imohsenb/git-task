use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::config::project;
use crate::git;
use crate::hints;
use crate::identity;
use crate::render;
use crate::store::git_store::Store;

#[derive(Clone, ValueEnum)]
pub enum Format {
    Text,
    Md,
    Json,
}

#[derive(Args)]
pub struct ShowArgs {
    id: String,
    #[arg(long, value_enum, default_value = "text")]
    format: Format,
}

pub fn run(args: ShowArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let full_id = store.resolve(&args.id)?;
    let task = store.load(&full_id)?;

    match args.format {
        Format::Text => {
            let key = project::effective_key_for(&repo)?;
            let directory = identity::contributor_directory(&repo)?;
            println!();
            println!("{}", render::to_text(&task, &key, &directory));
            print_follow_up_hints(&args.id);
        }
        Format::Md => {
            let key = project::effective_key_for(&repo)?;
            let directory = identity::contributor_directory(&repo)?;
            println!("{}", render::to_markdown(&task, &key, &directory));
            print_follow_up_hints(&args.id);
        }
        // Machine-readable output — never append a hint block, it'd corrupt the JSON for
        // anything piping this into a parser.
        Format::Json => println!("{}", serde_json::to_string_pretty(&task)?),
    }
    Ok(())
}

fn print_follow_up_hints(id: &str) {
    hints::print(&[
        (format!("status {id} <status>"), "change status".to_string()),
        (format!("comment {id} \"...\""), "add a comment".to_string()),
        (format!("edit {id} --title \"...\""), "edit fields".to_string()),
    ]);
}
