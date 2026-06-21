//! Log vs sim calibration: Gorn Eviscerator T10 L50 vs hostile 447012258 (Acrocanth L60).
//!
//! Fixture: [`fixtures/gorn_evisc_vs_447012258_log_outgoing.json`](fixtures/gorn_evisc_vs_447012258_log_outgoing.json).
//! Source export: `fight samples/gorn-evisc_vs_447012258_60_easywin_trivial.csv`.
//!
//! Gauges:
//! - **Sim bands** (deterministic seed): rounds, kill damage, isolytic applied, defender HP at end.
//! - **Log ratio bands**: sim metrics vs parsed CSV / summary anchors (different column semantics).
//! - **Trace invariants**: Hunt the Hunters isolytic bonus, Isolytic Vulnerability on damage rows.
//! - **Monte Carlo**: trivial win distribution for the log crew.

use kobayashi::calibration::{simulation_band_report, MetricBands};
use kobayashi::combat::parse_fight_export;
use kobayashi::combat::types::SimulationResult;
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, run_monte_carlo_with_registry, DefenderOpponent,
    MonteCarloSeedReplay,
};
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

const FIXTURE: &str = include_str!("fixtures/gorn_evisc_vs_447012258_log_outgoing.json");
const FIGHT_SAMPLE: &str = "gorn-evisc_vs_447012258_60_easywin_trivial.csv";

