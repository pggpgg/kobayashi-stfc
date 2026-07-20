//! Architectural invariant: the combat hot loop (`src/combat/`) must not touch the LCARS drop
//! report or emit drop-equivalent warnings. All drop recording happens upstream, at YAML→IR
//! resolve time. This test greps the combat module for forbidden patterns and fails if any
//! show up — guarding the zero-allocation hot-loop guarantee documented in CLAUDE.md.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files_below(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()));
    for entry in entries {
        let path = entry.expect("combat directory entry").path();
        if path.is_dir() {
            rust_files_below(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

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

    let mut files = Vec::new();
    rust_files_below(&combat_dir, &mut files);
    files.sort();

    let mut violations: Vec<String> = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        for (line_index, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for pattern in forbidden {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{pattern} → {}:{}:{line}",
                        path.display(),
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "src/combat/ must not contain drop-logging or warn macros (hot loop must stay allocation-free); \
         violations: {violations:#?}"
    );
}
