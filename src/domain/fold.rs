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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use crate::domain::op::{LinkKind, TaskKind};

    fn actor() -> Actor {
        Actor { name: "Test User".into(), email: "test@example.com".into() }
    }

    fn env(ts: i64, op: Operation) -> OpEnvelope {
        OpEnvelope { author: actor(), timestamp: ts, op }
    }

    #[test]
    fn create_sets_defaults() {
        let ops = vec![env(
            100,
            Operation::CreateTask { title: "Title".into(), kind: TaskKind::Bug, description: "Desc".into() },
        )];
        let task = fold("abc", &ops).unwrap();
        assert_eq!(task.title, "Title");
        assert_eq!(task.kind, TaskKind::Bug);
        assert_eq!(task.status, DEFAULT_STATUS);
        assert_eq!(task.priority, None);
        assert_eq!(task.created, 100);
        assert_eq!(task.updated, 100);
        assert_eq!(task.reporter, actor());
    }

    #[test]
    fn empty_ops_errors() {
        assert!(fold("abc", &[]).is_err());
    }

    #[test]
    fn missing_create_errors() {
        let ops = vec![env(100, Operation::SetStatus { status: "doing".into() })];
        assert!(fold("abc", &ops).is_err());
    }

    #[test]
    fn labels_dedup_on_add_and_ignore_missing_on_remove() {
        let ops = vec![
            env(1, Operation::CreateTask { title: "T".into(), kind: TaskKind::Task, description: "".into() }),
            env(2, Operation::AddLabel { label: "urgent".into() }),
            env(3, Operation::AddLabel { label: "urgent".into() }),
            env(4, Operation::RemoveLabel { label: "nope".into() }),
        ];
        let task = fold("abc", &ops).unwrap();
        assert_eq!(task.labels.len(), 1);
        assert!(task.labels.contains("urgent"));
        assert_eq!(task.updated, 4);
    }

    #[test]
    fn comments_get_sequential_ids_and_edit_by_id() {
        let ops = vec![
            env(1, Operation::CreateTask { title: "T".into(), kind: TaskKind::Task, description: "".into() }),
            env(2, Operation::AddComment { text: "first".into() }),
            env(3, Operation::AddComment { text: "second".into() }),
            env(4, Operation::EditComment { comment_id: 1, text: "first (edited)".into() }),
            env(5, Operation::EditComment { comment_id: 99, text: "no such comment".into() }),
        ];
        let task = fold("abc", &ops).unwrap();
        assert_eq!(task.comments.len(), 2);
        assert_eq!(task.comments[0].id, 1);
        assert_eq!(task.comments[0].text, "first (edited)");
        assert!(task.comments[0].edited);
        assert_eq!(task.comments[1].id, 2);
        assert!(!task.comments[1].edited);
    }

    #[test]
    fn links_dedup_and_remove() {
        let ops = vec![
            env(1, Operation::CreateTask { title: "T".into(), kind: TaskKind::Task, description: "".into() }),
            env(2, Operation::AddLink { kind: LinkKind::Blocks, target: "other".into() }),
            env(3, Operation::AddLink { kind: LinkKind::Blocks, target: "other".into() }),
            env(4, Operation::AddLink { kind: LinkKind::Relates, target: "other".into() }),
            env(5, Operation::RemoveLink { kind: LinkKind::Blocks, target: "other".into() }),
        ];
        let task = fold("abc", &ops).unwrap();
        assert_eq!(task.links.len(), 1);
        assert_eq!(task.links[0].kind, LinkKind::Relates);
    }

    #[test]
    fn parent_set_then_cleared() {
        let ops = vec![
            env(1, Operation::CreateTask { title: "T".into(), kind: TaskKind::Task, description: "".into() }),
            env(2, Operation::SetParent { parent: "epic123".into() }),
        ];
        let task = fold("abc", &ops).unwrap();
        assert_eq!(task.parent.as_deref(), Some("epic123"));

        let ops_cleared = vec![
            env(1, Operation::CreateTask { title: "T".into(), kind: TaskKind::Task, description: "".into() }),
            env(2, Operation::SetParent { parent: "epic123".into() }),
            env(3, Operation::ClearParent),
        ];
        let task_cleared = fold("abc", &ops_cleared).unwrap();
        assert_eq!(task_cleared.parent, None);
    }
}