#[derive(Debug, Deserialize)]
struct GornLogCrew {
    captain: String,
    bridge: Vec<String>,
    below_decks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RatioBand {
    min: f64,
    max: f64,
}

#[derive(Debug, Deserialize)]
struct MonteCarloExpectations {
    iterations: u32,
    seed: u64,
    min_win_rate: f64,
    min_r1_kill_rate: f64,
    min_avg_hull_remaining: f64,
}

#[derive(Debug, Deserialize)]
struct TraceExpectations {
    min_hunt_the_hunters_isolytic_bonus_composed: f64,
    require_isolytic_vulnerability_on_damage_application: bool,
}

#[derive(Debug, Deserialize)]
struct GornLogFixture {
    hostile_id: String,
    ship_kobayashi_id: String,
    ship_tier: u32,
    ship_level: u32,
    profile_id: String,
    scenario_seed: u64,
    sim_index: u64,
    crew: GornLogCrew,
    attacker_won: bool,
    round_count: u32,
    player_outgoing_attack_events: u32,
    total_outgoing_damage: f64,
    total_outgoing_isolytic_damage: f64,
    decisive_attack_critical: bool,
    defender_kill_damage_from_summary: f64,
    attacker_hull_remaining: f64,
    attacker_hull_max: f64,
    #[allow(dead_code)]
    attacker_hull_remaining_fraction: f64,
    enemy_incoming_attack_total_damage: f64,
    sim_bands: MetricBands,
    log_sim_ratio_bands: LogSimRatioBands,
    monte_carlo: MonteCarloExpectations,
    trace_expectations: TraceExpectations,
    #[allow(dead_code)]
    calibration_notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LogSimRatioBands {
    total_damage_vs_log_kill_summary: RatioBand,
    total_isolytic_vs_log_attack_isolytic_column: RatioBand,
    total_damage_vs_log_attack_total_damage_column: RatioBand,
}

fn fight_sample_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fight samples")
        .join(FIGHT_SAMPLE)
}

fn crew_from_fixture(f: &GornLogFixture) -> CrewCandidate {
    CrewCandidate {
        captain: f.crew.captain.clone(),
        bridge: f.crew.bridge.clone(),
        below_decks: f.crew.below_decks.clone(),
    }
}

struct CsvAttackTotals {
    player_outgoing_total_damage: f64,
    player_outgoing_isolytic_damage: f64,
    enemy_incoming_total_damage: f64,
    player_outgoing_attack_events: u32,
}

/// Parse Attack-row column sums from the game CSV export.
fn csv_attack_totals(csv: &str) -> CsvAttackTotals {
    let player_name = csv
        .lines()
        .nth(1)
        .and_then(|line| line.split('\t').next())
        .unwrap_or("HiggsBozo");
    let enemy_name = csv
        .lines()
        .nth(2)
        .and_then(|line| line.split('\t').next())
        .unwrap_or("Acrocanth");

    let mut in_events = false;
    let mut header: Vec<&str> = Vec::new();
    let mut player_outgoing_total_damage = 0.0;
    let mut player_outgoing_isolytic_damage = 0.0;
    let mut enemy_incoming_total_damage = 0.0;
    let mut player_outgoing_attack_events = 0u32;

    for line in csv.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with("Round\t") {
            in_events = true;
            header = line.split('\t').collect();
            continue;
        }
        if !in_events {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.first().and_then(|s| s.parse::<u32>().ok()).is_none() {
            break;
        }
        let row: std::collections::HashMap<&str, &str> = header
            .iter()
            .zip(cols.iter())
            .map(|(k, v)| (*k, *v))
            .collect();
        if row.get("Type").copied() != Some("Attack") {
            continue;
        }
        let attacker = row.get("Attacker Name").copied().unwrap_or("");
        let total_damage = row
            .get("Total Damage")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let total_isolytic = row
            .get("Total Isolytic Damage")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        if attacker == player_name {
            player_outgoing_attack_events += 1;
            player_outgoing_total_damage += total_damage;
            player_outgoing_isolytic_damage += total_isolytic;
        } else if attacker == enemy_name {
            enemy_incoming_total_damage += total_damage;
        }
    }

    CsvAttackTotals {
        player_outgoing_total_damage,
        player_outgoing_isolytic_damage,
        enemy_incoming_total_damage,
        player_outgoing_attack_events,
    }
}

fn replay_to_simulation_result(replay: &MonteCarloSeedReplay) -> SimulationResult {
    SimulationResult {
        total_damage: replay.total_damage,
        total_isolytic_damage: replay.total_isolytic_damage,
        attacker_won: replay.attacker_won,
        winner_by_round_limit: replay.winner_by_round_limit,
        rounds_simulated: replay.rounds_simulated,
        attacker_hull_remaining: replay.attacker_hull_remaining,
        defender_hull_remaining: replay.defender_hull_remaining,
        defender_shield_remaining: replay.defender_shield_remaining,
        attacker_shield_remaining: 0.0,
        events: Vec::new(),
        conqueror_borg_beam_suppression: false,
    }
}

fn assert_ratio_band(name: &str, sim: f64, log: f64, band: &RatioBand) {
    let ratio = sim / log.max(1.0);
    assert!(
        ratio >= band.min && ratio <= band.max,
        "{name}: ratio {ratio:.4} (sim={sim:.0} log={log:.0}) outside [{}, {}]",
        band.min,
        band.max
    );
}

fn max_composed_stack(replay: &MonteCarloSeedReplay, stack_key: &str) -> f64 {
    let mut max = 0.0_f64;
    for ev in &replay.trace_events {
        if ev.event_type != "stack_resolution" {
            continue;
        }
        let Some(stacks) = ev.values.get("stacks").and_then(|v| v.as_object()) else {
            continue;
        };
        let Some(entry) = stacks.get(stack_key).and_then(|v| v.as_object()) else {
            continue;
        };
        if let Some(c) = entry.get("composed").and_then(|v| v.as_f64()) {
            max = max.max(c);
        }
    }
    max
}

fn assert_trace_expectations(replay: &MonteCarloSeedReplay, trace: &TraceExpectations) {
    let max_isolytic_bonus = max_composed_stack(replay, "isolytic_damage_bonus");
    let hunt_fired = replay.trace_events.iter().any(|ev| {
        ev.event_type == "ability_activation"
            && ev.source.ship_ability_id.as_deref() == Some("1273528452")
    });
    let mut saw_vulnerability = false;

    for ev in &replay.trace_events {
        if ev.event_type == "damage_application"
            && ev
                .values
                .get("isolytic_vulnerability")
                .and_then(|v| v.as_bool())
                == Some(true)
        {
            saw_vulnerability = true;
        }
    }

    assert!(
        hunt_fired,
        "expected Hunt the Hunters (1273528452) ability_activation at combat begin"
    );
    assert!(
        max_isolytic_bonus >= trace.min_hunt_the_hunters_isolytic_bonus_composed,
        "Hunt the Hunters isolytic bonus composed={max_isolytic_bonus} expected >= {} (L50 curve ~60)",
        trace.min_hunt_the_hunters_isolytic_bonus_composed
    );

    if trace.require_isolytic_vulnerability_on_damage_application {
        assert!(
            saw_vulnerability,
            "expected at least one damage_application with isolytic_vulnerability=true (Acrocanth 658066283)"
        );
    }
}

fn print_calibration_report(
    f: &GornLogFixture,
    replay: &MonteCarloSeedReplay,
    csv: &CsvAttackTotals,
    band_report: &kobayashi::calibration::DriftRunReport,
) {
    eprintln!(
        "=== gorn447012258 log vs sim calibration (seed={}) ===",
        f.scenario_seed
    );
    eprintln!(
        "sim: rounds={} total_damage={:.0} total_isolytic={:.0} won={} hull_rem={:.0}",
        replay.rounds_simulated,
        replay.total_damage,
        replay.total_isolytic_damage,
        replay.attacker_won,
        replay.attacker_hull_remaining,
    );
    eprintln!(
        "log: rounds={} kill_summary={:.0} attack_total_col={:.0} attack_isolytic_col={:.0} incoming={:.0} attacker_hull_rem={:.0}/{:.0}",
        f.round_count,
        f.defender_kill_damage_from_summary,
        csv.player_outgoing_total_damage,
        csv.player_outgoing_isolytic_damage,
        csv.enemy_incoming_total_damage,
        f.attacker_hull_remaining,
        f.attacker_hull_max,
    );
    for row in &band_report.rows {
        eprintln!(
            "  band {}: actual={:.4} range=[{:.4}, {:.4}] ok={}",
            row.metric, row.actual, row.low, row.high, row.in_band
        );
    }
    eprintln!(
        "  ratios: sim/kill={:.3} sim_iso/log_iso={:.4} sim/log_attack_col={:.3}",
        replay.total_damage / f.defender_kill_damage_from_summary,
        replay.total_isolytic_damage / f.total_outgoing_isolytic_damage,
        replay.total_damage / f.total_outgoing_damage,
    );
}

#[test]
fn gorn_evisc_fight_export_parses_and_matches_fixture() {
    let contents =
        std::fs::read_to_string(fight_sample_path()).expect("fight sample CSV should exist");
    let export = parse_fight_export(&contents).expect("parse fight export");
    let f: GornLogFixture = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let csv = csv_attack_totals(&contents);

    assert!(export.attacker_won);
    assert_eq!(export.rounds, f.round_count);
    assert_eq!(export.defender_hull_remaining, 0.0);
    assert_eq!(export.defender_shield_remaining, 0.0);
    assert_eq!(export.player_ship_name.as_deref(), Some("GORN EVISCERATOR"));
    assert_eq!(
        export.player_officer_one.as_deref(),
        Some("Christopher Pike")
    );
    assert_eq!(export.player_officer_two.as_deref(), Some("Marlena Moreau"));
    assert_eq!(export.player_officer_three.as_deref(), Some("T'Laan"));
    assert_eq!(export.enemy_player_name.as_deref(), Some("Acrocanth"));
    assert_eq!(export.enemy_ship_level, Some(60));

    assert!(
        (csv.player_outgoing_total_damage - f.total_outgoing_damage).abs() < 1.0,
        "outgoing attack Total Damage column: csv={} fixture={}",
        csv.player_outgoing_total_damage,
        f.total_outgoing_damage
    );
    assert!(
        (csv.player_outgoing_isolytic_damage - f.total_outgoing_isolytic_damage).abs() < 1.0,
        "outgoing attack Total Isolytic Damage column: csv={} fixture={}",
        csv.player_outgoing_isolytic_damage,
        f.total_outgoing_isolytic_damage
    );
    assert!(
        (csv.enemy_incoming_total_damage - f.enemy_incoming_attack_total_damage).abs() < 1.0,
        "incoming enemy attack Total Damage: csv={} fixture={}",
        csv.enemy_incoming_total_damage,
        f.enemy_incoming_attack_total_damage
    );
    assert!(
        (export.total_damage - f.defender_kill_damage_from_summary).abs() < 16.0,
        "summary kill damage: log={} fixture={}",
        export.total_damage,
        f.defender_kill_damage_from_summary
    );
    assert_eq!(
        csv.player_outgoing_attack_events,
        f.player_outgoing_attack_events
    );

    let crit = export
        .events
        .iter()
        .filter(|e| e.event_type == "Attack")
        .filter(|e| e.total_damage > 50_000_000.0)
        .any(|e| e.critical_hit);
    assert_eq!(crit, f.decisive_attack_critical);
}

#[test]
fn gorn_evisc_log_vs_sim_calibration() {
    std::env::set_var("KOBAYASHI_OFFICER_SOURCE", "lcars");
    let contents =
        std::fs::read_to_string(fight_sample_path()).expect("fight sample CSV should exist");
    let csv = csv_attack_totals(&contents);
    let f: GornLogFixture = serde_json::from_str(FIXTURE).expect("fixture JSON");

    assert_eq!(f.hostile_id, "447012258");
    assert_eq!(f.profile_id, DEMO_PROFILE_ID);

    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    let candidate = crew_from_fixture(&f);

    let replay = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        &f.ship_kobayashi_id,
        &f.hostile_id,
        Some(f.ship_tier),
        Some(f.ship_level),
        &candidate,
        f.scenario_seed,
        f.sim_index,
        Some(DEMO_PROFILE_ID),
        2_000_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !replay.using_placeholder_combatants,
        "ship/hostile must resolve (got placeholders)"
    );

