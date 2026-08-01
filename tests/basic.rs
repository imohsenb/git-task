mod common;

use common::TestRepo;

#[test]
fn create_show_edit_status_comment_label_roundtrip() {
    let repo = TestRepo::new();

    let out = repo.run(&["new", "Fix login timeout", "--kind", "bug", "--desc", "token bug", "--priority", "high"]);
    let id = TestRepo::extract_id(&out);

    let show = repo.run(&["show", &id]);
    assert!(show.contains("Fix login timeout"));
    assert!(show.contains("Bug"));
    assert!(show.contains("todo")); // default status
    assert!(show.contains("high"));

    repo.run(&["status", &id, "doing"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("doing"));

    repo.run(&["edit", &id, "--assignee", "alice"]);
    let show = repo.run(&["show", &id]);
    assert!(show.contains("alice"));

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
    assert_eq!(value["title"], "JSON task");
    assert_eq!(value["kind"], "story");
    assert_eq!(value["status"], "todo");
    assert!(value["id"].as_str().unwrap().len() == 40);
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
    repo.run(&["key", "SRV"]);
    std::fs::write(
        repo.path().join(".gittask/config.toml"),
        "key = \"SRV\"\n\n[[rule]]\nname = \"triage\"\non = \"task.created\"\nwhen = \"kind == \\\"bug\\\"\"\ndo = [\"set_priority high\", \"add_label triage\"]\n",
    )
    .unwrap();

    let bug_out = repo.run(&["new", "A bug", "--kind", "bug", "--desc", "d"]);
    let bug_id = TestRepo::extract_id(&bug_out);
    let show = repo.run(&["show", &bug_id]);
    assert!(show.contains("high"));
    assert!(show.contains("triage"));

    let story_out = repo.run(&["new", "A story", "--kind", "story", "--desc", "d"]);
    let story_id = TestRepo::extract_id(&story_out);
    let show = repo.run(&["show", &story_id]);
    assert!(!show.contains("Priority"), "rule should not have fired for a non-bug: {show}");
}
