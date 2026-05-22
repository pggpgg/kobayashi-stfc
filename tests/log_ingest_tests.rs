//! Tests for raw combat log ingestion (parity with docs/combat_log_format.md).

use std::path::Path;

use kobayashi::combat::{
    compare_ingested_trace_to_simulator, expand_collapsed_repeat_events,
    hydrate_ingested_state_snapshots_from_values, ingested_events_to_combat_events,
    ingested_to_comparable, parity_within_tolerance, parse_combat_log_json, simulate_combat,
    tag_stats_snapshot_sources_client_default, validate_canonical_timeline, Combatant,
    CrewConfiguration, OpponentFactionTag, SimulationConfig, TraceCompareOptions, TraceMode,
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
    let log = parse_combat_log_json(&json).expect("parse");
    assert_eq!(log.schema_version, 1);
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
    assert!(!log.events.is_empty(), "at least one event");
    assert!(log.rounds_simulated >= 1, "at least one round");
    let round_indices: Vec<u32> = log.events.iter().map(|e| e.round_index).collect();
    let max_round = round_indices.iter().copied().max().unwrap_or(0);
    assert_eq!(
        max_round, log.rounds_simulated,
        "max event round matches rounds_simulated"
    );
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
        attacker_shield_remaining: 0.0,
        events: vec![],
        conqueror_borg_beam_suppression: false,
    };
    assert!(parity_within_tolerance(&sim, &log, 1.0, 1.0));
}

#[test]
fn parse_minimal_log() {
    let json = r#"{"rounds_simulated":1,"total_damage":100.0,"attacker_won":true,"defender_hull_remaining":0.0,"events":[]}"#;
    let log = parse_combat_log_json(json).expect("parse");
    assert_eq!(log.schema_version, 1);
    assert_eq!(log.rounds_simulated, 1);
    assert_eq!(log.total_damage, 100.0);
    assert!(log.attacker_won);
    assert_eq!(log.events.len(), 0);
}

