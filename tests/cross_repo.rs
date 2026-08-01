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
fn duplicate_register_is_rejected() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register"]);
    let err = repo.run_err(&["register"]);
    assert!(err.contains("already registered"), "unexpected error: {err}");
}
