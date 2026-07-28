use std::path::PathBuf;
use std::process::{Command, Output};

fn run_bench(args: &[&str]) -> Output {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Command::new(env!("CARGO_BIN_EXE_optimizer_method_bench"))
        .current_dir(&crate_root)
        .args(args)
        .output()
        .expect("optimizer_method_bench should run")
}

fn records(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("benchmark row should be valid JSON"))
        .collect()
}

#[test]
fn optimizer_method_bench_random_smoke_emits_jsonl_with_diversity() {
    let output = run_bench(&[
        "--case",
        "saladin_corvus",
        "--seed-panel",
        "7,8",
        "--methods",
        "random_stratified",
        "--random-candidates",
        "2",
        "--random-sims",
        "4",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rows = records(&stdout);
    let lanes: Vec<&serde_json::Value> =
        rows.iter().filter(|r| r["record_kind"] == "lane").collect();
    assert_eq!(lanes.len(), 2, "expected one lane row per seed: {stdout}");

    let mut seeds = Vec::new();
    for row in lanes {
        assert_eq!(row["case"], "saladin_corvus");
        assert_eq!(row["method"], "random_stratified");
        assert_eq!(row["ranked_count"], 2);
        assert_eq!(row["trials_run_total"], 8);
        assert_eq!(row["diversity_top_k"], 2);
        // Without --budget-mode the lane keeps its own knobs, and says so.
        assert_eq!(row["budget"]["mode"], "native");
        assert!(
            row["unique_captains_top_k"].as_u64().unwrap_or(0) >= 1,
            "captain diversity should be present"
        );
        assert!(
            row["avg_pairwise_material_jaccard_distance_top_k"]
                .as_f64()
                .is_some(),
            "pairwise material distance should be present"
        );
        seeds.push(row["seed"].as_u64().expect("seed"));
    }
    assert_eq!(seeds, vec![7, 8]);

    let stability: Vec<&serde_json::Value> = rows
        .iter()
        .filter(|r| r["record_kind"] == "stability")
        .collect();
    assert_eq!(stability.len(), 1, "one aggregate per (case, method)");
    assert_eq!(stability[0]["method"], "random_stratified");
    assert_eq!(stability[0]["seeds"], 2);
}

#[test]
fn equal_trial_budget_sizes_every_lane_to_the_same_trial_count() {
    let output = run_bench(&[
        "--case",
        "saladin_corvus",
        "--methods",
        "tiered,random_stratified",
        "--budget-mode",
        "equal-trials",
        "--trial-budget",
        "8000",
        "--scout-sims",
        "20",
        "--sims",
        "100",
        "--top-k",
        "4",
        "--random-sims",
        "100",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for row in records(&stdout) {
        if row["record_kind"] != "lane" {
            continue;
        }
        assert_eq!(row["budget"]["mode"], "equal_trials");
        assert_eq!(row["budget"]["trial_budget"], 8000);
        let projected = row["budget"]["projected_trials"]
            .as_u64()
            .expect("an MC lane projects a trial count");
        assert!(
            projected <= 8000,
            "{} overspent its budget: {projected}",
            row["method"]
        );
    }
}

#[test]
fn a_case_whose_ship_does_not_resolve_is_refused_rather_than_benchmarked() {
    // An unresolved id does not fail downstream — it produces a fight the crew wins in round 1,
    // which looks like a benchmark result. The harness must refuse instead.
    let output = run_bench(&["--ship", "saladin", "--hostile", "1140710508"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not resolve"),
        "should say why it refused: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "a refused case must not emit records"
    );
}
