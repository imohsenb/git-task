use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::op::{LinkKind, OpEnvelope, Priority, TaskKind};

pub const DEFAULT_STATUS: &str = "todo";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: u32,
    /// Email of the commenter — same addressing as `Task::assignee`; resolve a display name
    /// via `identity::display_name`/`full_display` rather than baking one in here.
    pub author: String,
    pub timestamp: i64,
    pub text: String,
    pub edited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub kind: LinkKind,
    pub target: String,
    /// `None` for a same-repo link; `Some(repo)` (an origin URL, preferred, or a local path
    /// fallback) for a cross-repo one. See `Operation::AddLink`.
    pub target_repo: Option<String>,
    /// Cross-repo only: the raw text the user typed for the target, kept for display.
    pub target_label: Option<String>,
}

impl Link {
    /// Same repo reference, tolerant of protocol/host-form differences — `None == None`
    /// (both same-repo), or both `Some` and `domain::remote::normalize` agrees. Used for
    /// link dedup/removal matching instead of raw string/derived equality, so a link added
    /// via one URL form of a repo (ssh) can be recognized — and removed — via any
    /// equivalent form (https, with/without `.git`).
    pub fn same_target_repo(a: &Option<String>, b: &Option<String>) -> bool {
        crate::domain::remote::same(a, b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: TaskKind,
    pub status: String,
    pub priority: Option<Priority>,
    pub assignee: Option<String>,
    /// Email of whoever ran `CreateTask` — resolved to a display name the same way as
    /// `assignee` and `Comment::author`, never baked in here.
    pub reporter: String,
    pub labels: BTreeSet<String>,
    pub fixed_versions: BTreeSet<String>,
    pub affected_versions: BTreeSet<String>,
    pub due: Option<String>,
    /// `None` parent, or a same-repo parent: the resolved local epic id (`parent_repo` is
    /// `None`). A cross-repo parent stores the epic's id as resolved *in the target repo* at
    /// link time (see `Operation::SetParent`) here, with `parent_repo`/`parent_label` set.
    pub parent: Option<String>,
    /// `None` for a same-repo parent (or no parent); `Some(repo)` (an origin URL, preferred,
    /// or a local path fallback) for a cross-repo one — same shape as `Link::target_repo`.
    pub parent_repo: Option<String>,
    /// Cross-repo only: the raw text the user typed for the epic, kept for display since the
    /// display key/address scheme of the target repo isn't known locally.
    pub parent_label: Option<String>,
    pub links: Vec<Link>,
    pub milestone: Option<String>,
    pub comments: Vec<Comment>,
    /// Set by `Operation::DeleteTask`. A soft delete — the task keeps folding normally and
    /// stays addressable by id; callers that should hide deleted tasks (`ls`) filter on this.
    pub deleted: bool,
    pub created: i64,
    pub updated: i64,
    pub history: Vec<OpEnvelope>,
}
