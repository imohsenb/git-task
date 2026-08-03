mod common;

use common::TestRepo;

#[test]
fn not_found_json_reports_kind_and_query() {
    let repo = TestRepo::new();
    let value = repo.run_err_json(&["show", "deadbeefdeadbeef", "--format", "json"]);
    assert_eq!(value["ok"], false);
    assert_eq!(value["command"], "show");
    assert_eq!(value["error"]["kind"], "not_found");
    assert_eq!(value["error"]["context"]["entity"], "task");
    assert_eq!(value["error"]["context"]["query"], "deadbeefdeadbeef");
}

#[test]
fn ambiguous_id_json_reports_kind_and_matches() {
    let repo = TestRepo::new();
    repo.run(&["new", "First", "--desc", "d"]);
    repo.run(&["new", "Second", "--desc", "d"]);

    // Every task id starts with the empty string, so this always matches both — a
    // deterministic way to force ambiguity without depending on real hash collisions.
    let value = repo.run_err_json(&["show", "", "--format", "json"]);
    assert_eq!(value["error"]["kind"], "ambiguous_id");
    let matches = value["error"]["context"]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    for m in matches {
        assert_eq!(m.as_str().unwrap().len(), 40, "matches should be full 40-hex ids");
    }
}

#[test]
fn identity_missing_json_reports_missing_fields_and_config_files() {
    let repo = TestRepo::new();
    repo.git(&["config", "--unset", "user.name"]);
    repo.git(&["config", "--unset", "user.email"]);

    let output = repo
        .cmd_no_global_identity()
        .args(["new", "T", "--desc", "d", "--format", "json"])
        .output()
        .expect("running git-task");
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be exactly one JSON document");

    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["kind"], "identity_missing");
    let missing: Vec<&str> =
        value["error"]["context"]["missing"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(missing.contains(&"user.name"), "missing should list user.name: {missing:?}");
    assert!(missing.contains(&"user.email"), "missing should list user.email: {missing:?}");
    assert!(value["error"]["context"]["config_files"].as_array().unwrap().len() >= 1);
}

#[test]
fn validation_error_json_for_missing_required_field() {
    let repo = TestRepo::new();
    repo.run(&["config", "field", "priority", "required"]);

    let value = repo.run_err_json(&["new", "T", "--desc", "d", "--format", "json"]);
    assert_eq!(value["error"]["kind"], "validation");
    let missing: Vec<&str> =
        value["error"]["context"]["missing"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(missing.contains(&"priority"));
}
