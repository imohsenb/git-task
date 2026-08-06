use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::automation::rules::Rule;
use crate::config::fields::FieldSpec;
use crate::config::project::ProjectConfig;

/// Blob name for the per-repo config op-chain, stored under `refs/tasks/config`. Deliberately
/// distinct from tasks' `ops.json` so a stray task `load` can never mis-read config, and vice-versa.
pub const BLOB_NAME: &str = "config-ops.json";

/// The only mutations that can be applied to a repo's config — the config analogue of
/// `domain::op::Operation`. Stored as an event-sourced op-chain and folded into a
/// `ProjectConfig` by `fold`, so two clones editing config independently reconcile through the
/// same DAG merge as tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum ConfigOp {
    SetKey { key: String },
    /// Explicit bool (not remove-on-false) so a project can force a field *optional* over a
    /// global-required default — `config::fields::resolve` honours a project `required = false`.
    SetFieldRequired { field: String, required: bool },
    UpsertRule { rule: Rule },
    RemoveRule { name: String },
    /// Per-repo override for a built-in automation (`automation::builtins::NAMES`) — the
    /// project-scoped half of the dual-scope toggle; `config::global::GlobalConfig::automation`
    /// is the `--global` half, and this one wins when both are set for the same name.
    SetAutomationEnabled { name: String, enabled: bool },
}

/// One config op plus who made it and when — mirrors `domain::op::OpEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOpEnvelope {
    pub author: Actor,
    pub timestamp: i64,
    #[serde(flatten)]
    pub op: ConfigOp,
}

/// Replays config ops (already in causal order) into the derived `ProjectConfig` read model.
pub fn fold(envelopes: &[ConfigOpEnvelope]) -> ProjectConfig {
    let mut cfg = ProjectConfig::default();
    for env in envelopes {
        match &env.op {
            ConfigOp::SetKey { key } => cfg.key = Some(key.clone()),
            ConfigOp::SetFieldRequired { field, required } => {
                cfg.fields.insert(field.clone(), FieldSpec { required: *required });
            }
            ConfigOp::UpsertRule { rule } => {
                match cfg.rules.iter_mut().find(|r| r.name == rule.name) {
                    Some(existing) => *existing = rule.clone(),
                    None => cfg.rules.push(rule.clone()),
                }
            }
            ConfigOp::RemoveRule { name } => cfg.rules.retain(|r| &r.name != name),
            ConfigOp::SetAutomationEnabled { name, enabled } => {
                cfg.automation.insert(name.clone(), *enabled);
            }
        }
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(op: ConfigOp) -> ConfigOpEnvelope {
        ConfigOpEnvelope {
            author: Actor { name: "t".into(), email: "t@t".into() },
            timestamp: 0,
            op,
        }
    }

    fn rule(name: &str, action: &str) -> Rule {
        Rule { name: name.into(), on: "task.created".into(), when: None, actions: vec![action.into()] }
    }

    #[test]
    fn set_key_last_writer_wins() {
        let cfg = fold(&[
            env(ConfigOp::SetKey { key: "AAA".into() }),
            env(ConfigOp::SetKey { key: "BBB".into() }),
        ]);
        assert_eq!(cfg.key.as_deref(), Some("BBB"));
    }

    #[test]
    fn field_optional_overrides_earlier_required() {
        let cfg = fold(&[
            env(ConfigOp::SetFieldRequired { field: "priority".into(), required: true }),
            env(ConfigOp::SetFieldRequired { field: "priority".into(), required: false }),
        ]);
        assert!(!cfg.fields.get("priority").unwrap().required);
    }

    #[test]
    fn upsert_replaces_rule_by_name_not_append() {
        let cfg = fold(&[
            env(ConfigOp::UpsertRule { rule: rule("triage", "add_label a") }),
            env(ConfigOp::UpsertRule { rule: rule("triage", "add_label b") }),
        ]);
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(cfg.rules[0].actions, vec!["add_label b".to_string()]);
    }

    #[test]
    fn remove_rule_drops_by_name() {
        let cfg = fold(&[
            env(ConfigOp::UpsertRule { rule: rule("a", "add_label x") }),
            env(ConfigOp::UpsertRule { rule: rule("b", "add_label y") }),
            env(ConfigOp::RemoveRule { name: "a".into() }),
        ]);
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(cfg.rules[0].name, "b");
    }

    #[test]
    fn set_automation_enabled_last_writer_wins_by_name() {
        let cfg = fold(&[
            env(ConfigOp::SetAutomationEnabled { name: "auto-sync".into(), enabled: false }),
            env(ConfigOp::SetAutomationEnabled { name: "auto-sync".into(), enabled: true }),
        ]);
        assert_eq!(cfg.automation.get("auto-sync"), Some(&true));
    }
}
