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
    SetAssignee { assignee: String },
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
}
