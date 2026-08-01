use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::domain::op::{OpEnvelope, TaskKind};

pub const DEFAULT_STATUS: &str = "todo";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: u32,
    pub author: Actor,
    pub timestamp: i64,
    pub text: String,
    pub edited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: TaskKind,
    pub status: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub reporter: Actor,
    pub labels: BTreeSet<String>,
    pub due: Option<String>,
    pub comments: Vec<Comment>,
    pub created: i64,
    pub updated: i64,
    pub history: Vec<OpEnvelope>,
}
