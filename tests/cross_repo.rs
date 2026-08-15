mod common;

use common::TestRepo;

#[test]
fn ls_aggregates_across_registered_repos_from_anywhere() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let elsewhere = tempfile::tempdir().expect("tempdir");

    let server = TestRepo::new_with_shared_config(config_dir.path());
    server.run(&["key", "SRV"]);
    server.run(&["new", "Server task", "--desc", "d"]);
    server.run(&["register", "--project", "backend"]);

    let web = TestRepo::new_with_shared_config(config_dir.path());
    web.run(&["key", "WEB"]);
    web.run(&["new", "Web task", "--desc", "d"]);
    web.run(&["register", "--project", "frontend"]);

    // Run `ls` from a directory that isn't either repo — aggregation must not depend on cwd.
    let output = server.cmd_from(elsewhere.path()).arg("ls").output().expect("running git-task");
    assert!(output.status.success(), "ls failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("Server task"));
    assert!(stdout.contains("Web task"));
    assert!(stdout.contains("backend"));
    assert!(stdout.contains("frontend"));
}

#[test]
fn ls_project_filter_narrows_to_one_repo() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let elsewhere = tempfile::tempdir().expect("tempdir");

    let server = TestRepo::new_with_shared_config(config_dir.path());
    server.run(&["new", "Server task", "--desc", "d"]);
    server.run(&["register", "--project", "backend"]);

    let web = TestRepo::new_with_shared_config(config_dir.path());
    web.run(&["new", "Web task", "--desc", "d"]);
    web.run(&["register", "--project", "frontend"]);

    let output = server
        .cmd_from(elsewhere.path())
        .args(["ls", "--project", "backend"])
        .output()
        .expect("running git-task");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Server task"));
    assert!(!stdout.contains("Web task"));
}

#[test]
fn ls_format_json_groups_by_repo() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let elsewhere = tempfile::tempdir().expect("tempdir");

    let server = TestRepo::new_with_shared_config(config_dir.path());
    server.run(&["key", "SRV"]);
    server.run(&["new", "Server task", "--desc", "d"]);
    server.run(&["register", "--project", "backend"]);

    let web = TestRepo::new_with_shared_config(config_dir.path());
    web.run(&["key", "WEB"]);
    web.run(&["new", "Web task", "--desc", "d"]);
    web.run(&["register", "--project", "frontend"]);

    let output = server
        .cmd_from(elsewhere.path())
        .args(["ls", "--format", "json"])
        .output()
        .expect("running git-task");
    assert!(output.status.success(), "ls --format json failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "ls");
    let data = &value["data"];
    assert_eq!(data["scope"]["mode"], "registry");
    assert_eq!(data["scope"]["repo_count"], 2);
    assert_eq!(data["total"], 2);
    assert_eq!(data["statuses"][0], "todo");

    let repos = data["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    let titles: Vec<&str> =
        repos.iter().flat_map(|r| r["tasks"].as_array().unwrap()).map(|t| t["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Server task"));
    assert!(titles.contains(&"Web task"));
    for repo in repos {
        assert!(repo["tasks"].as_array().unwrap()[0]["history"].is_null(), "ls omits history by default");
    }
}

#[test]
fn register_rerun_without_project_is_a_no_op() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register"]);
    // Not running interactively (no tty in tests), so this can't prompt — it should just
    // report the current project and leave everything alone, not error.
    let out = repo.run(&["register"]);
    assert!(out.contains("Already registered"), "unexpected output: {out}");
}

#[test]
fn register_rerun_with_same_project_is_a_no_op() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);
    let out = repo.run(&["register", "--project", "backend"]);
    assert!(out.contains("Nothing to do"), "unexpected output: {out}");
}

#[test]
fn register_rerun_with_new_project_moves_the_repo() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);
    let out = repo.run(&["register", "--project", "frontend"]);
    assert!(out.contains("Moved") && out.contains("frontend"), "unexpected output: {out}");

    let projects = repo.run(&["projects"]);
    assert!(projects.contains("frontend"));
    assert!(!projects.contains("backend"), "old project should be gone: {projects}");
}