#[test]
fn parse_multi_weapon_round_fixture() {
    let path = fixture_path("multi_weapon_round_log.json");
    let json = std::fs::read_to_string(&path).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
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

#[test]
fn optional_metadata_round_trips_json() {
    let json = r#"{"schema_version":2,"rounds_simulated":1,"total_damage":1.0,"attacker_won":true,"defender_hull_remaining":0.0,"events":[{"event_type":"round_start","round_index":1,"phase":"round","sequence":1,"client_kind":"START_ROUND","client_payload":{"id":9},"stats_snapshot":{"attacker_hull":1000.0},"values":{}}]}"#;
    let log = parse_combat_log_json(json).unwrap();
    assert_eq!(log.schema_version, 2);
    let ev = &log.events[0];
    assert_eq!(ev.sequence, Some(1));
    assert_eq!(ev.client_kind.as_deref(), Some("START_ROUND"));
    assert_eq!(
        ev.client_payload.as_ref().and_then(|v| v.get("id")),
        Some(&serde_json::json!(9))
    );
    assert_eq!(
        ev.stats_snapshot
            .as_ref()
            .and_then(|m| m.get("attacker_hull")),
        Some(&serde_json::json!(1000.0))
    );
}

#[test]
fn validate_timeline_v2_rich_fixture_passes() {
    let json = std::fs::read_to_string(fixture_path("rich_engine_aligned_log.json"))
        .expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    let o = validate_canonical_timeline(&log);
    assert!(
        o.errors.is_empty(),
        "errors={:?} warnings={:?}",
        o.errors,
        o.warnings
    );
}

#[test]
fn validate_timeline_invalid_damage_before_round_start_strict_errors() {
    let json =
        std::fs::read_to_string(fixture_path("invalid_timeline_v2.json")).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    let o = validate_canonical_timeline(&log);
    assert!(
        o.errors.iter().any(|e| e.contains("damage_application")),
        "{:?}",
        o.errors
    );
}

#[test]
fn validate_timeline_invalid_sequence_strict_errors() {
    let json =
        std::fs::read_to_string(fixture_path("invalid_sequence_v2.json")).expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");
    let o = validate_canonical_timeline(&log);
    assert!(
        o.errors.iter().any(|e| e.contains("sequence")),
        "{:?}",
        o.errors
    );
}

#[test]
fn validate_timeline_v1_partial_sequence_warns_not_errors() {
    let json = r#"{"schema_version":1,"rounds_simulated":1,"total_damage":0.0,"attacker_won":true,"defender_hull_remaining":100.0,"events":[{"event_type":"round_start","round_index":1,"phase":"round","sequence":1,"values":{}},{"event_type":"round_start","round_index":1,"phase":"round","values":{}}]}"#;
    let log = parse_combat_log_json(json).unwrap();
    let o = validate_canonical_timeline(&log);
    assert!(o.errors.is_empty());
    assert!(
        o.warnings.iter().any(|w| w.contains("sequence")),
        "{:?}",
        o.warnings
    );
}

#[test]
fn rich_engine_aligned_fixture_matches_canonical_sim_trace_subsequence() {
    let attacker = Combatant {
        id: "nero".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.15,
        crit_chance: 0.5,
        crit_multiplier: 1.8,
        proc_chance: 0.4,
        proc_multiplier: 1.25,
        end_of_round_damage: 3.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "swarm".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Events,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        emit_state_snapshots: false,
        crit_damage_reduction_perturb: 0.0,
    };
    let crew = CrewConfiguration::default();
    let sim = simulate_combat(&attacker, &defender, &config, &crew);

    let json = std::fs::read_to_string(fixture_path("rich_engine_aligned_log.json"))
        .expect("read fixture");
    let log = parse_combat_log_json(&json).expect("parse");

    compare_ingested_trace_to_simulator(&sim.events, &log.events, &TraceCompareOptions::default())
        .expect("ingested excerpt should match simulator trace subsequence");
}

#[test]
fn emit_state_snapshots_adds_state_snapshot_events() {
    let attacker = Combatant {
        id: "a1".to_string(),
        attack: 100.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 500.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "d1".to_string(),
        attack: 0.0,
        mitigation: 0.2,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 400.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 1,
        trace_mode: TraceMode::Events,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        emit_state_snapshots: true,
        crit_damage_reduction_perturb: 0.0,
    };
    let crew = CrewConfiguration::default();
    let sim = simulate_combat(&attacker, &defender, &config, &crew);
    let snaps: Vec<_> = sim
        .events
        .iter()
        .filter(|e| e.event_type == "state_snapshot")
        .collect();
    assert!(
        !snaps.is_empty(),
        "emit_state_snapshots should emit state_snapshot trace rows"
    );
    let mut saw_after_round_start = false;
    let mut saw_end_of_round = false;
    for e in &snaps {
        let payload = e
            .values
            .get("snapshot")
            .expect("state_snapshot row should carry values.snapshot");
        assert!(
            payload.get("anchor").is_some(),
            "snapshot should include anchor: {payload}"
        );
        match payload.get("anchor").and_then(|v| v.as_str()) {
            Some("after_round_start") => saw_after_round_start = true,
            Some("end_of_round_post_effects") => saw_end_of_round = true,
            _ => {}
        }
    }
    assert!(
        saw_after_round_start && saw_end_of_round,
        "anchors present: count={}",
        snaps.len()
    );
}

#[test]
fn schema_v3_fixture_passes_timeline_and_snapshots() {
    let json = std::fs::read_to_string(fixture_path("schema_v3_minimal_snapshot_log.json"))
        .expect("read fixture");
    let mut log = parse_combat_log_json(&json).expect("parse");
    assert_eq!(log.schema_version, 3);
    hydrate_ingested_state_snapshots_from_values(&mut log);
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v3_damage_without_following_snapshot_errors() {
    let json = r#"{
        "schema_version": 3,
        "rounds_simulated": 1,
        "total_damage": 10.0,
        "attacker_won": true,
        "defender_hull_remaining": 0.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round"},
            {"event_type": "damage_application", "round_index": 1, "phase": "damage", "weapon_index": 0},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"},
            {"event_type": "state_snapshot", "round_index": 1, "phase": "snapshot", "values": {"snapshot": {"anchor": "end_of_round_post_effects", "round_index": 1, "attacker": {"id": "a", "hull_remaining": 100.0, "shield_remaining": 0.0, "max_hull": 100.0, "max_shield": 0.0}, "defender": {"id": "d", "hull_remaining": 0.0, "shield_remaining": 0.0, "max_hull": 50.0, "max_shield": 0.0}, "flags": {"attacker_morale_active": false, "defender_morale_active": false, "defender_burning_active": false, "attacker_burning_active": false, "defender_hull_breach_active": false, "attacker_hull_breach_active": false, "assimilated_rounds_remaining": 0, "defender_assimilated_rounds_remaining": 0}, "total_defender_hull_damage": 50.0, "total_attacker_hull_damage": 0.0}}}
        ]
    }"#;
    let mut log = parse_combat_log_json(json).expect("parse");
    hydrate_ingested_state_snapshots_from_values(&mut log);
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome
            .errors
            .iter()
            .any(|e| e.contains("damage_application") && e.contains("state_snapshot")),
        "expected pairing error, got {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v4_client_minimal_fixture_passes_timeline() {
    let json = std::fs::read_to_string(fixture_path("schema_v4_client_minimal.json")).unwrap();
    let log = parse_combat_log_json(&json).expect("parse");
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v4_with_snapshots_uses_pairing_rules_like_v3() {
    let json =
        std::fs::read_to_string(fixture_path("schema_v3_minimal_snapshot_log.json")).unwrap();
    let mut log = parse_combat_log_json(&json).expect("parse");
    log.schema_version = 4;
    hydrate_ingested_state_snapshots_from_values(&mut log);
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome.errors.is_empty(),
        "unexpected errors: {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v4_stats_snapshot_requires_provenance_or_prefixes() {
    let json = r#"{
        "schema_version": 4,
        "rounds_simulated": 1,
        "total_damage": 1.0,
        "attacker_won": true,
        "defender_hull_remaining": 99.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round"},
            {"event_type": "damage_application", "round_index": 1, "phase": "damage",
             "stats_snapshot": {"hull_remaining": 99.0}},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"}
        ]
    }"#;
    let log = parse_combat_log_json(json).expect("parse");
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome.errors.iter().any(|e| e.contains("stats_snapshot")),
        "expected stats_snapshot provenance error, got {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v4_registered_client_kind_must_match_event_type() {
    let json = r#"{
        "schema_version": 4,
        "rounds_simulated": 1,
        "total_damage": 1.0,
        "attacker_won": true,
        "defender_hull_remaining": 99.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round",
             "client_kind": "fixture_kob_outbound_damage"},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"}
        ]
    }"#;
    let log = parse_combat_log_json(json).expect("parse");
    let outcome = validate_canonical_timeline(&log);
    assert!(
        outcome.errors.iter().any(|e| e.contains("client_kind")),
        "expected client_kind mismatch error, got {:?}",
        outcome.errors
    );
}

