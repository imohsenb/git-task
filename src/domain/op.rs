use serde::{Deserialize, Serialize};

use crate::actor::Actor;

/// A closed low/medium/high scale — unlike `status` (genuinely free-form, no fixed
/// vocabulary; see `color::status_semantic`), priority has always meant one of three tiers
/// in every part of the UI that reads it, so it gets a real enum instead of a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }

    /// Case-insensitive, with a few aliases for values this field accepted back when it was a
    /// free-form string (`critical`/`urgent` read as `high`, `normal` as `medium`) — existing
    /// tasks with those values in their event history must keep loading.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" | "minor" => Some(Priority::Low),
            "medium" | "normal" | "med" => Some(Priority::Medium),
            "high" | "critical" | "urgent" => Some(Priority::High),
            _ => None,
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Custom rather than derived: historical `SetPriority` ops (from when this field was a free
/// string) may hold values like `"Critical"` or `"normal"` — `from_str_loose` accepts those so
/// old task histories keep folding instead of erroring on load.
impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Priority::from_str_loose(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid priority '{s}' (expected low/medium/high)")))
    }
}

/// A single op-package entry: one mutation plus who made it and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpEnvelope {
    pub author: Actor,
    pub timestamp: i64,
    #[serde(flatten)]
    pub op: Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Bug,
    Story,
    Task,
    Epic,
    Subtask,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Bug => "bug",
            TaskKind::Story => "story",
            TaskKind::Task => "task",
            TaskKind::Epic => "epic",
            TaskKind::Subtask => "subtask",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bug" => Some(TaskKind::Bug),
            "story" => Some(TaskKind::Story),
            "task" => Some(TaskKind::Task),
            "epic" => Some(TaskKind::Epic),
            "subtask" => Some(TaskKind::Subtask),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Blocks,
    Relates,
    #[value(name = "dup")]
    #[serde(rename = "dup")]
    Duplicates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Operation {
    CreateTask {
        title: String,
        kind: TaskKind,
        description: String,
    },
    SetTitle { title: String },
    SetDescription { description: String },
    SetKind { kind: TaskKind },
    SetStatus { status: String },
    SetPriority { priority: Priority },
    /// `email` used to be a bare `assignee: String` (whatever text the CLI was given — a name,
    /// a handle, anything). Assignment now requires a real email (`identity::validate_email`),
    /// the one identity that stays stable across a distributed, event-sourced task store; the
    /// display name is resolved separately, from `identity::contributor_directory`, so it's
    /// never baked into the op. `#[serde(alias)]` keeps old op-chains loading under the
    /// original field name — whatever they hold just won't resolve to a name.
    SetAssignee {
        #[serde(alias = "assignee")]
        email: String,
    },
    AddLabel { label: String },
    RemoveLabel { label: String },
    AddComment { text: String },
    EditComment { comment_id: u32, text: String },
    SetDueDate { due: String },
    SetParent { parent: String },
    ClearParent,
    SetMilestone { milestone: String },
    AddLink { kind: LinkKind, target: String },
    RemoveLink { kind: LinkKind, target: String },
    /// Unsets `assignee`/`priority`/`due`/`milestone` — the `ClearParent` pattern extended to
    /// every other optional field. Each is its own variant (not a generic `ClearField { field }`)
    /// so `fold` stays exhaustive-match-checked against `Task`'s actual optional fields.
    ClearAssignee,
    ClearPriority,
    ClearDueDate,
    ClearMilestone,
    /// Soft delete: an ordinary event, appended to the chain like any other op, so it syncs
    /// via the normal push/pull/merge path and stays in `history`. There is no `RestoreTask`
    /// counterpart — once recorded it's meant to stick; `store::Store::drop` (the `drop` CLI
    /// command) is the separate, local-only, non-syncing hard delete for when the task
    /// shouldn't exist at all.
    DeleteTask,
}

impl Operation {
    /// The bare variant name (`"SetStatus"`, `"CreateTask"`, ...) — used for commit-message
    /// summaries (`store::git_store::op_summary`) and the `ops`/ `automation[].ops` fields of
    /// the `--format json` mutation payload, so both stay in lockstep with one source of truth.
    pub fn tag(&self) -> &'static str {
        match self {
            Operation::CreateTask { .. } => "CreateTask",
            Operation::SetTitle { .. } => "SetTitle",
            Operation::SetDescription { .. } => "SetDescription",
            Operation::SetKind { .. } => "SetKind",
            Operation::SetStatus { .. } => "SetStatus",
            Operation::SetPriority { .. } => "SetPriority",
            Operation::SetAssignee { .. } => "SetAssignee",
            Operation::AddLabel { .. } => "AddLabel",
            Operation::RemoveLabel { .. } => "RemoveLabel",
            Operation::AddComment { .. } => "AddComment",
            Operation::EditComment { .. } => "EditComment",
            Operation::SetDueDate { .. } => "SetDueDate",
            Operation::SetParent { .. } => "SetParent",
            Operation::ClearParent => "ClearParent",
            Operation::SetMilestone { .. } => "SetMilestone",
            Operation::AddLink { .. } => "AddLink",
            Operation::RemoveLink { .. } => "RemoveLink",
            Operation::ClearAssignee => "ClearAssignee",
            Operation::ClearPriority => "ClearPriority",
            Operation::ClearDueDate => "ClearDueDate",
            Operation::ClearMilestone => "ClearMilestone",
            Operation::DeleteTask => "DeleteTask",
        }
    }
}
