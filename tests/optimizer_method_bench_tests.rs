use std::path::PathBuf;
use std::process::Command;

#[test]
fn optimizer_method_bench_random_smoke_emits_jsonl_with_diversity() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer_method_bench"))
        .current_dir(&crate_root)
        .args([
            "--case",
            "saladin_numeric",
            "--seed-panel",
            "7,8",
            "--methods",
            "random_stratified",
            "--random-candidates",
            "2",
            "--random-sims",
            "4",
        ])
        .output()
        .expect("optimizer_method_bench should run");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 2, "expected one JSONL row per seed: {stdout}");

    let mut seeds = Vec::new();
    for line in lines {
        let row: serde_json::Value =
            serde_json::from_str(line).expect("benchmark row should be valid JSON");
        assert_eq!(row["case"], "saladin_numeric");
        assert_eq!(row["method"], "random_stratified");
        assert_eq!(row["ranked_count"], 2);
        assert_eq!(row["trials_run_total"], 8);
        assert_eq!(row["diversity_top_k"], 2);
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
}
