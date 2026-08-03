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
    pub parent: Option<String>,
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