#[test]
fn register_json_reports_action_and_full_registry() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());

    let out = repo.run(&["register", "--project", "backend", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["action"], "registered");
    assert_eq!(value["data"]["project"], "backend");
    assert_eq!(value["data"]["registry"]["repos"][0]["project"], "backend");

    // Not running interactively (no tty in tests) and no --project passed: noop, not a hang.
    let out = repo.run(&["register", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["action"], "noop");

    let out = repo.run(&["register", "--project", "frontend", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["action"], "moved");
    assert_eq!(value["data"]["project"], "frontend");
    assert_eq!(value["data"]["previous_project"], "backend");
}

#[test]
fn project_json_mutations_report_action_and_registry() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());

    let out = repo.run(&["project", "create", "infra", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["action"], "project_created");
    assert!(value["data"]["registry"]["projects"].as_array().unwrap().iter().any(|p| p == "infra"));

    let out = repo.run(&["project", "set-default", "infra", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["action"], "default_set");
    assert_eq!(value["data"]["registry"]["default_project"], "infra");
}

#[test]
fn project_delete_refuses_non_empty_project_without_force() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);

    let err = repo.run_err(&["project", "delete", "backend"]);
    assert!(err.contains("still has 1 repo"), "unexpected error: {err}");
    assert!(err.contains("--force"), "expected a --force hint, got: {err}");

    let projects = repo.run(&["projects"]);
    assert!(projects.contains("backend"), "project should still exist: {projects}");
}

#[test]
fn project_delete_with_force_unregisters_its_repos() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);

    let out = repo.run(&["project", "delete", "backend", "--force"]);
    assert!(out.contains("Unregistered 1 repo"), "unexpected output: {out}");
    assert!(out.contains("Deleted project 'backend'"), "unexpected output: {out}");

    let projects = repo.run(&["projects"]);
    assert!(!projects.contains("backend"), "project should be gone: {projects}");
    let repos = repo.run(&["repos"]);
    assert!(!repos.contains("backend"), "repo should have been unregistered: {repos}");
}

