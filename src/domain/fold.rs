use anyhow::{bail, Context, Result};

use crate::domain::op::{OpEnvelope, Operation};
use crate::domain::task::{Comment, Link, Task, DEFAULT_STATUS};

/// Replays an op-chain (oldest first) into the derived task state.
pub fn fold(id: &str, ops: &[OpEnvelope]) -> Result<Task> {
    let first = ops.first().context("task has no operations")?;
    let Operation::CreateTask { title, kind, description } = &first.op else {
        bail!("task {id} does not start with CreateTask");
    };

    let mut task = Task {
        id: id.to_string(),
        title: title.clone(),
        description: description.clone(),
        kind: *kind,
        status: DEFAULT_STATUS.to_string(),
        priority: None,
        assignee: None,
        reporter: first.author.clone(),
        labels: Default::default(),
        due: None,
        parent: None,
        links: Vec::new(),
        milestone: None,
        comments: Vec::new(),
        created: first.timestamp,
        updated: first.timestamp,
        history: ops.to_vec(),
    };

    let mut next_comment_id: u32 = 1;

    for env in ops {
        task.updated = task.updated.max(env.timestamp);
        match &env.op {
            Operation::CreateTask { .. } => {}
            Operation::SetTitle { title } => task.title = title.clone(),
            Operation::SetDescription { description } => task.description = description.clone(),
            Operation::SetKind { kind } => task.kind = *kind,
            Operation::SetStatus { status } => task.status = status.clone(),
            Operation::SetPriority { priority } => task.priority = Some(priority.clone()),
            Operation::SetAssignee { assignee } => task.assignee = Some(assignee.clone()),
            Operation::AddLabel { label } => {
                task.labels.insert(label.clone());
            }
            Operation::RemoveLabel { label } => {
                task.labels.remove(label);
            }
            Operation::AddComment { text } => {
                task.comments.push(Comment {
                    id: next_comment_id,
                    author: env.author.clone(),
                    timestamp: env.timestamp,
                    text: text.clone(),
                    edited: false,
                });
                next_comment_id += 1;
            }
            Operation::EditComment { comment_id, text } => {
                if let Some(c) = task.comments.iter_mut().find(|c| c.id == *comment_id) {
                    c.text = text.clone();
                    c.edited = true;
                }
            }
            Operation::SetDueDate { due } => task.due = Some(due.clone()),
            Operation::SetParent { parent } => task.parent = Some(parent.clone()),
            Operation::ClearParent => task.parent = None,
            Operation::SetMilestone { milestone } => task.milestone = Some(milestone.clone()),
            Operation::AddLink { kind, target } => {
                let link = Link { kind: *kind, target: target.clone() };
                if !task.links.contains(&link) {
                    task.links.push(link);
                }
            }
            Operation::RemoveLink { kind, target } => {
                task.links.retain(|l| !(l.kind == *kind && &l.target == target));
            }
        }
    }

    Ok(task)
}
