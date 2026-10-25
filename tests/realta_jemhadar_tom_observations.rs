//! Source observations from the paired Jem'Hadar Tom control.
//! The frozen six-family mismatch is recorded without fitting a replacement rule.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_tom_20261024_observed.json");

#[derive(Deserialize)]
struct Fixture {
    source_json_sha256: String,
    source_projection_sha256: String,
    source_setup_validation_v2_sha256: String,
    source_hypothesis_comparison_sha256: String,
    source_frozen_pre_observation_predictions_sha256: String,
    paired_baseline_hypothesis_comparison_sha256: String,
    battle_time: String,
    paired_baseline_time: String,
    observed_client_build: u32,
    parser_reference_client_build: u32,
    setup_check_summary: Setup,
    observed: Observed,
    limitations: Vec<String>,
}

#[derive(Deserialize)]
struct Setup {
    all_checks_pass: bool,
    checked_flags: BTreeMap<String, bool>,
    sole_paired_crew_change: String,
}

#[derive(Deserialize)]
struct Observed {
    rounds: u32,
    subrounds: usize,
    total_damage_hits: usize,
    outgoing_damage_hits: usize,
    tom_ability_before_attacks: Vec<TomAbility>,
    incoming_hits: Vec<Hit>,
    diagnostic_denominator_error_budget: f64,
    stronger_weapon: StrongerWeapon,
    weaker_weapon: WeakerWeapon,
    hp_ledger_exceptions: Vec<LedgerException>,
}

