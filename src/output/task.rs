use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::domain::id;
use crate::domain::op::{LinkKind, OpEnvelope, Priority, TaskKind};
use crate::domain::task::Task;
use crate::identity;

#[derive(Serialize)]
pub struct LinkJson {
    pub kind: LinkKind,
    /// The resolved local task id for a same-repo link; `null` for a cross-repo link (v1
    /// never resolves the target locally — see `target_repo`).
    pub target: Option<String>,
    /// Same-repo: `id::display(key, target)`. Cross-repo: the raw text the user typed for
    /// the target (`target_label`), since there's no local task to compute a real display
    /// id from.
    pub target_display_id: String,
    /// `null` for a same-repo link. For a cross-repo link, the target repo's `origin`
    /// remote URL (preferred) or a local filesystem path (fallback).
    pub target_repo: Option<String>,
}

/// One task whose `parent` is this one — `show` populates this by scanning (see
/// `cli::show::collect_children`), since nothing on the parent task itself records its
/// children; a mutation/`ls` payload always leaves this empty rather than paying for that scan.
#[derive(Serialize, Clone)]
pub struct ChildJson {
    /// The resolved local task id for a same-repo child; `null` for a cross-repo one (same
    /// convention as `LinkJson::target`).
    pub id: Option<String>,
    /// Same-repo: `id::display` under the current repo's key. Cross-repo: `id::display` under
    /// *its own* repo's key — unlike a cross-repo `Link`'s target, the child repo is one we
    /// actually opened to find this child, so its real display id is known, not just a label.
    pub display_id: String,
    pub title: String,
    pub kind: TaskKind,
    pub status: String,
    /// `null` for a same-repo child. For a cross-repo child, the repo it lives in: its
    /// registered name.
    pub repo: Option<String>,
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
    pub fixed_versions: BTreeSet<String>,
    pub affected_versions: BTreeSet<String>,
    pub due: Option<String>,
    pub milestone: Option<String>,
    pub parent: Option<String>,
    pub parent_display_id: Option<String>,
    /// `null` for a same-repo parent (or none). For a cross-repo parent, the target repo's
    /// `origin` remote URL (preferred) or a local filesystem path (fallback) — same shape as
    /// `LinkJson::target_repo`.
    pub parent_repo: Option<String>,
    /// Always empty from `from_task` itself — `show` fills it in afterward (see `ChildJson`);
    /// every other caller (`ls`, mutation payloads, `export`) leaves it empty rather than
    /// paying for the scan that finding children requires.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ChildJson>,
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
    /// `ProjectConfig::effective_key`) — every same-repo display/target id below is addressed
    /// under it; a cross-repo link/parent instead carries its own `target_repo`/`parent_repo`
    /// and shows the raw label the user typed, since there's no local id to address. `include_history`
    /// is false for `ls` (default) and every mutation payload (keeps them small); true for
    /// `show`/`export`, which already carried the full op-chain before this shape existed.
    pub fn from_task(task: &Task, key: &str, directory: &HashMap<String, String>, include_history: bool) -> Self {
        let assignee_name = task.assignee.as_deref().map(|e| identity::display_name(directory, e));
        let reporter_name = identity::display_name(directory, &task.reporter);
        let parent_display_id = task.parent.as_deref().map(|p| match &task.parent_repo {
            None => id::display(key, p),
            Some(_) => task.parent_label.clone().unwrap_or_else(|| p.to_string()),
        });
        let links = task
            .links
            .iter()
            .map(|l| match &l.target_repo {
                None => LinkJson {
                    kind: l.kind,
                    target: Some(l.target.clone()),
                    target_display_id: id::display(key, &l.target),
                    target_repo: None,
                },
                Some(repo) => LinkJson {
                    kind: l.kind,
                    target: None,
                    target_display_id: l.target_label.clone().unwrap_or_else(|| l.target.clone()),
                    target_repo: Some(repo.clone()),
                },
            })
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
            fixed_versions: task.fixed_versions.clone(),
            affected_versions: task.affected_versions.clone(),
            due: task.due.clone(),
            milestone: task.milestone.clone(),
            parent: task.parent.clone(),
            parent_display_id,
            parent_repo: task.parent_repo.clone(),
            children: Vec::new(),
            links,
            comments,
            deleted: task.deleted,
            created: task.created,
            updated: task.updated,
            history: include_history.then(|| task.history.clone()),
        }
    }
}
