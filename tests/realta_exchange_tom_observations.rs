//! Redacted Tom Paris paired-control observations, not full Tom engine calibration.
//! Reconstructed raw values and their nearest integers are diagnostics, not
//! server-side fields or proof of a damage numeric type.

use serde::Deserialize;

const FIXTURE: &str =
    include_str!("fixtures/recorded_fights/realta_exchange_tom_20261023_observed.json");

#[derive(Deserialize)]
struct Fixture {
    source_json_sha256: String,
    source_projection_sha256: String,
    battle_time: String,
    observed_client_build: u32,
    parser_reference_client_build: u32,
    setup_check_summary: SetupSummary,
    observed: Observed,
}

#[derive(Deserialize)]
struct SetupSummary {
    checked_flags: std::collections::BTreeMap<String, bool>,
}

#[derive(Deserialize)]
struct Observed {
    rounds: u32,
    subrounds: usize,
    total_damage_hits: usize,
    outgoing_damage_hits: usize,
    incoming_hits: Vec<Hit>,
    hull_repairs: Vec<Repair>,
    charge_events: Vec<Charge>,
    mitigation: Mitigation,
    shield_share: ShieldShare,
}

#[derive(Deserialize)]
struct Hit {
    round: u32,
    source_index: usize,
    shot_hit: u8,
    is_critical: u8,
    hull_damage: f64,
    shield_damage: f64,
    mitigated_damage: f64,
    isolytic_damage: f64,
    mitigated_isolytic_damage: f64,
    mitigated_apex_barrier: f64,
    mitigated_critical_damage: f64,
    mitigated_isolytic_critical_damage: f64,
    remaining_hull: f64,
    remaining_shield: f64,
}

#[derive(Deserialize)]
struct Repair {
    round: u32,
    source_index: usize,
    amount: f64,
}

#[derive(Deserialize)]
struct Charge {
    round: u32,
    source_index: usize,
    charge_percentage: f64,
}

#[derive(Deserialize)]
struct Mitigation {
    clean_hit_count: usize,
    conservative_component_error_budget: f64,
    joint_diagnostic_interval: [f64; 2],
    nearest_integer_reconstruction_ratio_range: [f64; 2],
    nearest_integer_is_hypothesis: bool,
    exact_coefficient_0_712_fractional_raw_also_fits: bool,
}

#[derive(Deserialize)]
struct ShieldShare {
    clean_rounds_1_to_2_count: usize,
    clean_rounds_3_plus_count: usize,
    first_zero_remaining_shield: EventIndex,
    terminal_zero_remaining_hull: EventIndex,
}

#[derive(Deserialize)]
struct EventIndex {
    round: u32,
    source_index: usize,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("redacted Tom Exchange fixture should parse")
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected} ± {tolerance}, observed {actual}"
    );
}

