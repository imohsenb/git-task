mod common;

use common::TestRepo;

#[test]
fn repos_json_shallow_and_deep() {
    let config_dir = tempfile::tempdir().expect("tempdir");
    let repo = TestRepo::new_with_shared_config(config_dir.path());
    repo.run(&["register", "--project", "backend"]);

    let out = repo.run(&["repos", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["command"], "repos");
    let entry = &value["data"]["repos"][0];
    assert!(entry["exists"].is_null(), "shallow repos entries leave probed fields null");
    assert!(entry["key"].is_null());

    let out = repo.run(&["repos", "--format", "json", "--deep"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    let entry = &value["data"]["repos"][0];
    assert_eq!(entry["exists"], true);
    assert_eq!(entry["openable"], true);
    assert!(entry["identity"]["ok"].as_bool().unwrap());
}

#[test]
fn whoami_json_reports_repo_global_and_effective() {
    let repo = TestRepo::new();
    let out = repo.run(&["whoami", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["command"], "whoami");
    assert_eq!(value["data"]["repo"]["email"], "test@example.com");
    assert_eq!(value["data"]["effective"]["email"], "test@example.com");
    assert_eq!(value["data"]["effective"]["ok"], true);
}

#[test]
fn status_comment_label_delete_json_shapes() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "T", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let out = repo.run(&["status", &id, "doing", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["task"]["status"], "doing");
    assert_eq!(value["data"]["ops"][0], "SetStatus");

    let out = repo.run(&["comment", &id, "a note", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["task"]["comments"][0]["text"], "a note");
    assert_eq!(value["data"]["ops"][0], "AddComment");

    let out = repo.run(&["label", &id, "add", "urgent", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["task"]["labels"][0], "urgent");
    assert_eq!(value["data"]["ops"][0], "AddLabel");

    let out = repo.run(&["delete", &id, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["data"]["task"]["deleted"], true);
    assert_eq!(value["data"]["ops"][0], "DeleteTask");
}

#[test]
fn drop_json_shape() {
    let repo = TestRepo::new();
    let out = repo.run(&["new", "T", "--desc", "d"]);
    let id = TestRepo::extract_id(&out);

    let out = repo.run(&["drop", &id, "--force", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(value["command"], "drop");
    assert_eq!(value["data"]["title"], "T");
    assert!(value["data"]["remote_deleted"].is_null());
    assert_eq!(value["data"]["id"].as_str().unwrap().len(), 40);
}
