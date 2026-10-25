//! Redacted, source-bound observations from one no-Tom Jem'Hadar control.
//! This constrains a piercing-layer hypothesis without calibrating a full fight.

use kobayashi::combat::mitigation::mitigation;
use kobayashi::combat::types::{AttackerStats, DefenderStats, ShipType};
use serde::Deserialize;
use std::collections::BTreeMap;

const FIXTURE: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_baseline_20261024_observed.json");

#[derive(Deserialize)]
struct Fixture {
    source_json_sha256: String,
    source_projection_sha256: String,
    source_setup_validation_sha256: String,
    source_hypothesis_comparison_sha256: String,
    battle_time: String,
    observed_client_build: u32,
    parser_reference_client_build: u32,
    setup_check_summary: Setup,
    source_derived_defense: Defense,
    observation: Observation,
    limitations: Vec<String>,
}

#[derive(Deserialize)]
struct Setup {
    all_checks_pass: bool,
    tom_paris_absent: bool,
    checked_flags: BTreeMap<String, bool>,
}

#[derive(Deserialize)]
struct Defense {
    base: [f64; 3],
    launch_unrounded: [f64; 3],
    combat_start_all_defenses_fraction: f64,
    combat_unrounded: [f64; 3],
    combat_native_rounded: [f64; 3],
}

#[derive(Deserialize)]
struct Observation {
    rounds: u32,
    subrounds: u32,
    incoming_hits: Vec<Hit>,
    primary_clean_source_index: usize,
    diagnostic_denominator_error_budget: f64,
    primary_clean_mitigation_interval: [f64; 2],
    piercing_hypotheses: Vec<PiercingHypothesis>,
    hp_ledger_exceptions: Vec<String>,
}

#[derive(Deserialize)]
struct Hit {
    round: u32,
    subround: u32,
    source_index: usize,
    weapon_id: u64,
    shot_hit: u8,
    is_critical: u8,
    hull_damage: f64,
    shield_damage: f64,
    mitigated_damage: f64,
    mitigated_apex_barrier: f64,
    isolytic_damage: f64,
    mitigated_isolytic_damage: f64,
    mitigated_critical_damage: f64,
    mitigated_isolytic_critical_damage: f64,
    remaining_hull: f64,
    remaining_shield: f64,
}

#[derive(Deserialize)]
struct PiercingHypothesis {
    name: String,
    armor_piercing: f64,
    shield_piercing: f64,
    accuracy: f64,
    native_defense_plusminus_one_band: [f64; 2],
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("redacted Jem'Hadar baseline fixture should parse")
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected} ± {tolerance}, observed {actual}"
    );
}

fn overlaps(a: [f64; 2], b: [f64; 2]) -> bool {
    a[0] <= b[1] && b[0] <= a[1]
}

fn reconstructed_raw(hit: &Hit) -> f64 {
    hit.hull_damage + hit.shield_damage + hit.mitigated_damage + hit.mitigated_apex_barrier
        - hit.isolytic_damage
}

fn diagnostic_interval(hit: &Hit, budget: f64) -> [f64; 2] {
    let raw = reconstructed_raw(hit);
    assert!(raw > budget);
    [
        hit.mitigated_damage / (raw + budget),
        hit.mitigated_damage / (raw - budget),
    ]
}

fn defense_cube_band(defense: [f64; 3], piercing: &PiercingHypothesis) -> [f64; 2] {
    let mut band = [f64::INFINITY, f64::NEG_INFINITY];
    for armor_offset in -1..=1 {
        for shield_offset in -1..=1 {
            for dodge_offset in -1..=1 {
                let m = mitigation(
                    DefenderStats {
                        armor: defense[0] + f64::from(armor_offset),
                        shield_deflection: defense[1] + f64::from(shield_offset),
                        dodge: defense[2] + f64::from(dodge_offset),
                    },
                    AttackerStats {
                        armor_piercing: piercing.armor_piercing,
                        shield_piercing: piercing.shield_piercing,
                        accuracy: piercing.accuracy,
                    },
                    ShipType::Explorer,
                );
                band[0] = band[0].min(m);
                band[1] = band[1].max(m);
            }
        }
    }
    band
}

