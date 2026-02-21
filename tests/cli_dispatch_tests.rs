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
fn optimize_command_dispatches_and_emits_deterministic_json() {
    let output_a = Command::new(bin())
        .args(["optimize", "enterprise", "swarm", "20"])
        .output()
        .expect("optimize should run");
    let output_b = Command::new(bin())
        .args(["optimize", "enterprise", "swarm", "20"])
        .output()
        .expect("optimize should run");

    assert_eq!(output_a.status.code(), Some(0));
    assert_eq!(output_b.status.code(), Some(0));

    let stdout_a = String::from_utf8_lossy(&output_a.stdout);
    let stdout_b = String::from_utf8_lossy(&output_b.stdout);
    assert_eq!(stdout_a, stdout_b);

    let payload: serde_json::Value =
        serde_json::from_str(&stdout_a).expect("optimize should emit json");
    let recommendations = payload
        .as_array()
        .expect("optimize payload should be an array");
    assert!(!recommendations.is_empty());

    let first = &recommendations[0];
    assert!(first["win_rate"].as_f64().unwrap_or(0.0) > 0.0);

    let first_hull = first["avg_hull_remaining"].as_f64().unwrap_or(0.0);
    let saw_hull_delta = recommendations.iter().any(|recommendation| {
        recommendation["avg_hull_remaining"]
            .as_f64()
            .map(|value| (value - first_hull).abs() > 1e-9)
            .unwrap_or(false)
    });
    assert!(
        saw_hull_delta,
        "recommendations should reflect combat metric differences"
    );
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
    fs::write(
        &path,
        "[{\"name\":\"SPOCK\",\"rank\":2},{\"name\":\"KIRK\",\"tier\":3}]",
    )
    .expect("fixture should be written");

    let output = Command::new(bin())
        .args(["import", path.to_string_lossy().as_ref()])
        .output()
        .expect("import should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("import summary:"));
    assert!(stdout.contains("matched=2"));
    assert!(stdout.contains("import complete: persisted 2 canonical roster entries"));

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