#[test]
fn clean_hits_preserve_diagnostic_mitigation_interval_without_claiming_integer_raw() {
    let fixture = fixture();
    assert_eq!(fixture.source_json_sha256.len(), 64);
    assert_eq!(fixture.source_projection_sha256.len(), 64);
    assert_eq!(fixture.battle_time, "2026-10-23T19:11:30");
    assert_eq!(fixture.observed_client_build, 273);
    assert_eq!(fixture.parser_reference_client_build, 272);
    assert_eq!(fixture.setup_check_summary.checked_flags.len(), 29);
    assert!(fixture
        .setup_check_summary
        .checked_flags
        .values()
        .all(|v| *v));

    let observed = &fixture.observed;
    assert_eq!(observed.rounds, 47);
    assert_eq!(observed.subrounds, 94);
    assert_eq!(observed.total_damage_hits, 235);
    assert_eq!(observed.outgoing_damage_hits, 93);
    assert_eq!(observed.incoming_hits.len(), 142);
    let clean: Vec<_> = observed
        .incoming_hits
        .iter()
        .filter(|hit| hit.shot_hit == 1 && hit.is_critical == 0 && hit.remaining_shield > 0.0)
        .collect();
    assert_eq!(clean.len(), 82);
    assert_eq!(clean.len(), observed.mitigation.clean_hit_count);
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.is_critical == 1)
            .count(),
        19
    );
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.is_critical == 0 && hit.remaining_shield == 0.0)
            .count(),
        41
    );

    let budget = observed.mitigation.conservative_component_error_budget;
    close(budget, 2.01, 1e-12);
    let mut lower: f64 = 0.0;
    let mut upper = f64::INFINITY;
    let mut ratio_min = f64::INFINITY;
    let mut ratio_max: f64 = 0.0;
    let mut first_two = 0;
    let mut later = 0;
    for hit in clean {
        let reconstructed =
            hit.hull_damage + hit.shield_damage + hit.mitigated_damage + hit.mitigated_apex_barrier
                - hit.isolytic_damage;
        assert!(reconstructed > budget);
        lower = lower.max(hit.mitigated_damage / (reconstructed + budget));
        upper = upper.min(hit.mitigated_damage / (reconstructed - budget));

        // Retain the striking ratio as an observation. The denominator is an
        // inferred nearest integer, and 0.712 with fractional raw also fits.
        let diagnostic_ratio = hit.mitigated_damage / reconstructed.round();
        ratio_min = ratio_min.min(diagnostic_ratio);
        ratio_max = ratio_max.max(diagnostic_ratio);
        close(hit.isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_critical_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_critical_damage, 0.0, 1e-12);

        let share = hit.shield_damage / (hit.shield_damage + hit.hull_damage);
        if hit.round <= 2 {
            first_two += 1;
            close(share, 0.825, 7e-5);
        } else {
            later += 1;
            close(share, 0.8, 7e-5);
        }
    }
    assert_eq!(first_two, 6);
    assert_eq!(later, 76);
    assert_eq!(first_two, observed.shield_share.clean_rounds_1_to_2_count);
    assert_eq!(later, observed.shield_share.clean_rounds_3_plus_count);
    assert!(lower <= upper);
    close(
        lower,
        observed.mitigation.joint_diagnostic_interval[0],
        1e-12,
    );
    close(
        upper,
        observed.mitigation.joint_diagnostic_interval[1],
        1e-12,
    );
    close(
        ratio_min,
        observed
            .mitigation
            .nearest_integer_reconstruction_ratio_range[0],
        1e-12,
    );
    close(
        ratio_max,
        observed
            .mitigation
            .nearest_integer_reconstruction_ratio_range[1],
        1e-12,
    );
    assert!(lower < 0.712 && 0.712 < upper);
    assert!(observed.mitigation.nearest_integer_is_hypothesis);
    assert!(
        observed
            .mitigation
            .exact_coefficient_0_712_fractional_raw_also_fits
    );
}

#[test]
fn clean_critical_hits_share_the_noncritical_fractional_accounting_bracket() {
    let observed = fixture().observed;
    let criticals: Vec<_> = observed
        .incoming_hits
        .iter()
        .filter(|hit| {
            hit.shot_hit == 1
                && hit.is_critical == 1
                && hit.remaining_hull > 0.0
                && hit.remaining_shield > 0.0
        })
        .collect();
    assert_eq!(criticals.len(), 10);

    // This reference range comes from separate noncritical hits and their
    // diagnostic nearest-integer raw, not a claimed server-side coefficient.
    let reference = observed
        .mitigation
        .nearest_integer_reconstruction_ratio_range;
    assert!(observed.mitigation.nearest_integer_is_hypothesis);
    let budget = observed.mitigation.conservative_component_error_budget;
    close(budget, 2.01, 1e-12);
    let mut lower: f64 = 0.0;
    let mut upper = f64::INFINITY;
    let mut fractional_raw_seen = false;
    for hit in criticals {
        close(hit.isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_critical_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_critical_damage, 0.0, 1e-12);
        let reconstructed =
            hit.hull_damage + hit.shield_damage + hit.mitigated_damage + hit.mitigated_apex_barrier;
        assert!(reconstructed > budget);
        fractional_raw_seen |= reconstructed.fract().abs() > 1e-6;
        // Preserve fractional critical raw when forming the accounting interval.
        let interval = [
            hit.mitigated_damage / (reconstructed + budget),
            hit.mitigated_damage / (reconstructed - budget),
        ];
        assert!(interval[0] <= reference[0] && reference[1] <= interval[1]);
        lower = lower.max(interval[0]);
        upper = upper.min(interval[1]);
    }
    assert!(fractional_raw_seen);
    assert!(lower <= upper);
    close(lower, 0.711982842420685, 1e-12);
    close(upper, 0.7120211375642358, 1e-12);
    assert!(lower <= observed.mitigation.joint_diagnostic_interval[1]);
    assert!(upper >= observed.mitigation.joint_diagnostic_interval[0]);
}

