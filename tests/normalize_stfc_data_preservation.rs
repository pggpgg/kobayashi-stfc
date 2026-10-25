use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::data::hostile::HostileIndex;
use kobayashi::data::registry::Registry;

#[test]
fn legacy_normalizer_preserves_modern_hostiles_and_unrelated_registry_rows() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = repo.join("target/test-tmp");
    fs::create_dir_all(&scratch).expect("scratch root");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let fixture = scratch.join(format!("legacy-normalizer-{nonce}"));
    let upstream = fixture.join("data/upstream/stfccommunity-data");
    let hostiles = fixture.join("data/hostiles");
    fs::create_dir_all(&upstream).expect("upstream dir");
    fs::create_dir_all(&hostiles).expect("hostiles dir");
    fs::write(
        upstream.join("legacy_scout.json"),
        r#"{"hostile_name":"Legacy Scout","level":1,"ship_class":"explorer","stats":{"defense":{"armor":1,"dodge":2,"shield_deflect":3},"health":{"hull_health":10,"shield_health":20}}}"#,
    )
    .expect("legacy input");
    fs::copy(
        repo.join("data/hostiles/2918121098.json"),
        hostiles.join("2918121098.json"),
    )
    .expect("modern hostile record");
    fs::write(
        hostiles.join("index.json"),
        r#"{"data_version":"modern-v1","source_note":"modern source","hostiles":[{"id":"2918121098","hostile_name":"Modern Hostile","level":81,"ship_class":"battleship"}]}"#,
    )
    .expect("modern hostile index");
    fs::write(
        fixture.join("data/registry.json"),
        r#"{"hostiles":{"source":"modern","data_version":"modern-v1","path":"hostiles/index.json"},"ships":{"source":"modern","path":"ships_extended/index.json"},"officer_eligibility":{"source":"reviewed","path":"officers/eligibility_matrix.json"},"syndicate_reputation":{"source":"reviewed","path":"syndicate_reputation.json"}}"#,
    )
    .expect("registry");

    let output = Command::new(env!("CARGO_BIN_EXE_normalize_stfc_data"))
        .env("CARGO_MANIFEST_DIR", &fixture)
        .output()
        .expect("legacy normalizer process");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let index: HostileIndex =
        serde_json::from_slice(&fs::read(hostiles.join("index.json")).expect("index"))
            .expect("hostile index schema");
    assert!(index.hostiles.iter().any(|row| row.id == "legacy_scout"));
    assert!(
        index.hostiles.iter().any(|row| row.id == "2918121098"),
        "the modern numeric hostile must remain resolvable"
    );
    assert_eq!(index.data_version.as_deref(), Some("modern-v1"));
    let registry: Registry = serde_json::from_slice(
        &fs::read(fixture.join("data/registry.json")).expect("registry after normalize"),
    )
    .expect("registry schema");
    for key in [
        "hostiles",
        "ships",
        "officer_eligibility",
        "syndicate_reputation",
    ] {
        assert!(registry.contains_key(key), "missing {key} registry row");
    }
    assert_eq!(registry["hostiles"].source, "modern");

    assert!(fixture.starts_with(&scratch));
    fs::remove_dir_all(fixture).expect("cleanup fixture");
}

#[test]
fn malformed_legacy_detail_never_becomes_a_zero_hull_hostile() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = repo.join("target/test-tmp");
    fs::create_dir_all(&scratch).expect("scratch root");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let fixture = scratch.join(format!("legacy-normalizer-corrupt-{nonce}"));
    let upstream = fixture.join("data/upstream/stfccommunity-data");
    fs::create_dir_all(&upstream).expect("upstream dir");
    fs::write(upstream.join("broken.json"), "{not JSON").expect("corrupt upstream detail");

    let output = Command::new(env!("CARGO_BIN_EXE_normalize_stfc_data"))
        .env("CARGO_MANIFEST_DIR", &fixture)
        .output()
        .expect("legacy normalizer process");
    assert!(
        !output.status.success(),
        "corrupt input must fail instead of publishing a synthetic zero-hull hostile"
    );
    assert!(!fixture.join("data/hostiles/broken.json").exists());
    assert!(fixture.starts_with(&scratch));
    fs::remove_dir_all(fixture).expect("cleanup fixture");
}
