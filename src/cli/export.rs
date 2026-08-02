use anyhow::{bail, Result};
use clap::Args;

use crate::config::project;
use crate::git;
use crate::identity;
use crate::output;
use crate::render;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct ExportArgs {
    id: Option<String>,
    /// Export every task in the current repo instead of a single one
    #[arg(long)]
    all: bool,
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

    if output::is_json() {
        // Bare `Task[]` for now — enriched into `TaskJson[]` next.
        output::print_ok(&tasks);
        return Ok(());
    }

    let key = project::effective_key_for(&repo)?;
    let directory = identity::contributor_directory(&repo)?;
    for (i, task) in tasks.iter().enumerate() {
        if i > 0 {
            println!("\n---\n");
        }
        print!("{}", render::to_markdown(task, &key, &directory));
    }
    Ok(())
}
