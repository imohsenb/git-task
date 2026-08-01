use serde::{Deserialize, Serialize};

use crate::actor::Actor;

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
    SetPriority { priority: String },
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
