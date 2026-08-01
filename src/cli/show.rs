use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::config::project;
use crate::git;
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
            println!("{}", render::to_text(&task, &key));
        }
        Format::Md => {
            let key = project::effective_key_for(&repo)?;
            println!("{}", render::to_markdown(&task, &key));
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(&task)?),
    }
    Ok(())
}
