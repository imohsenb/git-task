mod common;

use common::TestRepo;

#[test]
fn create_show_edit_status_comment_label_roundtrip() {
    let repo = TestRepo::new();

    let out = repo.run(&["new", "Fix login timeout", "--kind", "bug", "--desc", "token bug", "--priority", "high"]);
    let id = TestRepo::extract_id(&out);

    let show = repo.run(&["show", &id]);
    assert!(show.contains("Fix login timeout"));
    assert!(show.contains("BUG")); // kind badge is uppercased
    assert!(show.contains("TODO")); // default status, badge uppercased
    assert!(show.contains("HIGH")); // priority badge uppercased

    repo.run(&["status", &id, "doing"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("DOING"));

    repo.run(&["edit", &id, "--assignee", "alice@example.com"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("alice@example.com"));

    repo.run(&["comment", &id, "investigating"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("investigating"));
    assert!(show.contains("#1"));

    repo.run(&["comment", &id, "--edit", "1", "investigating further"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("investigating further"));
    assert!(show.contains("(edited)"));

    repo.run(&["label", &id, "add", "urgent"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("urgent"));

    repo.run(&["label", &id, "rm", "urgent"]);
    let show = repo.run(&["show", &id]);
    assert!(!show.contains("urgent"));
}

#[test]
fn show_json_round_trips_full_task_shape() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "JSON task", "--kind", "story", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let json = repo.run(&["show", &id, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "show");
    let task = &value["data"];
    assert_eq!(task["title"], "JSON task");
    assert_eq!(task["kind"], "story");
    assert_eq!(task["status"], "todo");
    assert!(task["id"].as_str().unwrap().len() == 40);
    assert_eq!(task["reporter"], "test@example.com");
    assert_eq!(task["reporter_name"], "Test User");
    assert!(task["display_id"].as_str().unwrap().starts_with(task["key"].as_str().unwrap()));
    assert!(task["history"].as_array().is_some(), "show should include full history");
}

#[test]
fn bare_hash_and_key_hash_addressing_resolve_identically() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "Addressing test", "--desc", "d"]);
    let key_id = TestRepo::extract_id(&out);
    let bare_hash = key_id.split_once('-').unwrap().1;

    let by_key = repo.run(&["show", &key_id, "--format", "json"]);
    let by_hash = repo.run(&["show", bare_hash, "--format", "json"]);
    assert_eq!(by_key, by_hash);
}

#[test]
fn delete_hides_from_ls_but_stays_addressable_and_synced_history() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "Doomed task", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    repo.run(&["delete", &id]);

    let ls = repo.run(&["ls"]);
    assert!(!ls.contains("Doomed task"), "deleted task should be hidden from default ls");

    let ls_deleted = repo.run(&["ls", "--deleted"]);
    assert!(ls_deleted.contains("Doomed task"), "--deleted should still show it");
    assert!(ls_deleted.contains("DELETED"));

    let show = repo.run(&["show", &id]);
    assert!(show.contains("DELETED"), "show should still work and flag the task as deleted");

    let err = repo.run_err(&["delete", &id]);
    assert!(err.contains("already deleted"), "unexpected error: {err}");
}

#[test]
fn drop_requires_force_and_removes_the_local_ref() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "Gone for good", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let err = repo.run_err(&["drop", &id]);
    assert!(err.contains("--force"), "unexpected error: {err}");

    repo.run(&["drop", &id, "--force"]);
    let err = repo.run_err(&["show", &id]);
    assert!(!err.is_empty());
}

#[test]
fn duplicate_label_is_rejected() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "T", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    repo.run(&["label", &id, "add", "x"]);
    let err = repo.run_err(&["label", &id, "add", "x"]);
    assert!(err.contains("already has label"), "unexpected error: {err}");
}

#[test]
fn edit_with_no_flags_is_rejected() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "T", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let err = repo.run_err(&["edit", &id]);
    assert!(err.contains("nothing to edit"), "unexpected error: {err}");
}

#[test]
fn new_without_title_or_description_fails_fast_when_not_interactive() {
    let repo = TestRepo::new();
    let err = repo.run_err(&["new"]);
    assert!(err.contains("title"), "expected missing-title error, got: {err}");
    assert!(err.contains("description"), "expected missing-description error, got: {err}");
}

#[test]
fn no_working_tree_files_are_created() {
    let repo = TestRepo::new();
    repo.run(&["new", "Should not touch worktree", "--desc", "d"]);

    let entries: Vec<_> = std::fs::read_dir(repo.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name != ".git")
        .collect();
    assert!(entries.is_empty(), "expected only .git in the working tree, found: {entries:?}");
}

#[test]
fn ls_filters_by_status_and_kind() {
    let repo = TestRepo::new();
    let bug_out = repo.run(&["new", "A bug", "--kind", "bug", "--desc", "d"]);
    let bug_id = TestRepo::extract_id(&bug_out);
    repo.run(&["new", "A story", "--kind", "story", "--desc", "d"]);
    repo.run(&["status", &bug_id, "doing"]);

    let doing = repo.run(&["ls", "--status", "doing"]);
    assert!(doing.contains("A bug"));
    assert!(!doing.contains("A story"));

    let stories = repo.run(&["ls", "--kind", "story"]);
    assert!(stories.contains("A story"));
    assert!(!stories.contains("A bug"));
}

