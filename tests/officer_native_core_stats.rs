//! Native officer core stats are rounded at the upstream source-load boundary.
//! Hand-authored LCARS rows remain as written, including fractional values.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use kobayashi::combat::{AbilityEffect, TimingWindow};
use kobayashi::data::native_officer_core_stats::stats_for_native_officer;
use kobayashi::lcars::{
    build_officer_model, build_officer_model_file_default, index_lcars_officers_by_id,
    resolve_crew_to_buff_set, LcarsLevelStats, ResolveOptions, DEFAULT_INPUT, DEFAULT_SUMMARY,
    DEFAULT_TRANSLATIONS,
};

const TOM_ID: &str = "tom-paris-3640cc";
const JADZIA_ID: &str = "jadzia-dax-736698";

fn observed_crew_options() -> ResolveOptions {
    ResolveOptions {
        tier: None,
        officer_tiers: Some(HashMap::from([(TOM_ID.into(), 4), (JADZIA_ID.into(), 1)])),
        officer_levels: Some(HashMap::from([(TOM_ID.into(), 20), (JADZIA_ID.into(), 5)])),
    }
}

#[test]
fn native_catalog_selects_only_verified_source_ids_and_levels() {
    let tom = stats_for_native_officer(3_849_424_588, 20).unwrap();
    let jadzia = stats_for_native_officer(1_916_661_181, 5).unwrap();
    assert_eq!((tom.attack, tom.defense, tom.health), (252, 421, 505));
    assert_eq!(
        (jadzia.attack, jadzia.defense, jadzia.health),
        (249, 19, 268)
    );
    assert!(stats_for_native_officer(u64::MAX, 20).is_none());
    assert!(stats_for_native_officer(3_849_424_588, 0).is_none());
    assert!(stats_for_native_officer(3_849_424_588, 10_000).is_none());
}

#[test]
fn source_backed_core_stats_flow_into_crew_totals_and_tom_coefficient() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let tom = officers.get(TOM_ID).unwrap().stats_at_level(20).unwrap();
    let jadzia = officers.get(JADZIA_ID).unwrap().stats_at_level(5).unwrap();

    // The native CreateCoreStats path rounds each f32-weighted axis before crew summation.
    assert_eq!((tom.attack, tom.defense, tom.health), (252.0, 421.0, 505.0));
    assert_eq!(
        (jadzia.attack, jadzia.defense, jadzia.health),
        (249.0, 19.0, 268.0)
    );

    let resolved = resolve_crew_to_buff_set(
        JADZIA_ID,
        &[],
        &[TOM_ID.into()],
        &officers,
        &observed_crew_options(),
    );
    assert_eq!(
        (
            resolved.officer_stat_totals.attack,
            resolved.officer_stat_totals.defense,
            resolved.officer_stat_totals.health,
        ),
        (501.0, 440.0, 773.0)
    );
    assert_eq!(resolved.bridge_officer_stat_totals.defense, 19.0);

    let tom_seat = resolved
        .crew
        .seats
        .iter()
        .find(|seat| seat.officer_id.as_deref() == Some(TOM_ID))
        .expect("Tom's below-decks ability must resolve");
    assert_eq!(tom_seat.ability.timing, TimingWindow::CombatBegin);
    assert_eq!(
        tom_seat.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect {
            coefficient: 4.0,
            chance: 1.0
        }
    );
}

#[test]
fn reviewed_newlines_apply_native_stats_but_changed_bytes_keep_fractional_rows() {
    let temp = std::env::temp_dir().join(format!(
        "kobayashi-native-core-source-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&temp).unwrap();
    struct RemoveDir(std::path::PathBuf);
    impl Drop for RemoveDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _remove_dir = RemoveDir(temp.clone());
    let source = Path::new("data/upstream/data-stfc-space/officers/3849424588.json");
    let raw = fs::read_to_string(source).unwrap();
    let lf = raw.replace("\r\n", "\n");
    assert!(
        lf.contains('\n'),
        "source must exercise newline normalization"
    );
    let crlf = lf.replace('\n', "\r\n");
    let load_tom = |bytes: &str| {
        fs::write(temp.join("3849424588.json"), bytes).unwrap();
        let officers = index_lcars_officers_by_id(
            build_officer_model(
                Path::new(DEFAULT_INPUT),
                Path::new(DEFAULT_SUMMARY),
                Path::new(DEFAULT_TRANSLATIONS),
                &temp,
                true,
            )
            .unwrap(),
        );
        let tom = officers.get(TOM_ID).unwrap().stats_at_level(20).unwrap();
        (tom.attack, tom.defense, tom.health)
    };

    // Both independently reviewed byte forms are accepted at the source-load boundary.
    assert_eq!(load_tom(&lf), (252.0, 421.0, 505.0));
    assert_eq!(load_tom(&crlf), (252.0, 421.0, 505.0));

    // Changing one semantically inert byte breaks the reviewed source hash and retains upstream
    // fractions. A string-typed ID similarly remains readable but does not become native-bound.
    assert_eq!(load_tom(&format!("{lf} ")), (252.36, 420.6, 504.72));
    let string_id = lf.replacen("\"id\": 3849424588", "\"id\": \"3849424588\"", 1);
    assert_ne!(string_id, lf);
    assert_eq!(load_tom(&string_id), (252.36, 420.6, 504.72));
}

#[test]
fn hand_authored_lcars_fractional_stat_remains_fractional() {
    let mut custom = build_officer_model_file_default()
        .unwrap()
        .officers
        .into_iter()
        .find(|officer| officer.id == TOM_ID)
        .unwrap();
    custom.id = "manual-fractional-defense".into();
    custom.stats = vec![LcarsLevelStats {
        level: 20,
        attack: 252.36,
        defense: 420.6,
        health: 504.72,
    }];
    let custom_id = custom.id.clone();
    let officers = index_lcars_officers_by_id(vec![custom]);
    assert_eq!(
        officers[&custom_id].stats_at_level(20).unwrap().defense,
        420.6
    );

    let resolved = resolve_crew_to_buff_set(
        "unused-captain",
        &[],
        std::slice::from_ref(&custom_id),
        &officers,
        &ResolveOptions {
            tier: Some(4),
            officer_tiers: None,
            officer_levels: Some(HashMap::from([(custom_id.clone(), 20)])),
        },
    );
    assert_eq!(resolved.officer_stat_totals.defense, 420.6);
    assert_eq!(resolved.crew.seats.len(), 1);
    assert_eq!(
        resolved.crew.seats[0].ability.effect,
        AbilityEffect::UnsupportedOfficerEffect {
            coefficient: 4.0,
            chance: 1.0
        }
    );
}
