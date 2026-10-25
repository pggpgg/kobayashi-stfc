//! A malformed optional upstream stat row must not remove LCARS abilities for every officer.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::lcars::{
    build_officer_model, index_lcars_officers_by_id, DEFAULT_INPUT, DEFAULT_SUMMARY,
    DEFAULT_TRANSLATIONS,
};

const TOM_ID: &str = "tom-paris-3640cc";
const JADZIA_ID: &str = "jadzia-dax-736698";

#[test]
fn one_malformed_upstream_officer_file_does_not_drop_all_officer_abilities() {
    let temp = std::env::temp_dir().join(format!(
        "kobayashi-officer-resilience-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&temp).unwrap();
    struct RemoveDir(PathBuf);
    impl Drop for RemoveDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _remove_dir = RemoveDir(temp.clone());

    fs::write(temp.join("3849424588.json"), b"{ malformed json").unwrap();
    fs::copy(
        "data/upstream/data-stfc-space/officers/1916661181.json",
        temp.join("1916661181.json"),
    )
    .unwrap();

    let built = build_officer_model(
        PathBuf::from(DEFAULT_INPUT).as_path(),
        PathBuf::from(DEFAULT_SUMMARY).as_path(),
        PathBuf::from(DEFAULT_TRANSLATIONS).as_path(),
        &temp,
        true,
    )
    .expect("a malformed optional stat file should not abort the entire officer model");
    let officers = index_lcars_officers_by_id(built);
    let tom = &officers[TOM_ID];
    let jadzia = &officers[JADZIA_ID];

    assert!(
        tom.captain_ability.is_some()
            || tom.bridge_ability.is_some()
            || tom.below_decks_ability.is_some()
    );
    assert!(
        tom.stats.is_empty(),
        "do not invent fallback stats for the malformed file"
    );
    assert!(
        !jadzia.stats.is_empty(),
        "valid officer stat files should still load"
    );
    assert!(
        jadzia.captain_ability.is_some()
            || jadzia.bridge_ability.is_some()
            || jadzia.below_decks_ability.is_some()
    );
}