/// `epic add --repo` records a fully-resolved cross-repo parent on the child's own side, and
/// `show` on the epic finds that child by scanning every other repo registered under the same
/// project — the "see all the linked tickets" end-to-end path: add from the child's repo, list
/// from the epic's.
#[test]
fn epic_cross_repo_add_lists_on_epic_and_removes_cleanly() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let epics_repo = TestRepo::new_with_shared_config(config_dir.path());
    let backend_repo = TestRepo::new_with_shared_config(config_dir.path());

    let register_out = epics_repo.run(&["register", "--project", "platform", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&register_out).expect("valid json");
    let epics_name = value["data"]["name"].as_str().expect("name").to_string();

    let register_out = backend_repo.run(&["register", "--project", "platform", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&register_out).expect("valid json");
    let backend_name = value["data"]["name"].as_str().expect("name").to_string();

    let epic_out = epics_repo.run(&["new", "Big Epic", "--kind", "epic", "--desc", "d"]);
    let epic_id = TestRepo::extract_id(&epic_out);

    let child_out = backend_repo.run(&["new", "Backend task", "--desc", "d"]);
    let child_id = TestRepo::extract_id(&child_out);

    backend_repo.run(&["epic", &epic_id, "add", &child_id, "--repo", &epics_name]);

    // The child sees its cross-repo parent.
    let show = backend_repo.run(&["show", &child_id]);
    assert!(show.contains("Parent"), "text output: {show}");
    let show_json = backend_repo.run(&["show", &child_id, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&show_json).expect("valid json");
    assert_eq!(value["data"]["parent_display_id"], epic_id);
    assert!(!value["data"]["parent_repo"].is_null());

    // The epic, from its own repo, lists the cross-repo child.
    let epic_show = epics_repo.run(&["show", &epic_id]);
    assert!(epic_show.contains("Backend task"), "text output: {epic_show}");

    let epic_show_json = epics_repo.run(&["show", &epic_id, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&epic_show_json).expect("valid json");
    let children = value["data"]["children"].as_array().expect("children array");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["title"], "Backend task");
    assert_eq!(children[0]["repo"], backend_name);
    assert!(children[0]["id"].is_null());

    // Removing it (from the child's repo, same --repo) drops it from the epic's listing.
    backend_repo.run(&["epic", &epic_id, "rm", &child_id, "--repo", &epics_name]);
    let epic_show_json = epics_repo.run(&["show", &epic_id, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&epic_show_json).expect("valid json");
    assert_eq!(value["data"]["children"].as_array().map(|a| a.len()).unwrap_or(0), 0);
}

/// Cross-repo epics require both repos registered under the *same* project — a different
/// project fails loudly rather than silently linking across project boundaries.
#[test]
fn epic_cross_repo_add_rejects_a_different_project() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let epics_repo = TestRepo::new_with_shared_config(config_dir.path());
    let backend_repo = TestRepo::new_with_shared_config(config_dir.path());

    let register_out = epics_repo.run(&["register", "--project", "platform", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&register_out).expect("valid json");
    let epics_name = value["data"]["name"].as_str().expect("name").to_string();

    backend_repo.run(&["register", "--project", "other"]);

    let epic_out = epics_repo.run(&["new", "Big Epic", "--kind", "epic", "--desc", "d"]);
    let epic_id = TestRepo::extract_id(&epic_out);
    let child_out = backend_repo.run(&["new", "Backend task", "--desc", "d"]);
    let child_id = TestRepo::extract_id(&child_out);

    let err = backend_repo.run_err(&["epic", &epic_id, "add", &child_id, "--repo", &epics_name]);
    assert!(err.contains("project"), "expected a same-project error, got: {err}");
}

/// Cross-repo epics require the current repo itself to be registered too — there's no project
/// to compare against otherwise.
#[test]
fn epic_cross_repo_add_rejects_when_current_repo_unregistered() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let epics_repo = TestRepo::new_with_shared_config(config_dir.path());
    let backend_repo = TestRepo::new_with_shared_config(config_dir.path());

    let register_out = epics_repo.run(&["register", "--project", "platform", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&register_out).expect("valid json");
    let epics_name = value["data"]["name"].as_str().expect("name").to_string();

    let epic_out = epics_repo.run(&["new", "Big Epic", "--kind", "epic", "--desc", "d"]);
    let epic_id = TestRepo::extract_id(&epic_out);
    let child_out = backend_repo.run(&["new", "Backend task", "--desc", "d"]);
    let child_id = TestRepo::extract_id(&child_out);

    let err = backend_repo.run_err(&["epic", &epic_id, "add", &child_id, "--repo", &epics_name]);
    assert!(err.contains("registered"), "expected a not-registered error, got: {err}");
}

/// The shared resolver `epic`/`link` both go through now hard-fails a `--repo` that can't be
/// resolved at all, instead of the old behavior of silently storing whatever string it was
/// given — a real gap, since a typo'd path used to record a dead reference with no error.
#[test]
fn repo_arg_bare_nonexistent_path_fails_for_both_link_and_epic() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "T", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);
    let epic_out = repo.run(&["new", "Epic", "--kind", "epic", "--desc", "d"]);
    let epic_id = TestRepo::extract_id(&epic_out);

    let nonexistent = repo.path().join("does-not-exist-anywhere");
    let nonexistent = nonexistent.to_string_lossy().to_string();

    let err = repo.run_err(&["link", &id, "add", "blocks", "abc123", "--repo", &nonexistent]);
    assert!(err.contains("cannot resolve"), "expected an unresolved-repo error, got: {err}");

    let err = repo.run_err(&["epic", &epic_id, "add", &id, "--repo", &nonexistent]);
    assert!(err.contains("cannot resolve"), "expected an unresolved-repo error, got: {err}");
}
