//! Log vs sim calibration: U.S.S. Enterprise-D vs V'ger Hurak (level 59) fight sample.
//! Fixture: [`fixtures/galaxy_ent_d_hurak59_log_outgoing.json`](fixtures/galaxy_ent_d_hurak59_log_outgoing.json).

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{replay_optimize_iteration_with_registry, DefenderOpponent};
use serde::Deserialize;
use std::sync::Arc;

const FIXTURE: &str = include_str!("fixtures/galaxy_ent_d_hurak59_log_outgoing.json");

#[derive(Debug, Deserialize)]
struct Hurak59LogFixture {
    hostile_id: String,
    ship_kobayashi_id: String,
    ship_tier: u32,
    ship_level: u32,
    total_outgoing_damage: f64,
    round_count: u32,
    #[serde(default)]
    morale_proc_rounds_from_log: Vec<u32>,
}

fn hurak59_candidate() -> CrewCandidate {
    CrewCandidate {
        captain: "annorax-830d35".to_string(),
        bridge: vec!["suder-d348a9".to_string(), "seska-848b5b".to_string()],
        below_decks: vec!["harry-kim-a79fdf (T4)".to_string()],
    }
}

#[test]
fn hurak59_log_fixture_and_sim_calibration() {
    std::env::set_var("KOBAYASHI_OFFICER_SOURCE", "lcars");
    let f: Hurak59LogFixture = serde_json::from_str(FIXTURE).expect("fixture JSON");
    assert_eq!(f.hostile_id, "518459749");
    assert!((f.total_outgoing_damage - 19_243_298_941.0).abs() < 1.0);
    assert_eq!(f.round_count, 23);
    assert_eq!(f.morale_proc_rounds_from_log.len(), 16);

    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    let candidate = hurak59_candidate();
    let replay = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        &f.ship_kobayashi_id,
        &f.hostile_id,
        Some(f.ship_tier),
        Some(f.ship_level),
        &candidate,
        12_345,
        0,
        Some(DEMO_PROFILE_ID),
        2_000_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !replay.using_placeholder_combatants,
        "ship/hostile must resolve (got placeholders)"
    );
    let log_total = f.total_outgoing_damage;
    let sim = replay.total_damage;
    let ratio = sim / log_total;
    eprintln!(
        "hurak59: sim_total_damage={sim:.0} log_outgoing_total={log_total:.0} ratio_sim_per_log={ratio:.3} rounds_sim={} attacker_won={}",
        replay.rounds_simulated,
        replay.attacker_won
    );
    assert!(
        ratio > 0.001 && ratio < 500.0,
        "sanity: ratio {ratio} out of bracket (sim={sim} log={log_total})"
    );

    // Demo profile has empty weapon_damage; additive-pool env should not change outcome vs off.
    std::env::remove_var("KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL");
    let replay_off = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        &f.ship_kobayashi_id,
        &f.hostile_id,
        Some(f.ship_tier),
        Some(f.ship_level),
        &candidate,
        99_001,
        0,
        Some(DEMO_PROFILE_ID),
        2_000_000,
        None,
        DefenderOpponent::Hostile,
    );
    std::env::set_var("KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL", "1");
    let replay_on = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        &f.ship_kobayashi_id,
        &f.hostile_id,
        Some(f.ship_tier),
        Some(f.ship_level),
        &candidate,
        99_001,
        0,
        Some(DEMO_PROFILE_ID),
        2_000_000,
        None,
        DefenderOpponent::Hostile,
    );
    std::env::remove_var("KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL");
    assert_eq!(
        replay_on.rounds_simulated, replay_off.rounds_simulated,
        "same seed/crew should produce same round count"
    );
    // `demo` profile merges `research.imported.json` etc.; merged `weapon_damage` is often > 0.
    // Pooled model should not increase total damage vs layered `(1+p)×(1+sum)`.
    assert!(
        replay_on.total_damage <= replay_off.total_damage * 1.000_000_1,
        "additive pool should not inflate total_damage (on={} off={})",
        replay_on.total_damage,
        replay_off.total_damage
    );
}
