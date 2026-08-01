use anyhow::Result;
use clap::{Args, ValueEnum};

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
        Format::Text => println!("{}", render::to_text(&task)),
        Format::Md => println!("{}", render::to_markdown(&task)),
        Format::Json => println!("{}", serde_json::to_string_pretty(&task)?),
    }
    Ok(())
}
