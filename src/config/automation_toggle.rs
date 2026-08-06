use std::collections::BTreeMap;

/// Per-automation-name enable/disable overrides, keyed by the built-in's hyphenated name
/// (e.g. `"auto-sync"`). Lives on both `GlobalConfig` (per-machine) and `ProjectConfig`
/// (per-repo, synced) — same dual-scope shape as `config::fields::FieldMap`.
pub type AutomationOverrides = BTreeMap<String, bool>;

/// project override > global override > default-enabled(true) — mirrors `fields::resolve`'s
/// "project wins" precedence, just keyed by automation name instead of field name and resolved
/// one name at a time rather than merged into a struct (there's no fixed set of fields to
/// enumerate up front — `automation::builtins::NAMES` is the source of truth for what exists).
pub fn resolve_enabled(name: &str, global: &AutomationOverrides, project: &AutomationOverrides) -> bool {
    project.get(name).or_else(|| global.get(name)).copied().unwrap_or(true)
}

/// Which scope (if any) is responsible for the resolved state — for display only
/// (`config show`, `automation list`), same purpose as `cli::config::field_status`'s `source`.
pub fn source(name: &str, global: &AutomationOverrides, project: &AutomationOverrides) -> &'static str {
    if project.contains_key(name) {
        "project"
    } else if global.contains_key(name) {
        "global"
    } else {
        "default"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_configured_means_enabled_by_default() {
        assert!(resolve_enabled("auto-sync", &AutomationOverrides::new(), &AutomationOverrides::new()));
    }

    #[test]
    fn global_disable_applies_with_no_project_override() {
        let mut global = AutomationOverrides::new();
        global.insert("auto-sync".to_string(), false);
        assert!(!resolve_enabled("auto-sync", &global, &AutomationOverrides::new()));
    }

    #[test]
    fn project_overrides_global_for_same_name() {
        let mut global = AutomationOverrides::new();
        global.insert("auto-sync".to_string(), false);
        let mut project = AutomationOverrides::new();
        project.insert("auto-sync".to_string(), true);
        assert!(resolve_enabled("auto-sync", &global, &project), "project's true should win over global's false");
    }

    #[test]
    fn source_reports_which_scope_has_the_override() {
        let mut global = AutomationOverrides::new();
        global.insert("auto-sync".to_string(), false);
        let mut project = AutomationOverrides::new();
        project.insert("auto-unassign-done".to_string(), false);

        assert_eq!(source("auto-sync", &global, &project), "global");
        assert_eq!(source("auto-unassign-done", &global, &project), "project");
        assert_eq!(source("auto-something-else", &global, &project), "default");
    }
}
