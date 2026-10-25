//! Provenance for existing normalized assets that predate the explicit units flag.
//! Future normalizers write the flag on each record; an explicit false always wins.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct NativeCriticalDamageManifest {
    schema_version: u32,
    ship_ids: HashSet<String>,
    hostile_ids: HashSet<String>,
}

fn manifest() -> &'static NativeCriticalDamageManifest {
    static MANIFEST: OnceLock<NativeCriticalDamageManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        let manifest: NativeCriticalDamageManifest = serde_json::from_str(include_str!(
            "../../data/native_critical_damage_sources.json"
        ))
        .expect("validated native critical-damage source manifest");
        assert_eq!(manifest.schema_version, 1);
        manifest
    })
}

pub(crate) fn ship_has_native_critical_bonus(id: &str) -> bool {
    manifest().ship_ids.contains(id)
}

pub(crate) fn hostile_has_native_critical_bonus(id: &str) -> bool {
    manifest().hostile_ids.contains(id)
}
