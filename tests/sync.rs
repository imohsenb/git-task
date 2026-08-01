mod common;

use std::process::Command as StdCommand;

use common::TestRepo;

fn init_bare(bare_dir: &std::path::Path) {
    let status = StdCommand::new("git").args(["init", "-q", "--bare"]).arg(bare_dir).status().unwrap();
    assert!(status.success());
}

#[test]
fn push_pull_new_task_and_fast_forward() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());

    let alice = TestRepo::new();
    alice.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    std::fs::write(alice.path().join(".gitkeep"), "").unwrap();
    alice.git(&["add", ".gitkeep"]);
    alice.git(&["commit", "-q", "-m", "init"]);
    alice.git(&["branch", "-M", "main"]);
    alice.git(&["push", "-q", "-u", "origin", "main"]);

    let bob = TestRepo::clone_from(bare.path(), "bob");

    alice.run(&["key", "SRV"]);
    let out = alice.run(&["new", "Shared task", "--desc", "initial"]);
    let id = TestRepo::extract_id(&out);
    alice.run(&["push"]);

    bob.run(&["key", "SRV"]);
    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("1 new"), "unexpected pull report: {pull_report}");
    let show = bob.run(&["show", &id]);
    assert!(show.contains("Shared task"));

    // fast-forward path
    alice.run(&["status", &id, "doing"]);
    alice.run(&["push"]);
    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("1 fast-forwarded"), "unexpected pull report: {pull_report}");
    let show = bob.run(&["show", &id]);
    assert!(show.contains("doing"));
}

#[test]
fn diverged_edits_merge_and_converge_to_identical_state() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());

    let alice = TestRepo::new();
    alice.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    std::fs::write(alice.path().join(".gitkeep"), "").unwrap();
    alice.git(&["add", ".gitkeep"]);
    alice.git(&["commit", "-q", "-m", "init"]);
    alice.git(&["branch", "-M", "main"]);
    alice.git(&["push", "-q", "-u", "origin", "main"]);

    let bob = TestRepo::clone_from(bare.path(), "bob");

    alice.run(&["key", "SRV"]);
    let out = alice.run(&["new", "Shared task", "--desc", "initial"]);
    let id = TestRepo::extract_id(&out);
    alice.run(&["push"]);

    bob.run(&["key", "SRV"]);
    bob.run(&["pull"]);

    // Diverge: alice and bob each edit different fields, neither syncing yet.
    alice.run(&["comment", &id, "alice's note"]);
    alice.run(&["edit", &id, "--priority", "high"]);

    bob.run(&["label", &id, "add", "urgent"]);
    bob.run(&["edit", &id, "--assignee", "bob"]);

    alice.run(&["push"]);

    // Bob's push should be rejected — he hasn't reconciled alice's changes yet.
    let err = bob.run_err(&["push"]);
    assert!(!err.is_empty(), "expected bob's push to be rejected");

    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("1 merged"), "expected a real merge: {pull_report}");

    let bob_json = bob.run(&["show", &id, "--format", "json"]);
    let bob_state: serde_json::Value = serde_json::from_str(&bob_json).unwrap();
    assert_eq!(bob_state["priority"], "high"); // alice's edit
    assert_eq!(bob_state["assignee"], "bob"); // bob's own edit
    assert_eq!(bob_state["labels"][0], "urgent"); // bob's own edit
    assert_eq!(bob_state["comments"][0]["text"], "alice's note"); // alice's edit

    // Push the merge back, pull it into alice — both sides must converge exactly.
    bob.run(&["push"]);
    let pull_report = alice.run(&["pull"]);
    assert!(pull_report.contains("1 fast-forwarded"), "alice should just fast-forward to bob's merge: {pull_report}");

    let alice_json = alice.run(&["show", &id, "--format", "json"]);
    let alice_state: serde_json::Value = serde_json::from_str(&alice_json).unwrap();
    assert_eq!(alice_state, bob_state, "both clones must converge to identical folded state");
}
