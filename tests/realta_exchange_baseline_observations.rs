//! Source-observation checks for the redacted no-Tom Exchange control.
//! These constrain accounting without asserting an unproven combat mechanic.

use serde::Deserialize;

const FIXTURE: &str =
    include_str!("fixtures/recorded_fights/realta_exchange_baseline_20261023_observed.json");

#[derive(Deserialize)]
struct Fixture {
    source_json_sha256: String,
    source_projection_sha256: String,
    battle_time: String,
    observed_client_build: u32,
    parser_reference_client_build: u32,
    setup_check_summary: SetupCheckSummary,
    observed: Observed,
}

#[derive(Deserialize)]
struct SetupCheckSummary {
    tom_paris_absent: bool,
    checked_flags: std::collections::BTreeMap<String, bool>,
}

#[derive(Deserialize)]
struct Observed {
    rounds: u32,
    subrounds: usize,
    incoming_hits: Vec<Hit>,
    hull_repairs: Vec<Repair>,
    charge_events: Vec<Charge>,
    mitigation: Mitigation,
    hp_ledger_exceptions: Vec<LedgerException>,
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
    nearest_integer_raw_ratio: f64,
    source_derived_osr_brackets: Vec<OsrBracket>,
}

#[derive(Deserialize)]
struct OsrBracket {
    combat_start_pool: f64,
    defenses_min: [u32; 3],
    defenses_max: [u32; 3],
    mitigation_band: [f64; 2],
}

#[derive(Deserialize)]
struct LedgerException {
    source_index: usize,
    round: u32,
    reason: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("redacted Exchange fixture should parse")
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected} ± {tolerance}, observed {actual}"
    );
}

#[test]
fn clean_incoming_damage_brackets_source_derived_osr_pools() {
    let fixture = fixture();
    let observed = &fixture.observed;
    assert_eq!(fixture.source_json_sha256.len(), 64);
    assert_eq!(fixture.source_projection_sha256.len(), 64);
    assert_eq!(fixture.battle_time, "2026-10-23T18:59:03");
    assert_eq!(fixture.observed_client_build, 273);
    assert_eq!(fixture.parser_reference_client_build, 272);
    assert!(fixture.setup_check_summary.tom_paris_absent);
    assert_eq!(fixture.setup_check_summary.checked_flags.len(), 25);
    assert!(fixture
        .setup_check_summary
        .checked_flags
        .values()
        .all(|v| *v));
    assert_eq!(observed.rounds, 17);
    assert_eq!(observed.subrounds, 34);
    assert_eq!(observed.incoming_hits.len(), 50);

    let clean: Vec<_> = observed
        .incoming_hits
        .iter()
        .filter(|hit| hit.shot_hit == 1 && hit.is_critical == 0 && hit.remaining_shield > 0.0)
        .collect();
    assert_eq!(clean.len(), 26);
    assert_eq!(clean.len(), observed.mitigation.clean_hit_count);
    assert_eq!(clean.iter().filter(|hit| hit.round <= 2).count(), 6);
    assert_eq!(clean.iter().filter(|hit| hit.round >= 3).count(), 20);

    let error = observed.mitigation.conservative_component_error_budget;
    close(error, 2.01, 1e-12);
    let mut lower: f64 = 0.0;
    let mut upper = f64::INFINITY;
    for hit in clean {
        // The journal reports split damage and mitigation, not the raw hit.
        let reconstructed_raw =
            hit.hull_damage + hit.shield_damage + hit.mitigated_damage + hit.mitigated_apex_barrier
                - hit.isolytic_damage;
        assert!(reconstructed_raw > error);
        close(
            hit.mitigated_damage / reconstructed_raw.round(),
            observed.mitigation.nearest_integer_raw_ratio,
            1e-12,
        );
        lower = lower.max(hit.mitigated_damage / (reconstructed_raw + error));
        upper = upper.min(hit.mitigated_damage / (reconstructed_raw - error));
        close(hit.isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_damage, 0.0, 1e-12);
        close(hit.mitigated_critical_damage, 0.0, 1e-12);
        close(hit.mitigated_isolytic_critical_damage, 0.0, 1e-12);

        let shield_share = hit.shield_damage / (hit.hull_damage + hit.shield_damage);
        let expected_share = if hit.round <= 2 { 0.825 } else { 0.8 };
        close(shield_share, expected_share, 3e-5);
    }
    assert!(lower <= upper, "clean hits disagree on constant mitigation");
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

    let brackets = &observed.mitigation.source_derived_osr_brackets;
    assert_eq!(brackets.len(), 2);
    let low = &brackets[0];
    let high = &brackets[1];
    close(low.combat_start_pool, 12.25, 1e-12);
    close(high.combat_start_pool, 19.25, 1e-12);
    assert_eq!(low.defenses_min, [123, 925, 41]);
    assert_eq!(low.defenses_max, [123, 947, 41]);
    assert_eq!(high.defenses_min, [144, 1079, 48]);
    assert_eq!(high.defenses_max, [144, 1101, 48]);
    assert!(low.mitigation_band[1] < lower);
    assert!(high.mitigation_band[0] <= upper && high.mitigation_band[1] >= lower);
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
    assert_eq!(criticals.len(), 5);

    // Independently retained from the noncritical observations. Their nearest
    // integer raw denominator is diagnostic, not proof of a server numeric type.
    let reference = observed.mitigation.nearest_integer_raw_ratio;
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
        // Critical raw remains fractional; no nearest-integer denominator here.
        let interval = [
            hit.mitigated_damage / (reconstructed + budget),
            hit.mitigated_damage / (reconstructed - budget),
        ];
        assert!(interval[0] <= reference && reference <= interval[1]);
        lower = lower.max(interval[0]);
        upper = upper.min(interval[1]);
    }
    assert!(fractional_raw_seen);
    assert!(lower <= upper);
    close(lower, 0.16193083481922396, 1e-12);
    close(upper, 0.16194570120464014, 1e-12);
    assert!(lower <= observed.mitigation.joint_diagnostic_interval[1]);
    assert!(upper >= observed.mitigation.joint_diagnostic_interval[0]);
}

