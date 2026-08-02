use std::collections::{BTreeSet, HashSet, VecDeque};

use anyhow::{bail, Context as _, Result};
use evalexpr::{context_map, eval_boolean_with_context, HashMapContext};
use git2::Repository;

use crate::actor::Actor;
use crate::automation::rules::{self, Rule};
use crate::config::project::ProjectConfig;
use crate::domain::id::TaskId;
use crate::domain::op::{Operation, Priority, TaskKind};
use crate::domain::task::Task;
use crate::store::git_store::Store;

const MAX_ITERATIONS: usize = 20;

fn automation_actor() -> Actor {
    Actor {
        name: "git-task-automation".to_string(),
        email: "automation@git-task.local".to_string(),
    }
}

/// Runs automation rules after a mutation. `written_ops` are the ops the caller just
/// appended/created — used to derive which events just fired. Loads global
/// (`~/.config/git-task/automation.toml`) + this repo's (`refs/tasks/config`) rules.
/// A rule can only fire once per call (loop guard), and the whole run is capped at
/// `MAX_ITERATIONS` cascaded events as a backstop against rule cycles.
pub fn run(repo: &Repository, task_id: &TaskId, written_ops: &[Operation]) -> Result<()> {
    let mut all_rules = rules::load_global()?;
    all_rules.extend(ProjectConfig::load(repo)?.rules);
    if all_rules.is_empty() {
        return Ok(());
    }

    let store = Store::new(repo);
    let actor = automation_actor();

    let mut pending: VecDeque<&'static str> = events_for(written_ops).into_iter().collect();
    let mut fired: HashSet<String> = HashSet::new();
    let mut iterations = 0usize;

    while let Some(event) = pending.pop_front() {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            eprintln!("automation: stopped after {MAX_ITERATIONS} cascaded events (possible rule cycle)");
            break;
        }

        let task = store.load(task_id)?;
        let ctx = build_context(&task);

        for rule in &all_rules {
            if rule.on != event || fired.contains(&rule.name) {
                continue;
            }

            let is_match = match matches(rule, &ctx) {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("automation: rule '{}' condition error, skipping: {err:#}", rule.name);
                    false
                }
            };
            if !is_match {
                continue;
            }
            fired.insert(rule.name.clone());

            let mut ops = Vec::new();
            for action in &rule.actions {
                match parse_action(action) {
                    Ok(op) => ops.push(op),
                    Err(err) => {
                        eprintln!("automation: rule '{}' action '{action}' skipped: {err:#}", rule.name)
                    }
                }
            }
            if ops.is_empty() {
                continue;
            }

            println!("automation: rule '{}' fired ({} action(s))", rule.name, ops.len());
            pending.extend(events_for(&ops));
            store.append(task_id, &actor, ops)?;
        }
    }

    Ok(())
}

fn events_for(ops: &[Operation]) -> Vec<&'static str> {
    let mut events = BTreeSet::new();
    let mut is_create = false;
    for op in ops {
        match op {
            Operation::CreateTask { .. } => {
                events.insert("task.created");
                is_create = true;
            }
            Operation::SetStatus { .. } => {
                events.insert("status.changed");
            }
            Operation::AddComment { .. } => {
                events.insert("comment.added");
            }
            Operation::AddLabel { .. } => {
                events.insert("label.added");
            }
            _ => {}
        }
    }
    if !is_create {
        events.insert("task.updated");
    }
    events.into_iter().collect()
}

fn build_context(task: &Task) -> HashMapContext {
    context_map! {
        "kind" => task.kind.as_str().to_string(),
        "status" => task.status.clone(),
        "priority" => task.priority.map(|p| p.as_str().to_string()).unwrap_or_default(),
        "assignee" => task.assignee.clone().unwrap_or_default(),
        "title" => task.title.clone(),
    }
    .unwrap_or_default()
}

fn matches(rule: &Rule, ctx: &HashMapContext) -> Result<bool> {
    match rule.when.as_deref().map(str::trim) {
        None | Some("") => Ok(true),
        Some(expr) => eval_boolean_with_context(expr, ctx)
            .with_context(|| format!("evaluating condition for rule '{}'", rule.name)),
    }
}

