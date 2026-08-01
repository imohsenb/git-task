use anyhow::{bail, Result};
use clap::Args;

use crate::actor::Actor;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct CommentArgs {
    id: String,
    text: String,
    /// Edit an existing comment by its number (shown in `git task show`) instead of adding a new one
    #[arg(long = "edit")]
    edit: Option<u32>,
}

pub fn run(args: CommentArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let store = Store::new(&repo);
    let task_id = store.resolve(&args.id)?;

    let op = match args.edit {
        Some(comment_id) => {
            let task = store.load(&task_id)?;
            if !task.comments.iter().any(|c| c.id == comment_id) {
                bail!("comment #{comment_id} not found on {}", id::short(&task_id));
            }
            Operation::EditComment { comment_id, text: args.text.clone() }
        }
        None => Operation::AddComment { text: args.text.clone() },
    };
    let editing = args.edit.is_some();

    store.append(&task_id, &author, vec![op])?;
    if editing {
        println!("comment updated on {}", id::short(&task_id));
    } else {
        println!("comment added to {}", id::short(&task_id));
    }
    Ok(())
}