#[test]
fn reported_physical_damage_reconstructs_all_hits_including_depletion_and_overkill() {
    let observed = fixture().observed;
    assert_eq!(observed.incoming_hits.len(), 50);
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.remaining_shield == 0.0 && hit.remaining_hull > 0.0)
            .count(),
        17
    );
    assert_eq!(
        observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.remaining_hull == 0.0)
            .count(),
        2
    );

    // Separate evidence from the clean-hit selection: these recorded damage
    // fields retain assigned damage before remaining-HP clamping. Do not replace
    // them with before-minus-after HP or generalize this to untested channels.
    let reference = observed.mitigation.nearest_integer_raw_ratio;
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
        assert!(interval[0] <= reference && reference <= interval[1]);
        lower = lower.max(interval[0]);
        upper = upper.min(interval[1]);
    }
    assert!(lower <= upper);
    close(lower, 0.16193376246972066, 1e-12);
    close(upper, 0.16194175073519296, 1e-12);
    assert!(lower <= observed.mitigation.joint_diagnostic_interval[1]);
    assert!(upper >= observed.mitigation.joint_diagnostic_interval[0]);
}

#[test]
fn repairs_charges_and_terminal_hits_remain_distinct_observations() {
    let fixture = fixture();
    let observed = fixture.observed;
    assert_eq!(observed.hull_repairs.len(), 16);
    assert_eq!(observed.charge_events.len(), 8);
    let charge_rounds: Vec<_> = observed.charge_events.iter().map(|c| c.round).collect();
    assert_eq!(charge_rounds, [2, 4, 6, 8, 10, 12, 14, 16]);
    assert!(observed
        .charge_events
        .iter()
        .all(|c| c.charge_percentage == 1.0));

    for repair in &observed.hull_repairs {
        let prior_round_hull_damage: f64 = observed
            .incoming_hits
            .iter()
            .filter(|hit| hit.round + 1 == repair.round)
            .map(|hit| hit.hull_damage)
            .sum();
        close(repair.amount, 0.08 * prior_round_hull_damage, 1e-9);
    }

    let exception_indices: Vec<_> = observed
        .hp_ledger_exceptions
        .iter()
        .map(|e| e.source_index)
        .collect();
    assert_eq!(exception_indices, [1186, 1863, 1882]);
    assert_eq!(observed.hp_ledger_exceptions[0].round, 11);
    assert!(observed.hp_ledger_exceptions[0]
        .reason
        .contains("shield depletion"));
    assert_eq!(observed.hp_ledger_exceptions[1].round, 17);
    assert_eq!(observed.hp_ledger_exceptions[2].round, 17);

    let depletion = observed
        .incoming_hits
        .iter()
        .find(|hit| hit.source_index == 1186)
        .unwrap();
    assert_eq!(depletion.remaining_shield, 0.0);
    assert!(depletion.shield_damage > 0.0);
    let terminal: Vec<_> = observed
        .incoming_hits
        .iter()
        .filter(|hit| hit.round == 17)
        .collect();
    assert_eq!(terminal.len(), 2);
    assert_eq!(terminal[0].source_index, 1863);
    assert_eq!(terminal[1].source_index, 1882);
    assert_eq!(terminal[0].remaining_hull, 0.0);
    assert_eq!(terminal[1].remaining_hull, 0.0);
    assert!(terminal.iter().all(|hit| hit.hull_damage > 0.0));

    // Follow the reported player HP through source order, including repairs.
    // Exactly the three documented pre-cap/death records break direct subtraction.
    let mut events: Vec<_> = observed
        .incoming_hits
        .iter()
        .map(|hit| (hit.source_index, Some(hit), None))
        .chain(
            observed
                .hull_repairs
                .iter()
                .map(|repair| (repair.source_index, None, Some(repair))),
        )
        .collect();
    events.sort_by_key(|event| event.0);
    let mut prior_hull = None;
    let mut prior_shield = None;
    let mut exact_hits = 0;
    let mut observed_exceptions = Vec::new();
    for (_, hit, repair) in events {
        if let Some(repair) = repair {
            prior_hull = Some(prior_hull.expect("repair follows the first hit") + repair.amount);
            continue;
        }
        let hit = hit.expect("source event is hit or repair");
        let hull_before = prior_hull.unwrap_or(hit.remaining_hull + hit.hull_damage);
        let shield_before = prior_shield.unwrap_or(hit.remaining_shield + hit.shield_damage);
        let hull_residual = hull_before - hit.hull_damage - hit.remaining_hull;
        let shield_residual = shield_before - hit.shield_damage - hit.remaining_shield;
        match hit.source_index {
            1186 => {
                close(hull_residual, 0.0, 1e-6);
                assert!(shield_residual < 0.0);
                observed_exceptions.push(hit.source_index);
            }
            1863 | 1882 => {
                assert!(hull_residual < 0.0);
                close(shield_residual, 0.0, 1e-6);
                observed_exceptions.push(hit.source_index);
            }
            _ => {
                close(hull_residual, 0.0, 1e-6);
                close(shield_residual, 0.0, 1e-6);
                exact_hits += 1;
            }
        }
        prior_hull = Some(hit.remaining_hull);
        prior_shield = Some(hit.remaining_shield);
    }
    assert_eq!(exact_hits, 47);
    assert_eq!(observed_exceptions, exception_indices);

    // Charge markers are separate short events. Do not silently turn them
    // into zero-damage attacks or coerce depletion/overkill into exact HP deltas.
    assert!(observed.charge_events.iter().all(|charge| observed
        .incoming_hits
        .iter()
        .all(|hit| hit.source_index != charge.source_index)));
}
