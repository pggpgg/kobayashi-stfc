//! Architectural invariant: the combat hot loop (`src/combat/`) must not touch the LCARS drop
//! report or emit drop-equivalent warnings. All drop recording happens upstream, at YAML→IR
//! resolve time. This test greps the combat module for forbidden patterns and fails if any
//! show up — guarding the zero-allocation hot-loop guarantee documented in CLAUDE.md.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn combat_module_has_no_drop_logging() {
    let combat_dir = PathBuf::from("src/combat");
    if !combat_dir.is_dir() {
        return; // minimal checkouts skip
    }

    // Forbidden patterns. Update with care: each one represents a way to record drop / warn
    // signals from inside the hot loop, which would re-introduce per-round allocations.
    let forbidden = [
        "LcarsDropReport",
        "DroppedLcarsEffect",
        "maybe_record_drop",
        "lcars_effect_to_combat_effect_spec_with_report",
        "warn!",
        "warn_span!",
    ];

    let mut violations: Vec<String> = Vec::new();
    for pattern in forbidden {
        let output = Command::new("grep")
            .args(["-rn", "--include=*.rs", pattern, "src/combat"])
            .output()
            .expect("grep should be available");
        let hits = String::from_utf8_lossy(&output.stdout);
        for line in hits.lines() {
            // grep returns "path:lineno:content" — filter doc-comment mentions if any sneak in.
            if line.contains("//") || line.contains("///") {
                continue;
            }
            violations.push(format!("{pattern} → {line}"));
        }
    }

    assert!(
        violations.is_empty(),
        "src/combat/ must not contain drop-logging or warn macros (hot loop must stay allocation-free); \
         violations: {violations:#?}"
    );
}