#[test]
fn setup_provenance_is_frozen_and_seventeen_checks_passed() {
    let f = fixture();
    assert_eq!(f.battle_time, "2026-10-24T02:54:29");
    assert_eq!(f.observed_client_build, 273);
    assert_eq!(f.parser_reference_client_build, 272);
    assert_eq!(
        f.source_json_sha256,
        "5ea33b6061ae83e072c5e7e4bde33d9042f4ce7d7c1c8dd5ced34e766a04cc50"
    );
    assert_eq!(
        f.source_projection_sha256,
        "fd8df1fa39c67d0027cbb17292dfcb4d21499b2e2a4b1f61cd999dec5ea4ced2"
    );
    for hash in [
        &f.source_setup_validation_sha256,
        &f.source_hypothesis_comparison_sha256,
    ] {
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    assert!(f.setup_check_summary.all_checks_pass);
    assert!(f.setup_check_summary.tom_paris_absent);
    assert_eq!(f.setup_check_summary.checked_flags.len(), 17);
    assert!(f
        .setup_check_summary
        .checked_flags
        .values()
        .all(|pass| *pass));
    assert_eq!(f.source_derived_defense.base, [3.0, 22.0, 1.0]);
    assert_eq!(
        f.source_derived_defense.launch_unrounded,
        [86.37, 677.38, 29.19]
    );
    close(
        f.source_derived_defense.combat_start_all_defenses_fraction,
        13.0,
        1e-12,
    );
    for axis in 0..3 {
        close(
            f.source_derived_defense.combat_unrounded[axis],
            f.source_derived_defense.launch_unrounded[axis]
                + f.source_derived_defense.base[axis]
                    * f.source_derived_defense.combat_start_all_defenses_fraction,
            1e-10,
        );
    }
    assert_eq!(
        f.source_derived_defense.combat_native_rounded,
        [125.0, 963.0, 42.0]
    );
    assert_eq!(f.observation.rounds, 1);
    assert_eq!(f.observation.subrounds, 4);
    assert_eq!(f.observation.incoming_hits.len(), 3);
    assert!(f
        .limitations
        .iter()
        .any(|s| s.contains("Only the stronger component")));
}

#[test]
fn clean_hit_overlaps_its_weapon_component_but_rejects_aggregate_band() {
    let f = fixture();
    let o = &f.observation;
    assert_eq!(o.primary_clean_source_index, 7);
    assert_eq!(o.piercing_hypotheses.len(), 2);
    close(o.diagnostic_denominator_error_budget, 2.01, 1e-12);
    let hit = &o.incoming_hits[0];
    assert_eq!(hit.source_index, o.primary_clean_source_index);
    assert!(hit.remaining_hull > 0.0 && hit.remaining_shield > 0.0);
    assert_eq!(hit.shot_hit, 1);
    assert_eq!(hit.is_critical, 0);
    let observed = diagnostic_interval(hit, o.diagnostic_denominator_error_budget);
    for (actual, expected) in observed.iter().zip(o.primary_clean_mitigation_interval) {
        close(*actual, expected, 1e-12);
    }
    let local = &o.piercing_hypotheses[0];
    let aggregate = &o.piercing_hypotheses[1];
    assert_eq!(local.name, "observed_weapon_component");
    assert_eq!(aggregate.name, "normalized_unweighted_component_average");
    assert_eq!(
        [local.armor_piercing, local.shield_piercing, local.accuracy],
        [803768.0, 1298802.0, 765496.0]
    );
    for (hypothesis, expected_overlap) in [(local, true), (aggregate, false)] {
        let calculated =
            defense_cube_band(f.source_derived_defense.combat_native_rounded, hypothesis);
        // The immutable pre-observation bands used decimal 1.1. The engine now
        // preserves the native float32 offset. Verify its actual observational
        // conclusion separately, without rewriting the frozen prediction.
        assert_eq!(overlaps(observed, calculated), expected_overlap);
        assert!(overlaps(
            calculated,
            hypothesis.native_defense_plusminus_one_band
        ));
    }
    assert!(overlaps(observed, local.native_defense_plusminus_one_band));
    assert!(!overlaps(
        observed,
        aggregate.native_defense_plusminus_one_band
    ));
}

#[test]
fn all_three_assigned_damage_records_preserve_depletion_and_overkill() {
    let f = fixture();
    let o = &f.observation;
    let hits = &o.incoming_hits;
    assert_eq!(
        hits.iter().map(|h| h.source_index).collect::<Vec<_>>(),
        [7, 26, 45]
    );
    for hit in hits {
        assert_eq!((hit.round, hit.subround, hit.weapon_id), (1, 1, 2806302352));
        assert_eq!((hit.shot_hit, hit.is_critical), (1, 0));
        assert!(hit.hull_damage >= 0.0 && hit.shield_damage >= 0.0);
        assert!(hit.mitigated_damage > 0.0 && hit.mitigated_apex_barrier > 0.0);
        assert!(reconstructed_raw(hit) > 0.0);
        assert_eq!(hit.isolytic_damage, 0.0);
        assert_eq!(hit.mitigated_isolytic_damage, 0.0);
        assert_eq!(hit.mitigated_critical_damage, 0.0);
        assert_eq!(hit.mitigated_isolytic_critical_damage, 0.0);
        assert!(overlaps(
            diagnostic_interval(hit, o.diagnostic_denominator_error_budget),
            o.piercing_hypotheses[0].native_defense_plusminus_one_band
        ));
    }

    // The clean first hit exactly matches both HP deltas.
    close(
        652159.0 - hits[0].hull_damage,
        hits[0].remaining_hull,
        1e-12,
    );
    close(
        748507.0 - hits[0].shield_damage,
        hits[0].remaining_shield,
        1e-12,
    );
    // Depletion retains assigned shield damage; it is not the remaining-HP delta.
    close(
        hits[0].remaining_hull - hits[1].hull_damage,
        hits[1].remaining_hull,
        1e-12,
    );
    assert_eq!(hits[1].remaining_shield, 0.0);
    close(
        hits[1].shield_damage - hits[0].remaining_shield,
        192239.0,
        1e-12,
    );
    // Terminal assigned hull damage also exceeds available hull.
    assert_eq!(
        (hits[2].remaining_hull, hits[2].remaining_shield),
        (0.0, 0.0)
    );
    close(hits[2].hull_damage - hits[1].remaining_hull, 99181.0, 1e-12);
    assert_eq!(o.hp_ledger_exceptions.len(), 2);
}
