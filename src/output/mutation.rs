use std::collections::HashMap;

use serde::Serialize;

use crate::automation::engine::AutomationEvent;
use crate::domain::op::Operation;
use crate::domain::task::Task;
use crate::output::TaskJson;

/// The one shape every mutating command (`new`, `edit`, `status`, `comment`, `label`, `epic`,
/// `link`, `delete`) returns under `--format json`. `task` is loaded *after* automation has run —
/// a rule can change the state the user just set, and the caller needs to see the final result,
/// not the one it asked for. `ops` are just the tags of the ops the user's own action appended
/// (`automation`'s own ops live in each `AutomationEvent.ops` instead, so a caller can tell the
/// two apart). `created` is only `Some(true)` for `new`.
#[derive(Serialize)]
pub struct MutationJson {
    pub task: TaskJson,
    pub ops: Vec<String>,
    pub automation: Vec<AutomationEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<bool>,
}

/// Builds and prints a `MutationJson` in one call — every mutating command's JSON branch is
/// otherwise identical boilerplate (build `TaskJson`, tag the ops, attach automation, print).
pub fn print_mutation(
    task: &Task,
    key: &str,
    directory: &HashMap<String, String>,
    ops: &[Operation],
    automation: Vec<AutomationEvent>,
    created: Option<bool>,
) {
    super::print_ok(MutationJson {
        task: TaskJson::from_task(task, key, directory, false),
        ops: ops.iter().map(|op| op.tag().to_string()).collect(),
        automation,
        created,
    });
}
