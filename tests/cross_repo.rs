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
