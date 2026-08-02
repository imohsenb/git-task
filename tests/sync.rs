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
    assert!(show.contains("DOING")); // status badge uppercased
}

#[test]
fn drop_with_remote_deletes_the_ref_on_the_remote() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());

    let alice = TestRepo::new();
    alice.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    std::fs::write(alice.path().join(".gitkeep"), "").unwrap();
    alice.git(&["add", ".gitkeep"]);
    alice.git(&["commit", "-q", "-m", "init"]);
    alice.git(&["branch", "-M", "main"]);
    alice.git(&["push", "-q", "-u", "origin", "main"]);

    alice.run(&["key", "SRV"]);
    let out = alice.run(&["new", "Doomed shared task", "--desc", "initial"]);
    let id = TestRepo::extract_id(&out);
    alice.run(&["push"]);

    // `push` also carries along `refs/tasks/config` (the per-repo config chain), so count
    // only refs that aren't that reserved one.
    let count_bare_task_refs = || {
        let output = StdCommand::new("git")
            .arg("--git-dir")
            .arg(bare.path())
            .args(["for-each-ref", "refs/tasks"])
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().lines().filter(|l| !l.contains("refs/tasks/config")).count()
    };
    assert_eq!(count_bare_task_refs(), 1, "expected exactly one task ref on the bare remote");

    alice.run(&["drop", &id, "--force", "--remote"]);

    assert_eq!(count_bare_task_refs(), 0, "remote task ref should be gone after drop --remote");
}

#[test]
fn clone_fetches_tasks_only_into_fresh_directory() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());

    let alice = TestRepo::new();
    alice.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    std::fs::write(alice.path().join(".gitkeep"), "").unwrap();
    alice.git(&["add", ".gitkeep"]);
    alice.git(&["commit", "-q", "-m", "init"]);
    alice.git(&["branch", "-M", "main"]);
    alice.git(&["push", "-q", "-u", "origin", "main"]);

    alice.run(&["key", "SRV"]);
    let out = alice.run(&["new", "Shared task", "--desc", "initial"]);
    let id = TestRepo::extract_id(&out);
    alice.run(&["push"]);

    let workspace = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let bare_str = bare.path().to_str().unwrap();
    let clone_report = common::run_in(workspace.path(), config_dir.path(), &["clone", bare_str, "cloned-tasks"]);
    assert!(clone_report.contains("1 task"), "unexpected clone report: {clone_report}");

    let cloned_dir = workspace.path().join("cloned-tasks");
    assert!(cloned_dir.join(".git").exists(), "clone should init a git repo");
    assert!(!cloned_dir.join(".gitkeep").exists(), "clone must not check out source files");

    let show = common::run_in(&cloned_dir, config_dir.path(), &["show", &id]);
    assert!(show.contains("Shared task"));
}

#[test]
fn clone_derives_directory_name_from_url() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());
    let bare_str = bare.path().to_str().unwrap();

    let workspace = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let out = common::run_in(workspace.path(), config_dir.path(), &["clone", bare_str]);
    assert!(out.contains("0 task"), "unexpected clone report: {out}");

    let repo_name = std::path::Path::new(bare_str).file_name().unwrap().to_string_lossy();
    let expected_dir = workspace.path().join(format!("{repo_name}-tasks"));
    assert!(expected_dir.join(".git").exists(), "expected default dir '{repo_name}-tasks' to exist");
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
    bob.run(&["edit", &id, "--assignee", "bob@example.com"]);

    alice.run(&["push"]);

    // Bob's push should be rejected — he hasn't reconciled alice's changes yet.
    let err = bob.run_err(&["push"]);
    assert!(!err.is_empty(), "expected bob's push to be rejected");

    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("1 merged"), "expected a real merge: {pull_report}");

    let bob_json = bob.run(&["show", &id, "--format", "json"]);
    let bob_response: serde_json::Value = serde_json::from_str(&bob_json).unwrap();
    let bob_state = &bob_response["data"];
    assert_eq!(bob_state["priority"], "high"); // alice's edit
    assert_eq!(bob_state["assignee"], "bob@example.com"); // bob's own edit
    assert_eq!(bob_state["labels"][0], "urgent"); // bob's own edit
    assert_eq!(bob_state["comments"][0]["text"], "alice's note"); // alice's edit

    // Push the merge back, pull it into alice — both sides must converge exactly.
    bob.run(&["push"]);
    let pull_report = alice.run(&["pull"]);
    assert!(pull_report.contains("1 fast-forwarded"), "alice should just fast-forward to bob's merge: {pull_report}");

    let alice_json = alice.run(&["show", &id, "--format", "json"]);
    let alice_response: serde_json::Value = serde_json::from_str(&alice_json).unwrap();
    assert_eq!(&alice_response["data"], bob_state, "both clones must converge to identical folded state");
}

#[test]
fn diverged_config_edits_merge_and_converge() {
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

    // Shared base: alice sets the key, pushes; bob pulls it so both share one config root.
    alice.run(&["config", "key", "SRV"]);
    alice.run(&["push"]);
    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("config: initialized"), "bob should get config: {pull_report}");

    // Diverge: alice tightens a field, bob adds a rule — neither syncing yet.
    alice.run(&["config", "field", "priority", "required"]);
    bob.run(&[
        "config", "rule", "add", "--name", "triage", "--on", "task.created", "--do",
        "add_label urgent",
    ]);

    alice.run(&["push"]);
    // Bob's push is rejected until he reconciles alice's config change.
    let err = bob.run_err(&["push"]);
    assert!(!err.is_empty(), "expected bob's config push to be rejected");

    let pull_report = bob.run(&["pull"]);
    assert!(pull_report.contains("config: merged"), "expected a real config merge: {pull_report}");

    // Bob now has both edits.
    let bob_show = bob.run(&["config", "show"]);
    let priority_row = bob_show.lines().find(|l| l.contains("Priority")).unwrap_or("");
    assert!(priority_row.contains("required"), "alice's field edit missing: {bob_show}");
    assert!(bob_show.contains("triage"), "bob's own rule missing: {bob_show}");

    // Push the merge back; alice fast-forwards and both converge to identical config.
    bob.run(&["push"]);
    let pull_report = alice.run(&["pull"]);
    assert!(pull_report.contains("config: updated"), "alice should ff config: {pull_report}");

    let alice_show = alice.run(&["config", "show"]);
    assert_eq!(alice_show, bob_show, "both clones must converge to identical config");
}