#[test]
fn reported_physical_damage_reconstructs_all_hits_including_depletion_and_overkill() {
    let observed = fixture().observed;
    assert_eq!(observed.incoming_hits.len(), 142);
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.remaining_shield == 0.0 && hit.remaining_hull > 0.0)
            .count(),
        49
    );
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.remaining_hull == 0.0)
            .count(),
        1
    );

    // All incoming records in this capture preserve assigned physical damage,
    // including pre-cap shield and terminal hull damage. This is independent
    // observation coverage, not a claim about every future hostile or channel.
    let reference = observed
        .mitigation
        .nearest_integer_reconstruction_ratio_range;
    assert!(observed.mitigation.nearest_integer_is_hypothesis);
    let budget = observed.mitigation.conservative_component_error_budget;
    close(budget, 2.01, 1e-12);
    let mut lower: f64 = 0.0;
    let mut upper = f64::INFINITY;
    for hit in &observed.incoming_hits {
        assert_eq!(hit.shot_hit, 1);
        close(hit.isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_critical_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_critical_damage, 0.0, 1e-12);
        let raw =
            hit.hull_damage + hit.shield_damage + hit.mitigated_damage + hit.mitigated_apex_barrier;
        assert!(raw > budget);
        let interval = [
            hit.mitigated_damage / (raw + budget),
            hit.mitigated_damage / (raw - budget),
        ];
        assert!(interval[0] <= reference[0] && reference[1] <= interval[1]);
        lower = lower.max(interval[0]);
        upper = upper.min(interval[1]);
    }
    assert!(lower <= upper);
    close(lower, 0.7119846852075901, 1e-12);
    close(upper, 0.7120182495763421, 1e-12);
    assert!(lower <= observed.mitigation.joint_diagnostic_interval[1]);
    assert!(upper >= observed.mitigation.joint_diagnostic_interval[0]);
}

#[test]
fn repairs_and_charge_events_stay_separate_from_damage_and_depletion() {
    let observed = fixture().observed;
    assert_eq!(observed.hull_repairs.len(), 46);
    assert_eq!(observed.charge_events.len(), 23);
    let expected_charge_rounds: Vec<u32> = (2..=46).step_by(2).collect();
    let charge_rounds: Vec<_> = observed.charge_events.iter().map(|c| c.round).collect();
    assert_eq!(charge_rounds, expected_charge_rounds);
    assert!(observed
        .charge_events
        .iter()
        .all(|charge| charge.charge_percentage == 1.0));

    for repair in &observed.hull_repairs {
        let first_incoming_index = observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.round == repair.round)
            .map(|hit| hit.source_index)
            .min()
            .expect("each repair round has incoming damage");
        assert!(repair.source_index < first_incoming_index);
        let prior_round_hull_damage: f64 = observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.round + 1 == repair.round)
            .map(|hit| hit.hull_damage)
            .sum();
        close(repair.amount, 0.08 * prior_round_hull_damage, 1e-9);
    }

    let depletion = observed
        .incoming_hits
        .iter()
        .find(|hit| hit.remaining_shield == 0.0)
        .expect("the shield eventually depletes");
    assert_eq!(
        depletion.round,
        observed.shield_share.first_zero_remaining_shield.round
    );
    assert_eq!(
        depletion.source_index,
        observed
            .shield_share
            .first_zero_remaining_shield
            .source_index
    );
    assert_eq!(depletion.round, 31);
    assert_eq!(depletion.source_index, 3556);
    let death = observed
        .incoming_hits
        .iter()
        .find(|hit| hit.remaining_hull == 0.0)
        .expect("terminal hit should remain represented");
    assert_eq!(
        death.round,
        observed.shield_share.terminal_zero_remaining_hull.round
    );
    assert_eq!(
        death.source_index,
        observed
            .shield_share
            .terminal_zero_remaining_hull
            .source_index
    );
    assert_eq!(death.round, 47);
    assert_eq!(death.source_index, 5431);
    assert!(death.hull_damage > 0.0);
    assert!(observed.charge_events.iter().all(|charge| observed
        .incoming_hits
        .iter()
        .all(|hit| hit.source_index != charge.source_index)));
}
