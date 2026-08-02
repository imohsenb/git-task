use anyhow::Result;
use clap::Args;

use crate::config::project;
use crate::git;
use crate::hints;
use crate::identity;
use crate::output;
use crate::render;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct ShowArgs {
    id: String,
    /// Print markdown instead of the boxed detail view (the old `--format md`, now that
    /// `--format` is the global text/json switch — see `--format json` for machine output)
    #[arg(long)]
    markdown: bool,
}

pub fn run(args: ShowArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let full_id = store.resolve(&args.id)?;
    let task = store.load(&full_id)?;

    if output::is_json() {
        // Bare `Task` for now — enriched into `TaskJson` (display_id, key, resolved names) next.
        output::print_ok(&task);
        return Ok(());
    }

    let key = project::effective_key_for(&repo)?;
    let directory = identity::contributor_directory(&repo)?;
    if args.markdown {
        println!("{}", render::to_markdown(&task, &key, &directory));
    } else {
        println!();
        println!("{}", render::to_text(&task, &key, &directory));
    }
    print_follow_up_hints(&args.id);
    Ok(())
}

fn print_follow_up_hints(id: &str) {
    hints::print(&[
        (format!("status {id} <status>"), "change status".to_string()),
        (format!("comment {id} \"...\""), "add a comment".to_string()),
        (format!("edit {id} --title \"...\""), "edit fields".to_string()),
    ]);
}
