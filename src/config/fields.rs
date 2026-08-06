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

#[cfg(test)]
mod tests {
    use super::*;

    fn required(name: &str) -> FieldMap {
        let mut map = FieldMap::new();
        map.insert(name.to_string(), FieldSpec { required: true });
        map
    }

    #[test]
    fn nothing_configured_means_nothing_required() {
        let r = resolve(&FieldMap::new(), &FieldMap::new());
        assert!(!r.priority && !r.assignee && !r.due);
    }

    #[test]
    fn global_required_applies_with_no_project_override() {
        let r = resolve(&required("priority"), &FieldMap::new());
        assert!(r.priority);
        assert!(!r.assignee);
    }

    #[test]
    fn project_overrides_global_for_same_field() {
        let global = required("priority");
        let mut project = FieldMap::new();
        project.insert("priority".to_string(), FieldSpec { required: false });
        let r = resolve(&global, &project);
        assert!(!r.priority, "project's required=false should win over global's true");
    }

    #[test]
    fn project_can_add_a_requirement_global_does_not_have() {
        let r = resolve(&FieldMap::new(), &required("assignee"));
        assert!(r.assignee);
        assert!(!r.priority);
    }
}