    let sim = replay_to_simulation_result(&replay);
    let band_report = simulation_band_report(
        "gorn_evisc_vs_447012258",
        Some("Gorn Eviscerator T10 L50 vs Acrocanth 447012258"),
        Some("fight samples/gorn-evisc_vs_447012258_60_easywin_trivial.csv"),
        &f.sim_bands,
        Some(f.attacker_won),
        &sim,
    );
    print_calibration_report(&f, &replay, &csv, &band_report);

    assert!(
        band_report.all_ok,
        "sim band report failed: {:?}",
        band_report
            .rows
            .iter()
            .filter(|r| !r.in_band)
            .collect::<Vec<_>>()
    );

    assert!(
        (sim.total_isolytic_damage - sim.total_damage).abs() / sim.total_damage.max(1.0) < 0.01,
        "vulnerability: total_isolytic_damage should match total_damage (iso={} total={})",
        sim.total_isolytic_damage,
        sim.total_damage
    );

    let ratios = &f.log_sim_ratio_bands;
    assert_ratio_band(
        "total_damage_vs_log_kill_summary",
        sim.total_damage,
        f.defender_kill_damage_from_summary,
        &ratios.total_damage_vs_log_kill_summary,
    );
    assert_ratio_band(
        "total_isolytic_vs_log_attack_isolytic_column",
        sim.total_isolytic_damage,
        f.total_outgoing_isolytic_damage,
        &ratios.total_isolytic_vs_log_attack_isolytic_column,
    );
    assert_ratio_band(
        "total_damage_vs_log_attack_total_damage_column",
        sim.total_damage,
        f.total_outgoing_damage,
        &ratios.total_damage_vs_log_attack_total_damage_column,
    );

