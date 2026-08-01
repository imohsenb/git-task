use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(default)]
    pub required: bool,
}

pub type FieldMap = BTreeMap<String, FieldSpec>;

#[derive(Debug, Clone, Default)]
pub struct RequiredFields {
    pub priority: bool,
    pub assignee: bool,
    pub due: bool,
}

/// Merges global and per-project field specs (project wins on conflict) into
/// the concrete set of fields `new` must fill before a task can be created.
pub fn resolve(global: &FieldMap, project: &FieldMap) -> RequiredFields {
    let mut merged = global.clone();
    for (name, spec) in project {
        merged.insert(name.clone(), spec.clone());
    }

    RequiredFields {
        priority: merged.get("priority").is_some_and(|f| f.required),
        assignee: merged.get("assignee").is_some_and(|f| f.required),
        due: merged.get("due").is_some_and(|f| f.required),
    }
}
