use crate::automation::rules::Rule;
use crate::config::automation_toggle::{self, AutomationOverrides};

/// Clears the assignee when a task's status is set to `"done"`. Modeled as a genuine embedded
/// `Rule` rather than a special-cased mechanism, so it runs through the exact same
/// evalexpr/action pipeline a hand-authored rule does.
pub const AUTO_UNASSIGN_DONE: &str = "auto-unassign-done";

/// Pushes and pulls in the background after any mutating command settles. Not a `Rule` — it's a
/// network side effect, not an `Operation`, so it's handled as a distinct final step in
/// `engine::run` (see `sync::trigger`) rather than through `parse_action`.
pub const AUTO_SYNC: &str = "auto-sync";

/// Every built-in automation name, in the order they're documented/displayed — also the source
/// of truth `automation::builtins::is_known` validates `automation enable/disable <name>` against.
pub const NAMES: &[&str] = &[AUTO_UNASSIGN_DONE, AUTO_SYNC];

pub fn is_known(name: &str) -> bool {
    NAMES.contains(&name)
}

fn unassign_done_rule() -> Rule {
    Rule {
        name: AUTO_UNASSIGN_DONE.to_string(),
        on: "status.changed".to_string(),
        when: Some(r#"status == "done""#.to_string()),
        actions: vec!["clear_assignee".to_string()],
    }
}

/// The `Rule`-shaped built-ins enabled for this repo (currently just `auto-unassign-done` —
/// `auto-sync` isn't a `Rule`, see its doc comment above), in catalog order. `engine::run`
/// prepends these ahead of global/project user rules, so local automation runs prior to
/// system-level automation within any single event's matching pass.
pub fn enabled_rules(global: &AutomationOverrides, project: &AutomationOverrides) -> Vec<Rule> {
    let mut rules = Vec::new();
    if automation_toggle::resolve_enabled(AUTO_UNASSIGN_DONE, global, project) {
        rules.push(unassign_done_rule());
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_by_default_includes_unassign_done() {
        let rules = enabled_rules(&AutomationOverrides::new(), &AutomationOverrides::new());
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, AUTO_UNASSIGN_DONE);
    }

    #[test]
    fn disabled_globally_excludes_it() {
        let mut global = AutomationOverrides::new();
        global.insert(AUTO_UNASSIGN_DONE.to_string(), false);
        assert!(enabled_rules(&global, &AutomationOverrides::new()).is_empty());
    }

    #[test]
    fn project_override_re_enables_over_global_disable() {
        let mut global = AutomationOverrides::new();
        global.insert(AUTO_UNASSIGN_DONE.to_string(), false);
        let mut project = AutomationOverrides::new();
        project.insert(AUTO_UNASSIGN_DONE.to_string(), true);
        assert_eq!(enabled_rules(&global, &project).len(), 1);
    }

    #[test]
    fn is_known_rejects_unrecognized_names() {
        assert!(is_known(AUTO_SYNC));
        assert!(!is_known("auto-something-else"));
    }
}
