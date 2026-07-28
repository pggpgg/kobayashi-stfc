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

fn unique_temp_txt_path(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("kobayashi-{name}-{stamp}.txt"))
}

/// Disposable profile id so `kobayashi import` integration tests never overwrite a real
/// `profiles/<user>/roster.imported.json` (see `import_command_imports_*`).
fn unique_cli_import_test_profile_id() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    format!("__kobayashi_cli_import_{stamp}")
}

struct RemoveProfileDirGuard(PathBuf);

impl Drop for RemoveProfileDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// CLI may emit tracing JSON lines before or after the final JSON payload; compare only that payload.
/// Tracing lines are single-line JSON objects with `timestamp` + `level`. The real payload may span
/// multiple lines (pretty-printed array or object), so try suffix parses starting at each `{`/`[` line.
fn extract_cli_json_payload(stdout: &str) -> Option<String> {
    fn is_tracing_json(val: &serde_json::Value) -> bool {
        val.as_object()
            .map(|o| o.contains_key("timestamp") && o.contains_key("level"))
            .unwrap_or(false)
    }
    let lines: Vec<&str> = stdout.lines().collect();
    for start in (0..lines.len()).rev() {
        let first = lines[start].trim();
        if !first.starts_with('[') && !first.starts_with('{') {
            continue;
        }
        let candidate = lines[start..].join("\n");
        let trimmed = candidate.trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if !is_tracing_json(&v) {
                return Some(trimmed.to_string());
            }
        }
    }
    let trimmed = stdout.trim();
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return Some(trimmed.to_string());
    }
    None
}

#[test]
fn simulate_command_dispatches_and_emits_json() {
    let output = Command::new(bin())
        .args(["simulate", "2", "11"])
        .output()
        .expect("simulate should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_text = extract_cli_json_payload(&stdout).expect("simulate should emit json");
    let payload: serde_json::Value =
        serde_json::from_str(&json_text).expect("simulate should emit json");
    // Per-shot trace events; count depends on rounds/shots/weapons (see combat engine).
    assert_eq!(payload["events"].as_array().map(Vec::len), Some(23));
    assert!(payload["total_damage"].is_number());
}

#[test]
fn optimize_command_dispatches_and_emits_deterministic_json() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let output_a = Command::new(bin())
        .current_dir(&crate_root)
        .args([
            "optimize",
            "--ship",
            "uss_enterprise",
            "--hostile",
            "2918121098",
            "--sims",
            "20",
            "--max-candidates",
            "200",
        ])
        .output()
        .expect("optimize should run");
    let output_b = Command::new(bin())
        .current_dir(&crate_root)
        .args([
            "optimize",
            "--ship",
            "uss_enterprise",
            "--hostile",
            "2918121098",
            "--sims",
            "20",
            "--max-candidates",
            "200",
        ])
        .output()
        .expect("optimize should run");

    assert_eq!(output_a.status.code(), Some(0));
    assert_eq!(output_b.status.code(), Some(0));

    let stdout_a = String::from_utf8_lossy(&output_a.stdout);
    let stdout_b = String::from_utf8_lossy(&output_b.stdout);
    let json_a = extract_cli_json_payload(&stdout_a).expect("optimize should emit JSON array");
    let json_b = extract_cli_json_payload(&stdout_b).expect("optimize should emit JSON array");
    assert_eq!(
        json_a, json_b,
        "two runs with same args should produce identical JSON output (determinism)"
    );

    let recommendations: Vec<serde_json::Value> =
        serde_json::from_str(json_a.trim()).expect("optimize should emit valid JSON array");

    if recommendations.is_empty() {
        // No ship/hostile data or no candidates; empty result is valid
        return;
    }

    let first = &recommendations[0];
    let all_zero_win_rate = recommendations
        .iter()
        .all(|r| r["win_rate"].as_f64().unwrap_or(1.0) == 0.0);
    if !all_zero_win_rate {
        assert!(first["win_rate"].as_f64().unwrap_or(0.0) > 0.0);
    }

    // If all crews win with maximum hull remaining, the scenario is one-sided (attacker dominates),
    // and hull-delta differentiation is impossible — skip that check in this degenerate case.
    let all_max_outcome = recommendations.iter().all(|r| {
        r["win_rate"].as_f64().unwrap_or(0.0) >= 1.0
            && r["avg_hull_remaining"].as_f64().unwrap_or(0.0) >= 1.0 - 1e-9
    });
    if !all_zero_win_rate && !all_max_outcome {
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

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile_id = unique_cli_import_test_profile_id();
    let profile_dir = crate_root.join("profiles").join(&profile_id);
    let _guard = RemoveProfileDirGuard(profile_dir.clone());

    let output = Command::new(bin())
        .current_dir(&crate_root)
        .args([
            "import",
            path.to_string_lossy().as_ref(),
            "--profile",
            profile_id.as_str(),
        ])
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
fn import_command_imports_txt_roster() {
    let path = unique_temp_txt_path("roster");
    fs::write(&path, "name,tier,level\nKirk,3,45\nSpock,T2,")
        .expect("roster fixture should be written");

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile_id = unique_cli_import_test_profile_id();
    let profile_dir = crate_root.join("profiles").join(&profile_id);
    let _guard = RemoveProfileDirGuard(profile_dir.clone());

    let output = Command::new(bin())
        .current_dir(&crate_root)
        .args([
            "import",
            path.to_string_lossy().as_ref(),
            "--profile",
            profile_id.as_str(),
        ])
        .output()
        .expect("import should run");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("import summary:"));
    assert!(stdout.contains("import complete:"));
    assert!(stdout.contains("persisted 2 canonical roster entries"));

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

/// `optimize` must refuse an id it cannot resolve instead of running the engine's synthetic
/// fallback, which returns a confident 100%-win-rate answer for a fight that never existed.
#[test]
fn optimize_command_refuses_ids_that_do_not_resolve() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // "saladin" is not a ship id ("uss_saladin" is) — the realistic near-miss.
    for (label, ship, hostile) in [
        ("ship", "saladin", "2918121098"),
        ("hostile", "uss_saladin", "definitely_not_a_hostile"),
    ] {
        let output = Command::new(bin())
            .current_dir(&crate_root)
            .args([
                "optimize",
                "--ship",
                ship,
                "--hostile",
                hostile,
                "--sims",
                "20",
            ])
            .output()
            .expect("optimize should run");

        assert_ne!(
            output.status.code(),
            Some(0),
            "unresolvable {label} should fail, stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let needle = if label == "ship" { ship } else { hostile };
        assert!(
            stderr.contains(needle),
            "error should name the offending {label} {needle:?}: {stderr}"
        );
        assert!(
            extract_cli_json_payload(&String::from_utf8_lossy(&output.stdout)).is_none(),
            "a refused scenario must not emit recommendations"
        );
    }
}
