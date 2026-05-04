//! Bundled `profiles/demo/roster.imported.json` must stay a representative sample (hundreds of
//! officers). Accidental overwrites—e.g. `kobayashi import` integration tests targeting the default
//! profile—have historically replaced it with a 2-row fixture; this test fails CI if the file is
//! truncated or wrongly trimmed.

use std::fs;
use std::path::PathBuf;

const DEMO_ROSTER_REL: &str = "profiles/demo/roster.imported.json";
/// Minimum officer count for the shipped demo roster; well below the maintained ~275 but enough to
/// catch accidental tiny fixtures (see module comment).
const DEMO_ROSTER_MIN_OFFICERS: usize = 200;

#[test]
fn bundled_demo_roster_is_not_truncated() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEMO_ROSTER_REL);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read bundled demo roster at {}: {e}; expected {} under the repo root",
            path.display(),
            DEMO_ROSTER_REL
        )
    });
    let value: serde_json::Value =
        serde_json::from_str(&raw).expect("bundled demo roster must be valid JSON");
    let officers = value
        .get("officers")
        .and_then(|o| o.as_array())
        .expect("bundled demo roster must have an 'officers' array");
    let n = officers.len();
    assert!(
        n >= DEMO_ROSTER_MIN_OFFICERS,
        "bundled demo roster at {} has only {n} officers; expected at least {}. \
         The file may have been overwritten by a local import/test or an accidental commit—restore \
         from git history or a full roster export (see profiles/README.md).",
        path.display(),
        DEMO_ROSTER_MIN_OFFICERS
    );
}
