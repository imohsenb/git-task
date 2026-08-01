use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use crate::git;
use crate::render;
use crate::store::git_store::Store;

#[derive(Clone, ValueEnum)]
pub enum ExportFormat {
    Md,
    Json,
}

#[derive(Args)]
pub struct ExportArgs {
    id: Option<String>,
    /// Export every task in the current repo instead of a single one
    #[arg(long)]
    all: bool,
    #[arg(long, value_enum, default_value = "md")]
    format: ExportFormat,
}

pub fn run(args: ExportArgs) -> Result<()> {
    if args.all == args.id.is_some() {
        bail!("pass exactly one of <id> or --all");
    }

    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);

    let tasks = if args.all {
        store
            .list_ids()?
            .into_iter()
            .map(|full_id| store.load(&full_id))
            .collect::<Result<Vec<_>>>()?
    } else {
        let full_id = store.resolve(args.id.as_deref().unwrap())?;
        vec![store.load(&full_id)?]
    };

    match args.format {
        ExportFormat::Json => println!("{}", serde_json::to_string_pretty(&tasks)?),
        ExportFormat::Md => {
            for (i, task) in tasks.iter().enumerate() {
                if i > 0 {
                    println!("\n---\n");
                }
                print!("{}", render::to_markdown(task));
            }
        }
    }
    Ok(())
}