#[test]
fn epic_and_link_relationships() {
    let repo = TestRepo::new();
    let epic_out = repo.run(&["new", "Epic", "--kind", "epic", "--desc", "d"]);
    let epic_id = TestRepo::extract_id(&epic_out);
    let child_out = repo.run(&["new", "Child", "--desc", "d"]);
    let child_id = TestRepo::extract_id(&child_out);

    repo.run(&["epic", &epic_id, "add", &child_id]);
    let show = repo.run(&["show", &child_id]);
    assert!(show.contains("Parent"));

    let ls = repo.run(&["ls", "--parent", &epic_id]);
    assert!(ls.contains("Child"));

    repo.run(&["epic", &epic_id, "rm", &child_id]);
    let show = repo.run(&["show", &child_id]);
    assert!(!show.contains("Parent"));

    let other_out = repo.run(&["new", "Other", "--desc", "d"]);
    let other_id = TestRepo::extract_id(&other_out);
    repo.run(&["link", &child_id, "add", "blocks", &other_id]);
    let show = repo.run(&["show", &child_id]);
    assert!(show.contains("Blocks"));
}

#[test]
fn automation_rule_fires_on_matching_creation() {
    let repo = TestRepo::new();
    repo.run(&["config", "key", "SRV"]);
    // Rules are configured via the CLI (event-sourced into refs/tasks/config), not a file.
    repo.run(&[
        "config",
        "rule",
        "add",
        "--name",
        "triage",
        "--on",
        "task.created",
        "--when",
        "kind == \"bug\"",
        "--do",
        "set_priority high",
        "--do",
        "add_label triage",
    ]);

    let bug_out = repo.run(&["new", "A bug", "--kind", "bug", "--desc", "d"]);
    let bug_id = TestRepo::extract_id(&bug_out);
    let show = repo.run(&["show", &bug_id]);
    assert!(show.contains("HIGH")); // priority badge uppercased
    assert!(show.contains("triage"));

    let story_out = repo.run(&["new", "A story", "--kind", "story", "--desc", "d"]);
    let story_id = TestRepo::extract_id(&story_out);
    let show = repo.run(&["show", &story_id]);
    assert!(!show.contains("Priority"), "rule should not have fired for a non-bug: {show}");
}

/// The regression `--format json` exists to prevent: a mutation whose automation rule fires
/// used to `println!` its own "automation: rule 'x' fired" line straight to stdout, which would
/// land ahead of (and corrupt) the JSON document below it.
#[test]
fn mutation_json_is_exactly_one_document_even_when_automation_fires() {
    let repo = TestRepo::new();
    repo.run(&["config", "key", "SRV"]);
    repo.run(&[
        "config", "rule", "add", "--name", "triage", "--on", "task.created", "--when", "kind == \"bug\"", "--do",
        "add_label triage",
    ]);

    let out = repo.run(&["new", "A bug", "--kind", "bug", "--desc", "d", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("stdout must be exactly one JSON document");
    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "new");
    assert_eq!(value["data"]["created"], true);
    assert_eq!(value["data"]["ops"][0], "CreateTask");
    assert_eq!(value["data"]["task"]["labels"][0], "triage", "task must reflect state AFTER automation settles");
    assert_eq!(value["data"]["automation"][0]["rule"], "triage");
    assert_eq!(value["data"]["automation"][0]["ops"][0], "AddLabel");
    assert!(value["data"]["task"]["history"].is_null(), "mutation payloads omit history");
}

#[test]
fn edit_json_reports_updated_task_and_ops() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "Editable", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let out = repo.run(&["edit", &id, "--title", "Edited", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["task"]["title"], "Edited");
    assert_eq!(value["data"]["ops"][0], "SetTitle");
    assert!(value["data"]["created"].is_null(), "created is only present for `new`");
}

#[test]
fn config_json_reports_key_fields_and_rules() {
    let repo = TestRepo::new();
    repo.run(&["config", "key", "SRV"]);
    repo.run(&["config", "field", "priority", "required"]);
    repo.run(&[
        "config", "rule", "add", "--name", "triage", "--on", "task.created", "--do", "add_label triage",
    ]);

    let out = repo.run(&["config", "show", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["key"], "SRV");
    assert_eq!(value["data"]["key_source"], "config");
    assert_eq!(value["data"]["fields"]["priority"]["required"], true);
    assert_eq!(value["data"]["fields"]["priority"]["source"], "repo");
    assert_eq!(value["data"]["fields"]["assignee"]["source"], "default");
    assert_eq!(value["data"]["rules"][0]["name"], "triage");
    assert_eq!(value["data"]["rules"][0]["scope"], "repo");

    // Mutations return the same shape nested under "config".
    let out = repo.run(&["config", "field", "due", "required", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["config"]["fields"]["due"]["required"], true);
}

#[test]
fn projects_json_lists_projects_and_default() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);

    let out = repo.run(&["projects", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["default_project"], "main");
    let projects = value["data"]["projects"].as_array().unwrap();
    let backend = projects.iter().find(|p| p["name"] == "backend").expect("backend project present");
    assert_eq!(backend["repos"][0].as_str().is_some(), true);
}
