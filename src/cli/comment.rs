use anyhow::Result;
use clap::Args;

use crate::actor::Actor;
use crate::automation;
use crate::config::project;
use crate::domain::id;
use crate::domain::op::Operation;
use crate::git;
use crate::logger::{task_ref, Logger};
use crate::output::ClassifiedError;
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
    let key = project::effective_key_for(&repo)?;

    let task = store.load(&task_id)?;
    let op = match args.edit {
        Some(comment_id) => {
            if !task.comments.iter().any(|c| c.id == comment_id) {
                return Err(anyhow::Error::new(ClassifiedError::NotFound {
                    message: format!("comment #{comment_id} not found on {}", id::display(&key, &task_id)),
                    query: comment_id.to_string(),
                    entity: "comment".to_string(),
                }));
            }
            Operation::EditComment { comment_id, text: args.text.clone() }
        }
        None => Operation::AddComment { text: args.text.clone() },
    };
    let editing = args.edit.is_some();

    let ops = vec![op];
    store.append(&task_id, &author, ops.clone())?;
    automation::engine::run(&repo, &task_id, &ops)?;
    let display_id = id::display(&key, &task_id);
    let action = if editing { "Comment updated" } else { "Comment added" };
    Logger::info(&format!("{action} {}", task_ref(&display_id, task.kind, &task.title)), None, &[]);
    Ok(())
}
