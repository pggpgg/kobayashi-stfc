//! Tests for raw combat log ingestion (parity with docs/combat_log_format.md).

use std::path::Path;

use kobayashi::combat::{
    parse_combat_log_json, parity_within_tolerance, ingested_to_comparable,
    ingested_events_to_combat_events, IngestedCombatLog,
};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
        .join(name)
}

#[test]
fn parse_sample_combat_log_fixture() {
    let path = fixture_path("sample_combat_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log: IngestedCombatLog = parse_combat_log_json(&json).expect("parse");
    assert_eq!(log.rounds_simulated, 2);
    assert_eq!(log.events.len(), 6);
    assert!((log.total_damage - 380.5).abs() < 1e-9);
    assert!(log.attacker_won);
    assert!((log.defender_hull_remaining - 0.0).abs() < 1e-9);
    assert!((log.defender_shield_remaining - 0.0).abs() < 1e-9);
    // Round-level events omit `weapon_index`.
    assert_eq!(log.events[0].weapon_index, None);
    // Damage events can include sub-round weapon index.
    assert_eq!(log.events[1].weapon_index, Some(0));
    assert_eq!(log.events[4].weapon_index, Some(0));
}

#[test]
fn parse_combat_log_assert_event_count_and_round_count() {
    let path = fixture_path("sample_combat_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    assert!(log.events.len() >= 1, "at least one event");
    assert!(log.rounds_simulated >= 1, "at least one round");
    let round_indices: Vec<u32> = log.events.iter().map(|e| e.round_index).collect();
    let max_round = round_indices.iter().copied().max().unwrap_or(0);
    assert_eq!(max_round, log.rounds_simulated, "max event round matches rounds_simulated");
}

#[test]
fn ingested_to_comparable_returns_key_fields() {
    let path = fixture_path("sample_combat_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    let (total_damage, attacker_won, rounds, def_hull, def_shield) = ingested_to_comparable(&log);
    assert!((total_damage - 380.5).abs() < 1e-9);
    assert!(attacker_won);
    assert_eq!(rounds, 2);
    assert!((def_hull - 0.0).abs() < 1e-9);
    assert!((def_shield - 0.0).abs() < 1e-9);
}

#[test]
fn ingested_events_convert_to_combat_events() {
    let path = fixture_path("sample_combat_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    let combat_events = ingested_events_to_combat_events(&log.events);
    assert_eq!(combat_events.len(), log.events.len());
    assert_eq!(combat_events[0].event_type, "round_start");
    assert_eq!(combat_events[1].event_type, "damage_application");
    // `weapon_index` is optional in the raw log format and should default to `None`
    // for round-level events.
    assert_eq!(combat_events[0].weapon_index, None);
    // For multi-weapon parity, damage events include sub-round weapon index.
    assert_eq!(combat_events[1].weapon_index, Some(0));
    assert_eq!(combat_events[4].weapon_index, Some(0));
}

#[test]
fn parity_within_tolerance_matches_when_close() {
    let path = fixture_path("sample_combat_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    use kobayashi::combat::SimulationResult;
    let sim = SimulationResult {
        total_damage: 380.5,
        attacker_won: true,
        winner_by_round_limit: false,
        rounds_simulated: 2,
        attacker_hull_remaining: 1000.0,
        defender_hull_remaining: 0.0,
        defender_shield_remaining: 0.0,
        events: vec![],
    };
    assert!(parity_within_tolerance(&sim, &log, 1.0, 1.0));
}

#[test]
fn parse_minimal_log() {
    let json = r#"{"rounds_simulated":1,"total_damage":100.0,"attacker_won":true,"defender_hull_remaining":0.0,"events":[]}"#;
    let log = parse_combat_log_json(json).expect("parse");
    assert_eq!(log.rounds_simulated, 1);
    assert_eq!(log.total_damage, 100.0);
    assert!(log.attacker_won);
    assert_eq!(log.events.len(), 0);
}

#[test]
fn parse_multi_weapon_round_fixture() {
    let path = fixture_path("multi_weapon_round_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log: IngestedCombatLog = parse_combat_log_json(&json).expect("parse");
    assert_eq!(log.rounds_simulated, 1);
    assert!((log.total_damage - 15.0).abs() < 1e-9);
    assert_eq!(log.events.len(), 4);
    let dmg: Vec<_> = log
        .events
        .iter()
        .filter(|e| e.event_type == "damage_application")
        .collect();
    assert_eq!(dmg.len(), 2);
    assert_eq!(dmg[0].weapon_index, Some(0));
    assert_eq!(dmg[1].weapon_index, Some(1));
    let combat = ingested_events_to_combat_events(&log.events);
    assert_eq!(combat.len(), 4);
    let w_idx: Vec<_> = combat
        .iter()
        .filter(|e| e.event_type == "damage_application")
        .map(|e| e.weapon_index)
        .collect();
    assert_eq!(w_idx, vec![Some(0), Some(1)]);
}
