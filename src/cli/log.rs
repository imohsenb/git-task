use anyhow::Result;
use clap::Args;

use crate::git;
use crate::render;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct LogArgs {
    id: String,
}

pub fn run(args: LogArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let task_id = store.resolve(&args.id)?;
    let task = store.load(&task_id)?;

    print!("{}", render::to_log(&task));
    Ok(())
}