fn parse_action(action: &str) -> Result<Operation> {
    let (verb, rest) = action.trim().split_once(' ').unwrap_or((action.trim(), ""));
    let value = strip_quotes(rest.trim());
    match verb {
        "set_priority" => Priority::from_str_loose(&value)
            .map(|priority| Operation::SetPriority { priority })
            .ok_or_else(|| anyhow::anyhow!("unknown priority '{value}'")),
        "set_status" => Ok(Operation::SetStatus { status: value }),
        "set_assignee" => Ok(Operation::SetAssignee { assignee: value }),
        "set_kind" => TaskKind::from_str_loose(&value)
            .map(|kind| Operation::SetKind { kind })
            .ok_or_else(|| anyhow::anyhow!("unknown kind '{value}'")),
        "add_label" => Ok(Operation::AddLabel { label: value }),
        "remove_label" => Ok(Operation::RemoveLabel { label: value }),
        "set_due" => Ok(Operation::SetDueDate { due: value }),
        "set_milestone" => Ok(Operation::SetMilestone { milestone: value }),
        "add_comment" => Ok(Operation::AddComment { text: value }),
        other => bail!("unknown automation action '{other}'"),
    }
}

fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_for_create_is_created_only_not_updated() {
        let ops = vec![Operation::CreateTask {
            title: "T".into(),
            kind: TaskKind::Task,
            description: "".into(),
        }];
        assert_eq!(events_for(&ops), vec!["task.created"]);
    }

    #[test]
    fn events_for_create_plus_label_fires_both() {
        let ops = vec![
            Operation::CreateTask { title: "T".into(), kind: TaskKind::Bug, description: "".into() },
            Operation::AddLabel { label: "x".into() },
        ];
        assert_eq!(events_for(&ops), vec!["label.added", "task.created"]);
    }

    #[test]
    fn events_for_non_create_always_includes_task_updated() {
        let ops = vec![Operation::SetPriority { priority: Priority::High }];
        assert_eq!(events_for(&ops), vec!["task.updated"]);
    }

    #[test]
    fn events_for_status_change_includes_both_specific_and_updated() {
        let ops = vec![Operation::SetStatus { status: "doing".into() }];
        let events = events_for(&ops);
        assert!(events.contains(&"status.changed"));
        assert!(events.contains(&"task.updated"));
    }

    #[test]
    fn parse_action_known_verbs() {
        assert!(matches!(
            parse_action("set_priority high").unwrap(),
            Operation::SetPriority { priority: Priority::High }
        ));
        assert!(matches!(
            parse_action("add_label triage").unwrap(),
            Operation::AddLabel { label } if label == "triage"
        ));
        assert!(matches!(
            parse_action("set_kind bug").unwrap(),
            Operation::SetKind { kind: TaskKind::Bug }
        ));
    }

    #[test]
    fn parse_action_strips_quotes_for_multi_word_values() {
        match parse_action("add_comment \"multi word note\"").unwrap() {
            Operation::AddComment { text } => assert_eq!(text, "multi word note"),
            other => panic!("expected AddComment, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_unknown_verb_errors() {
        assert!(parse_action("delete_everything now").is_err());
    }

    #[test]
    fn parse_action_unknown_kind_errors() {
        assert!(parse_action("set_kind not_a_real_kind").is_err());
    }

    #[test]
    fn parse_action_unknown_priority_errors() {
        assert!(parse_action("set_priority not_a_real_priority").is_err());
    }

    #[test]
    fn matches_no_condition_is_always_true() {
        let rule = Rule { name: "r".into(), on: "task.created".into(), when: None, actions: vec![] };
        let ctx = context_map! { "kind" => "bug" }.unwrap();
        assert!(matches(&rule, &ctx).unwrap());
    }

    #[test]
    fn matches_evaluates_condition_against_context() {
        let rule = Rule {
            name: "r".into(),
            on: "task.created".into(),
            when: Some("kind == \"bug\"".into()),
            actions: vec![],
        };
        let bug_ctx = context_map! { "kind" => "bug" }.unwrap();
        let story_ctx = context_map! { "kind" => "story" }.unwrap();
        assert!(matches(&rule, &bug_ctx).unwrap());
        assert!(!matches(&rule, &story_ctx).unwrap());
    }

    #[test]
    fn matches_bad_condition_errors_rather_than_panics() {
        let rule = Rule {
            name: "r".into(),
            on: "task.created".into(),
            when: Some("kind === bug".into()),
            actions: vec![],
        };
        let ctx = context_map! { "kind" => "bug" }.unwrap();
        assert!(matches(&rule, &ctx).is_err());
    }
}
