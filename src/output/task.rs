use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::domain::id;
use crate::domain::op::{LinkKind, OpEnvelope, Priority, TaskKind};
use crate::domain::task::Task;
use crate::identity;

#[derive(Serialize)]
pub struct LinkJson {
    pub kind: LinkKind,
    pub target: String,
    pub target_display_id: String,
}

#[derive(Serialize)]
pub struct CommentJson {
    pub id: u32,
    pub author: String,
    pub author_name: String,
    pub timestamp: i64,
    pub text: String,
    pub edited: bool,
}

/// The `Task` read model plus everything a frontend needs but can't derive itself:
/// `display_id`/`key` (from the repo's `refs/tasks/config` chain, via `id::display`) and every
/// `*_name` field (resolved from `identity::contributor_directory`, built once per repo by the
/// caller and threaded through here rather than re-walked per task).
#[derive(Serialize)]
pub struct TaskJson {
    pub id: String,
    pub display_id: String,
    pub key: String,
    pub title: String,
    pub description: String,
    pub kind: TaskKind,
    pub status: String,
    pub priority: Option<Priority>,
    pub assignee: Option<String>,
    pub assignee_name: Option<String>,
    pub reporter: String,
    pub reporter_name: String,
    pub labels: BTreeSet<String>,
    pub due: Option<String>,
    pub milestone: Option<String>,
    pub parent: Option<String>,
    pub parent_display_id: Option<String>,
    pub links: Vec<LinkJson>,
    pub comments: Vec<CommentJson>,
    pub deleted: bool,
    pub created: i64,
    pub updated: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<OpEnvelope>>,
}

impl TaskJson {
    /// `key` is this repo's effective address key (`project::effective_key_for`/
    /// `ProjectConfig::effective_key`) — every display/target id below is addressed under it,
    /// since links and parents are always within the same repo's task store. `include_history`
    /// is false for `ls` (default) and every mutation payload (keeps them small); true for
    /// `show`/`export`, which already carried the full op-chain before this shape existed.
    pub fn from_task(task: &Task, key: &str, directory: &HashMap<String, String>, include_history: bool) -> Self {
        let assignee_name = task.assignee.as_deref().map(|e| identity::display_name(directory, e));
        let reporter_name = identity::display_name(directory, &task.reporter);
        let parent_display_id = task.parent.as_deref().map(|p| id::display(key, p));
        let links = task
            .links
            .iter()
            .map(|l| LinkJson { kind: l.kind, target: l.target.clone(), target_display_id: id::display(key, &l.target) })
            .collect();
        let comments = task
            .comments
            .iter()
            .map(|c| CommentJson {
                id: c.id,
                author: c.author.clone(),
                author_name: identity::display_name(directory, &c.author),
                timestamp: c.timestamp,
                text: c.text.clone(),
                edited: c.edited,
            })
            .collect();

        TaskJson {
            id: task.id.clone(),
            display_id: id::display(key, &task.id),
            key: key.to_string(),
            title: task.title.clone(),
            description: task.description.clone(),
            kind: task.kind,
            status: task.status.clone(),
            priority: task.priority,
            assignee: task.assignee.clone(),
            assignee_name,
            reporter: task.reporter.clone(),
            reporter_name,
            labels: task.labels.clone(),
            due: task.due.clone(),
            milestone: task.milestone.clone(),
            parent: task.parent.clone(),
            parent_display_id,
            links,
            comments,
            deleted: task.deleted,
            created: task.created,
            updated: task.updated,
            history: include_history.then(|| task.history.clone()),
        }
    }
}
