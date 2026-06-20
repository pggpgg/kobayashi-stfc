//! Regression gate: actionable opaque building `buff_*` stats must not exceed baseline.

use std::path::Path;

use kobayashi::data::mapping_gap_report::{
    load_building_mapping_gaps_baseline, load_opaque_buff_allowlist, scan_building_bonus_gaps,
    DEFAULT_BUILDING_MAPPING_GAPS_BASELINE_PATH, DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH,
};

#[test]
fn repo_building_opaque_buff_actionable_within_baseline() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let buildings_dir = manifest.join("data/buildings");
    let allowlist_path = manifest.join(DEFAULT_OPAQUE_BUFF_ALLOWLIST_PATH);
    let baseline_path = manifest.join(DEFAULT_BUILDING_MAPPING_GAPS_BASELINE_PATH);

    let report = scan_building_bonus_gaps(&buildings_dir).expect("scan building bonus gaps");
    let allowlist = load_opaque_buff_allowlist(&allowlist_path);
    let actionable = report.actionable_opaque_buff_stats(&allowlist);
    let allowlisted = report.allowlisted_opaque_buff_stats(&allowlist);

    assert!(
        allowlisted.len() >= 200,
        "expected economy/meta allowlist to be populated (got {} allowlisted of {} opaque)",
        allowlisted.len(),
        report.opaque_buff_stats.len()
    );

    let baseline = load_building_mapping_gaps_baseline(&baseline_path)
        .expect("data/buildings/mapping_gaps_baseline.json must exist");

    assert!(
        actionable.len() <= baseline.actionable_opaque_buff_stats,
        "actionable opaque buff count {} exceeds baseline {} — extend allowlist, map stat, or refresh baseline",
        actionable.len(),
        baseline.actionable_opaque_buff_stats
    );
}
