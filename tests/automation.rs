mod common;

use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

use common::TestRepo;

fn init_bare(bare_dir: &std::path::Path) {
    let status = StdCommand::new("git").args(["init", "-q", "--bare"]).arg(bare_dir).status().unwrap();
    assert!(status.success());
}

#[test]
fn auto_unassign_done_clears_assignee_on_status_done() {
    let repo = TestRepo::new();
    let out = repo.run(&[
        "new", "T", "--desc", "d", "--assignee", "a@b.com", "--status", "done", "--format", "json",
    ]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");

    assert!(value["data"]["task"]["assignee"].is_null(), "assignee should be cleared: {value}");
    assert_eq!(value["data"]["automation"][0]["rule"], "auto-unassign-done");
    assert_eq!(value["data"]["automation"][0]["ops"][0], "ClearAssignee");
}

#[test]
fn auto_unassign_done_disabled_per_repo_keeps_assignee() {
    let repo = TestRepo::new();
    repo.run(&["automation", "disable", "auto-unassign-done"]);

    let out = repo.run(&[
        "new", "T", "--desc", "d", "--assignee", "a@b.com", "--status", "done", "--format", "json",
    ]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");

    assert_eq!(value["data"]["task"]["assignee"], "a@b.com");
    assert!(value["data"]["automation"].as_array().unwrap().is_empty());
}

#[test]
fn auto_unassign_done_project_override_wins_over_global_disable() {
    let repo = TestRepo::new();
    repo.run(&["automation", "disable", "auto-unassign-done", "--global"]);
    repo.run(&["automation", "enable", "auto-unassign-done"]);

    let out = repo.run(&[
        "new", "T", "--desc", "d", "--assignee", "a@b.com", "--status", "done", "--format", "json",
    ]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");

    assert!(value["data"]["task"]["assignee"].is_null(), "project enable should win over global disable: {value}");
}

#[test]
fn automation_toggle_rejects_unknown_name() {
    let repo = TestRepo::new();
    let err = repo.run_err(&["automation", "disable", "bogus-name"]);
    assert!(err.contains("unknown built-in automation"), "unexpected error: {err}");
}

#[test]
fn config_show_json_lists_builtins_with_resolved_state_and_source() {
    let repo = TestRepo::new();

    let out = repo.run(&["config", "show", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    let builtins = value["data"]["builtins"].as_array().expect("builtins array");
    assert_eq!(builtins.len(), 2);
    let names: Vec<&str> = builtins.iter().map(|b| b["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"auto-unassign-done"));
    assert!(names.contains(&"auto-sync"));
    for b in builtins {
        assert_eq!(b["enabled"], true, "should be enabled by default: {b}");
        assert_eq!(b["source"], "default");
    }

    repo.run(&["automation", "disable", "auto-sync"]);
    let out = repo.run(&["config", "show", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    let builtins = value["data"]["builtins"].as_array().expect("builtins array");
    let auto_sync = builtins.iter().find(|b| b["name"] == "auto-sync").expect("auto-sync entry");
    assert_eq!(auto_sync["enabled"], false);
    assert_eq!(auto_sync["source"], "project");
}

/// The requirement this exists to check: `auto-sync` must never print anything, in either
/// output mode, on any outcome — including its guaranteed-failure path here (no remote
/// configured). Deterministic (no timing dependency), unlike the background-push test below.
#[test]
fn auto_sync_produces_no_extra_output_with_no_remote_configured() {
    let repo = TestRepo::new();

    let json_out = repo
        .cmd_with_auto_sync()
        .args(["new", "Silent A", "--desc", "d", "--format", "json"])
        .output()
        .expect("running git-task");
    assert!(json_out.status.success());
    assert!(
        json_out.stderr.is_empty(),
        "auto-sync must never write to stderr: {}",
        String::from_utf8_lossy(&json_out.stderr)
    );
    let stdout = String::from_utf8(json_out.stdout).expect("utf8 stdout");
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be exactly one JSON document");

    let text_out =
        repo.cmd_with_auto_sync().args(["new", "Silent B", "--desc", "d"]).output().expect("running git-task");
    assert!(text_out.status.success());
    assert!(
        text_out.stderr.is_empty(),
        "auto-sync must never write to stderr: {}",
        String::from_utf8_lossy(&text_out.stderr)
    );
}

/// The one timing-sensitive test in this suite (bounded-timeout polling), isolated here so its
/// flakiness risk doesn't spread: confirms the detached background worker (`sync::worker`)
/// actually reaches the remote, not just that it was spawned.
#[test]
fn auto_sync_pushes_in_background_eventually() {
    let bare = tempfile::tempdir().unwrap();
    init_bare(bare.path());

    let alice = TestRepo::new();
    alice.git(&["remote", "add", "origin", bare.path().to_str().unwrap()]);
    std::fs::write(alice.path().join(".gitkeep"), "").unwrap();
    alice.git(&["add", ".gitkeep"]);
    alice.git(&["commit", "-q", "-m", "init"]);
    alice.git(&["branch", "-M", "main"]);
    alice.git(&["push", "-q", "-u", "origin", "main"]);

    alice.run(&["config", "key", "SRV"]);

    let out = alice
        .cmd_with_auto_sync()
        .args(["new", "Auto synced", "--desc", "d"])
        .output()
        .expect("running git-task");
    assert!(out.status.success());
    // `ls-remote` lists refs by the task's full oid, not the `KEY-<short hash>` display form
    // `extract_id` returns — the short hash is a prefix of that oid, so search for just that.
    let display_id = TestRepo::extract_id(&String::from_utf8_lossy(&out.stdout));
    let hash_prefix = display_id.split_once('-').map(|(_, h)| h.to_string()).unwrap_or(display_id);

    // The triggering command already returned — the actual push happens in a detached
    // background worker, so give it a bounded window to land.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = false;
    while Instant::now() < deadline {
        let refs =
            StdCommand::new("git").args(["ls-remote", bare.path().to_str().unwrap()]).output().expect("git ls-remote");
        if String::from_utf8_lossy(&refs.stdout).contains(&hash_prefix) {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(seen, "auto-sync should have pushed the new task to the remote within the timeout");
}