#[test]
fn schema_v4_collapsed_ambiguous_emits_warning() {
    let json = r#"{
        "schema_version": 4,
        "rounds_simulated": 1,
        "total_damage": 0.0,
        "attacker_won": true,
        "defender_hull_remaining": 100.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round"},
            {"event_type": "damage_application", "round_index": 1, "phase": "damage",
             "values": {"collapsed_ambiguous": true},
             "stats_snapshot": {"_provenance": {"source": "client"}}},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"}
        ]
    }"#;
    let log = parse_combat_log_json(json).expect("parse");
    let outcome = validate_canonical_timeline(&log);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.contains("collapsed_ambiguous")),
        "expected warning, got {:?}",
        outcome.warnings
    );
}

#[test]
fn expand_collapsed_repeat_events_matches_fixture() {
    let before = std::fs::read_to_string(fixture_path("collapsed_repeat_before.json")).unwrap();
    let after = std::fs::read_to_string(fixture_path("collapsed_repeat_expanded.json")).unwrap();
    let mut log = parse_combat_log_json(&before).expect("parse before");
    let expected = parse_combat_log_json(&after).expect("parse after");
    expand_collapsed_repeat_events(&mut log).expect("expand");
    assert_eq!(log, expected);
}

#[test]
fn tag_stats_snapshot_adds_provenance_for_plain_keys() {
    let json = r#"{
        "schema_version": 4,
        "rounds_simulated": 1,
        "total_damage": 1.0,
        "attacker_won": true,
        "defender_hull_remaining": 99.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round"},
            {"event_type": "damage_application", "round_index": 1, "phase": "damage",
             "stats_snapshot": {"hull_remaining": 99.0}},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"}
        ]
    }"#;
    let mut log = parse_combat_log_json(json).expect("parse");
    let outcome = validate_canonical_timeline(&log);
    assert!(!outcome.errors.is_empty());
    tag_stats_snapshot_sources_client_default(&mut log);
    let outcome = validate_canonical_timeline(&log);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn expand_collapsed_repeat_events_rejects_huge_count() {
    let json = r#"{
        "schema_version": 4,
        "rounds_simulated": 1,
        "total_damage": 0.0,
        "attacker_won": true,
        "defender_hull_remaining": 100.0,
        "events": [
            {"event_type": "round_start", "round_index": 1, "phase": "round"},
            {"event_type": "damage_application", "round_index": 1, "phase": "damage",
             "values": {"collapsed_repeat_count": 999}},
            {"event_type": "end_of_round_effects", "round_index": 1, "phase": "end"}
        ]
    }"#;
    let mut log = parse_combat_log_json(json).expect("parse");
    let err = expand_collapsed_repeat_events(&mut log).unwrap_err();
    assert!(err.contains("exceeds max"), "{err}");
}
