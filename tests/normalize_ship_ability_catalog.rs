use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn one_malformed_catalog_entry_does_not_drop_other_ship_abilities() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "kobayashi-normalize-ship-ability-{}-{nonce}",
        std::process::id()
    ));
    let ship_dir = root.join("data/upstream/data-stfc-space/ships");
    fs::create_dir_all(&ship_dir).expect("create isolated source tree");
    fs::write(
        root.join("data/upstream/data-stfc-space/ship_id_registry.json"),
        r#"{"ships":[{"numeric_id":1,"id":"test_ship","ship_name":"Test Ship","ship_class":"explorer"}]}"#,
    )
    .expect("write isolated id registry");
    fs::write(
        root.join("data/upstream/data-stfc-space/ship_ability_catalog.json"),
        r#"{"entries":{
            "1":{"timing":"combat_begin","effect_type":"accuracy","value_is_percentage":true},
            "2":{"timing":"round_start"}
        }}"#,
    )
    .expect("write valid and malformed catalog rows");
    fs::write(
        ship_dir.join("1.json"),
        r#"{
            "grade":1,
            "tiers":[{"tier":1,"components":[]}],
            "levels":[{"level":1,"shield":0,"health":0}],
            "ability":[{"id":1,"value_is_percentage":true,"values":[{"value":10}]}]
        }"#,
    )
    .expect("write minimal upstream ship");

    let run = Command::new(env!("CARGO_BIN_EXE_normalize_data_stfc_space"))
        .current_dir(&root)
        .env("STFCSPACE_SHIPS_VERSION", "test-version")
        .output()
        .expect("run normalizer against isolated source tree");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "normalizer failed: stdout={stdout}; stderr={stderr}"
    );

    let normalized: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("data/ships_extended/test_ship.json"))
            .expect("normalized isolated ship exists"),
    )
    .expect("normalized ship JSON");
    assert_eq!(
        normalized["abilities"][0]["id"], "1",
        "the valid ability should survive a malformed sibling catalog row"
    );
    assert_eq!(normalized["abilities"][0]["value"], 0.1);

    fs::remove_dir_all(&root).expect("remove isolated source tree");
}
