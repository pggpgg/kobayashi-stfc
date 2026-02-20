use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kobayashi")
}

fn unique_temp_path(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("kobayashi-{name}-{stamp}.json"))
}

#[test]
fn simulate_command_dispatches_and_emits_json() {
    let output = Command::new(bin())
        .args(["simulate", "2", "11"])
        .output()
        .expect("simulate should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("simulate should emit json");
    assert_eq!(payload["events"].as_array().map(Vec::len), Some(8));
    assert!(payload["total_damage"].is_number());
}

#[test]
fn optimize_command_dispatches_and_emits_json() {
    let output = Command::new(bin())
        .args(["optimize", "enterprise", "swarm", "20"])
        .output()
        .expect("optimize should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("optimize should emit json");
    assert!(payload.is_array());
}

#[test]
fn import_command_returns_usage_without_path() {
    let output = Command::new(bin())
        .arg("import")
        .output()
        .expect("import should run");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: kobayashi import"));
}

#[test]
fn import_command_imports_json_file() {
    let path = unique_temp_path("import");
    fs::write(&path, "[{\"id\":\"one\"},{\"id\":\"two\"}]").expect("fixture should be written");

    let output = Command::new(bin())
        .args(["import", path.to_string_lossy().as_ref()])
        .output()
        .expect("import should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("import complete: records=2"));

    let _ = fs::remove_file(path);
}

#[test]
fn validate_command_returns_non_zero_on_invalid_data() {
    let path = unique_temp_path("invalid-officers");
    fs::write(&path, "[{\"id\":\"\",\"name\":\"\"}]").expect("fixture should be written");

    let output = Command::new(bin())
        .args(["validate", path.to_string_lossy().as_ref()])
        .output()
        .expect("validate should run");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("validation failed"));

    let _ = fs::remove_file(path);
}