    assert_trace_expectations(&replay, &f.trace_expectations);

    let mc = &f.monte_carlo;
    let (results, placeholder) = run_monte_carlo_with_registry(
        registry.as_ref(),
        &f.ship_kobayashi_id,
        &f.hostile_id,
        Some(f.ship_tier),
        Some(f.ship_level),
        std::slice::from_ref(&candidate),
        mc.iterations as usize,
        mc.seed,
        Some(DEMO_PROFILE_ID),
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(!placeholder);
    let r = &results[0];
    eprintln!(
        "mc(n={}): win_rate={:.4} r1_kill={:.4} avg_hull={:.4} (log: round-1 crit win)",
        mc.iterations, r.win_rate, r.r1_kill_rate, r.avg_hull_remaining
    );
    assert!(
        r.win_rate >= mc.min_win_rate,
        "log crew should trivially win (got {})",
        r.win_rate
    );
    assert!(
        r.r1_kill_rate >= mc.min_r1_kill_rate,
        "expected meaningful r1_kill after Hunt the Hunters L50 scaling (got {})",
        r.r1_kill_rate
    );
    assert!(
        r.avg_hull_remaining >= mc.min_avg_hull_remaining,
        "hull remaining on wins (got {})",
        r.avg_hull_remaining
    );
}
