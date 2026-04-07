//! Trace invariant harness: [`kobayashi::calibration::check_trace_invariants`] on drift fixtures and synthetic traces.

use std::path::Path;

use kobayashi::calibration::{
    check_trace_invariants, list_drift_fixture_paths, load_drift_fixture,
    simulate_drift_fixture_traced, TraceInvariantContext,
};
use kobayashi::combat::{CombatEvent, EventSource};
use serde_json::{Map, Value};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recorded_fights")
}

#[test]
fn drift_fixtures_satisfy_trace_invariants() {
    let dir = fixtures_dir();
    let paths = list_drift_fixture_paths(&dir).expect("list drift fixtures");
    assert!(
        !paths.is_empty(),
        "expected at least one drift_*.json under {:?}",
        dir
    );

    for path in &paths {
        let spec = load_drift_fixture(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let result = simulate_drift_fixture_traced(&spec);
        let ctx = TraceInvariantContext {
            max_config_rounds: spec.simulation.rounds,
            expect_monotonic_defender_running_hull_damage: false,
        };
        check_trace_invariants(&result.events, &ctx).unwrap_or_else(|errs| {
            panic!(
                "trace invariants failed for {}:\n{:#?}",
                path.display(),
                errs
            )
        });
    }
}

#[test]
fn empty_trace_passes() {
    let ctx = TraceInvariantContext {
        max_config_rounds: 10,
        ..Default::default()
    };
    check_trace_invariants(&[], &ctx).expect("empty");
}

fn damage_application(round: u32, hull_damage: f64, running: f64) -> CombatEvent {
    CombatEvent {
        event_type: "damage_application".into(),
        round_index: round,
        phase: "damage".into(),
        source: EventSource::default(),
        values: Map::from_iter([
            ("damage_after_apex".into(), Value::from(1.0)),
            ("shield_damage".into(), Value::from(0.0)),
            ("hull_damage".into(), Value::from(hull_damage)),
            ("running_hull_damage".into(), Value::from(running)),
            ("defender_shield_remaining".into(), Value::from(0.0)),
            ("shield_mitigation".into(), Value::from(0.0)),
        ]),
        weapon_index: Some(0),
    }
}

#[test]
fn round_index_must_not_decrease() {
    let events = vec![
        damage_application(2, 1.0, 1.0),
        damage_application(1, 1.0, 2.0),
    ];
    let ctx = TraceInvariantContext {
        max_config_rounds: 5,
        ..Default::default()
    };
    let err = check_trace_invariants(&events, &ctx).unwrap_err();
    assert!(err.iter().any(|e| e.code == "round_order"));
}

#[test]
fn negative_hull_damage_fails() {
    let events = vec![damage_application(1, -0.1, 0.0)];
    let ctx = TraceInvariantContext {
        max_config_rounds: 5,
        ..Default::default()
    };
    let err = check_trace_invariants(&events, &ctx).unwrap_err();
    assert!(err.iter().any(|e| e.code == "damage_numeric"));
}

#[test]
fn monotonic_running_hull_optional_flag() {
    let events = vec![
        damage_application(1, 1.0, 5.0),
        damage_application(1, 1.0, 4.0),
    ];
    let ctx = TraceInvariantContext {
        max_config_rounds: 5,
        expect_monotonic_defender_running_hull_damage: true,
    };
    let err = check_trace_invariants(&events, &ctx).unwrap_err();
    assert!(err.iter().any(|e| e.code == "running_hull_monotone"));
}

#[test]
fn mitigation_multiplier_consistency_detects_mismatch() {
    let events = vec![CombatEvent {
        event_type: "mitigation_calc".into(),
        round_index: 1,
        phase: "defense".into(),
        source: EventSource::default(),
        values: Map::from_iter([
            ("mitigation".into(), Value::from(0.2)),
            ("multiplier".into(), Value::from(0.5)),
        ]),
        weapon_index: Some(0),
    }];
    let ctx = TraceInvariantContext {
        max_config_rounds: 5,
        ..Default::default()
    };
    let err = check_trace_invariants(&events, &ctx).unwrap_err();
    assert!(err
        .iter()
        .any(|e| e.code == "mitigation_multiplier_consistency"));
}

#[test]
fn hit_index_sequence_must_be_contiguous() {
    let mk = |hi: u64| -> CombatEvent {
        CombatEvent {
            event_type: "attack_roll".into(),
            round_index: 1,
            phase: "attack".into(),
            source: EventSource::default(),
            values: Map::from_iter([("hit_index".into(), Value::from(hi))]),
            weapon_index: Some(0),
        }
    };
    let events = vec![mk(0), mk(2)];
    let ctx = TraceInvariantContext {
        max_config_rounds: 5,
        ..Default::default()
    };
    let err = check_trace_invariants(&events, &ctx).unwrap_err();
    assert!(err.iter().any(|e| e.code == "hit_index_sequence"));
}