#[derive(Deserialize)]
struct TomAbility {
    source_index: usize,
    officer_id: u64,
    ability_id: u64,
    value: f64,
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
struct StrongerWeapon {
    weapon_id: u64,
    directly_tested_in_paired_baseline: bool,
    clean_noncritical_count: usize,
    clean_noncritical_interval: [f64; 2],
    clean_critical_count: usize,
    clean_critical_interval: [f64; 2],
    primary_combined_count: usize,
    primary_combined_interval: [f64; 2],
    frozen_source_placement_families: Vec<FrozenFamily>,
}

#[derive(Deserialize)]
struct FrozenFamily {
    source: String,
    placement: String,
    frozen_osr1_band: [f64; 2],
    frozen_osr0to1_fallback_band: [f64; 2],
}

#[derive(Deserialize)]
struct WeakerWeapon {
    weapon_id: u64,
    primary_clean_count: usize,
    conditional_depleted_terminal_count: usize,
    conditional_depleted_terminal_interval: [f64; 2],
}

#[derive(Deserialize)]
struct LedgerException {
    source_index: usize,
    kind: String,
    assigned_excess: Option<f64>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("redacted Jem'Hadar Tom fixture should parse")
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

fn joint_interval(hits: &[&Hit], budget: f64) -> [f64; 2] {
    hits.iter().fold([0.0_f64, f64::INFINITY], |joint, hit| {
        let interval = diagnostic_interval(hit, budget);
        [joint[0].max(interval[0]), joint[1].min(interval[1])]
    })
}

#[test]
fn paired_setup_and_tom_ability_are_source_bound() {
    let f = fixture();
    assert_eq!(f.battle_time, "2026-10-24T07:38:32");
    assert_eq!(f.paired_baseline_time, "2026-10-24T02:54:29");
    assert_eq!(f.observed_client_build, 273);
    assert_eq!(f.parser_reference_client_build, 272);
    assert_eq!(
        f.source_json_sha256,
        "4dee4ed472ec5c8345a2f1c355d22d32f36c05b44b94f6b6241258a8c6cb0130"
    );
    assert_eq!(
        f.source_projection_sha256,
        "451610a7077f40723775f14d29d8fe78b8df17f6e91eedecb091370fbd022f39"
    );
    assert_eq!(
        f.source_setup_validation_v2_sha256,
        "cb993dfa9c9647f8cb55174e351586c2a8e87d61670dce094446133e9ec354bf"
    );
    assert_eq!(
        f.source_hypothesis_comparison_sha256,
        "142cbdee483eb37265766c30550e3e5b2b88a05a2efcade76020092c2ab5d23e"
    );
    assert_eq!(
        f.source_frozen_pre_observation_predictions_sha256,
        "2340e0207ca52a358e4b5032659fa65ab9cdcd7a940959b2ef1f9cd95a291948"
    );
    assert_eq!(
        f.paired_baseline_hypothesis_comparison_sha256,
        "d4c8b23cadd8012a1e0a768f7054b5e1bae913ef67b7cda9e27671f4b7c48706"
    );
    assert!(f.setup_check_summary.all_checks_pass);
    assert_eq!(f.setup_check_summary.checked_flags.len(), 20);
    assert!(f
        .setup_check_summary
        .checked_flags
        .values()
        .all(|pass| *pass));
    assert!(f
        .setup_check_summary
        .sole_paired_crew_change
        .contains("below-decks slot 3"));
    assert_eq!(f.observed.rounds, 1);
    assert_eq!(f.observed.subrounds, 4);
    assert_eq!(f.observed.total_damage_hits, 11);
    assert_eq!(f.observed.outgoing_damage_hits, 2);
    assert_eq!(f.observed.incoming_hits.len(), 9);
    assert_eq!(f.observed.tom_ability_before_attacks.len(), 1);
    let ability = &f.observed.tom_ability_before_attacks[0];
    assert_eq!(
        (ability.source_index, ability.officer_id, ability.ability_id),
        (4, 3849424588, 3192273655)
    );
    close(ability.value, 4.0, 1e-12);
    assert!(f.limitations.iter().any(|s| s.contains("No winner")));
}

#[test]
fn four_clean_stronger_weapon_hits_reject_all_six_frozen_families() {
    let f = fixture();
    let o = &f.observed;
    close(o.diagnostic_denominator_error_budget, 2.01, 1e-12);
    let strong = &o.stronger_weapon;
    assert_eq!(strong.weapon_id, 2806302352);
    assert!(strong.directly_tested_in_paired_baseline);
    let clean: Vec<_> = o
        .incoming_hits
        .iter()
        .filter(|hit| {
            hit.weapon_id == strong.weapon_id
                && hit.shot_hit == 1
                && hit.remaining_hull > 0.0
                && hit.remaining_shield > 0.0
        })
        .collect();
    let noncritical: Vec<_> = clean
        .iter()
        .copied()
        .filter(|hit| hit.is_critical == 0)
        .collect();
    let critical: Vec<_> = clean
        .iter()
        .copied()
        .filter(|hit| hit.is_critical == 1)
        .collect();
    assert_eq!((noncritical.len(), critical.len(), clean.len()), (3, 1, 4));
    assert_eq!(noncritical.len(), strong.clean_noncritical_count);
    assert_eq!(critical.len(), strong.clean_critical_count);
    assert_eq!(clean.len(), strong.primary_combined_count);
    for (actual, recorded) in [
        (
            joint_interval(&noncritical, o.diagnostic_denominator_error_budget),
            strong.clean_noncritical_interval,
        ),
        (
            joint_interval(&critical, o.diagnostic_denominator_error_budget),
            strong.clean_critical_interval,
        ),
        (
            joint_interval(&clean, o.diagnostic_denominator_error_budget),
            strong.primary_combined_interval,
        ),
    ] {
        assert!(actual[0] <= actual[1]);
        close(actual[0], recorded[0], 1e-12);
        close(actual[1], recorded[1], 1e-12);
    }
    assert!(reconstructed_raw(critical[0]).fract().abs() > 1e-6);
    assert!(overlaps(
        strong.clean_critical_interval,
        strong.clean_noncritical_interval
    ));
    assert_eq!(strong.frozen_source_placement_families.len(), 6);
    let identities: BTreeSet<_> = strong
        .frozen_source_placement_families
        .iter()
        .map(|family| (family.source.as_str(), family.placement.as_str()))
        .collect();
    assert_eq!(identities.len(), 6);
    for family in &strong.frozen_source_placement_families {
        assert!(family.frozen_osr1_band[0] <= family.frozen_osr1_band[1]);
        assert!(family.frozen_osr0to1_fallback_band[0] <= family.frozen_osr0to1_fallback_band[1]);
        assert!(!overlaps(
            strong.primary_combined_interval,
            family.frozen_osr1_band
        ));
        assert!(!overlaps(
            strong.primary_combined_interval,
            family.frozen_osr0to1_fallback_band
        ));
    }
}

#[test]
fn assigned_damage_retains_depletion_terminal_and_conditional_kinetic_evidence() {
    let f = fixture();
    let o = &f.observed;
    let hits = &o.incoming_hits;
    assert_eq!(
        hits.iter().map(|hit| hit.source_index).collect::<Vec<_>>(),
        vec![13, 32, 51, 95, 114, 133, 177, 196, 215]
    );
    for hit in hits {
        assert_eq!(hit.round, 1);
        assert_eq!((hit.shot_hit, hit.mitigated_isolytic_damage), (1, 0.0));
        assert_eq!(hit.isolytic_damage, 0.0);
        assert_eq!(hit.mitigated_critical_damage, 0.0);
        assert_eq!(hit.mitigated_isolytic_critical_damage, 0.0);
        assert!(hit.hull_damage >= 0.0 && hit.shield_damage >= 0.0);
        assert!(hit.mitigated_damage > 0.0 && hit.mitigated_apex_barrier > 0.0);
        assert!(reconstructed_raw(hit) > o.diagnostic_denominator_error_budget);
    }
    assert_eq!(
        hits[..3].iter().map(|hit| hit.subround).collect::<Vec<_>>(),
        vec![1; 3]
    );
    assert_eq!(
        hits[3..6]
            .iter()
            .map(|hit| hit.subround)
            .collect::<Vec<_>>(),
        vec![2; 3]
    );
    assert_eq!(
        hits[6..].iter().map(|hit| hit.subround).collect::<Vec<_>>(),
        vec![3; 3]
    );
    // Before depletion, assigned physical damage equals the HP delta.
    let mut hull = 652159.0;
    let mut shield = 748507.0;
    for hit in &hits[..4] {
        hull -= hit.hull_damage;
        shield -= hit.shield_damage;
        close(hit.remaining_hull, hull, 1e-12);
        close(hit.remaining_shield, shield, 1e-12);
    }
    assert_eq!(hits[3].is_critical, 1);
    assert_eq!(hits[4].is_critical, 0);
    // The fifth shot assigns more shield damage than the shield has left.
    close(hits[4].shield_damage - shield, 103216.0, 1e-12);
    assert_eq!(hits[4].remaining_shield, 0.0);
    close(hits[4].remaining_hull, hull - hits[4].hull_damage, 1e-12);
    close(
        hits[5].remaining_hull,
        hits[4].remaining_hull - hits[5].hull_damage,
        1e-12,
    );
    assert_eq!(hits[5].remaining_shield, 0.0);
    // The seventh shot is terminal; two later assigned hits still appear.
    close(
        hits[6].hull_damage - hits[5].remaining_hull,
        194483.0,
        1e-12,
    );
    assert!(hits[6..]
        .iter()
        .all(|hit| hit.remaining_hull == 0.0 && hit.remaining_shield == 0.0));
    assert!(hits[7..].iter().all(|hit| hit.hull_damage > 0.0));
    assert_eq!(o.hp_ledger_exceptions.len(), 4);
    assert_eq!(
        o.hp_ledger_exceptions
            .iter()
            .map(|exception| exception.source_index)
            .collect::<Vec<_>>(),
        vec![114, 177, 196, 215]
    );
    assert_eq!(o.hp_ledger_exceptions[0].assigned_excess, Some(103216.0));
    assert_eq!(o.hp_ledger_exceptions[1].assigned_excess, Some(194483.0));
    assert!(o.hp_ledger_exceptions[0]
        .kind
        .contains("shield_overassigned"));
    assert!(o.hp_ledger_exceptions[1].kind.contains("hull_overassigned"));
    assert!(o.hp_ledger_exceptions[2..]
        .iter()
        .all(|exception| exception.assigned_excess.is_none()));

    let weak = &o.weaker_weapon;
    assert_eq!(weak.weapon_id, 519835226);
    assert_eq!(weak.primary_clean_count, 0);
    let kinetic: Vec<_> = hits
        .iter()
        .filter(|hit| hit.weapon_id == weak.weapon_id)
        .collect();
    assert_eq!(kinetic.len(), weak.conditional_depleted_terminal_count);
    assert!(kinetic
        .iter()
        .all(|hit| hit.remaining_hull == 0.0 && hit.remaining_shield == 0.0));
    let conditional = joint_interval(&kinetic, o.diagnostic_denominator_error_budget);
    assert!(conditional[0] <= conditional[1]);
    close(
        conditional[0],
        weak.conditional_depleted_terminal_interval[0],
        1e-12,
    );
    close(
        conditional[1],
        weak.conditional_depleted_terminal_interval[1],
        1e-12,
    );
}
