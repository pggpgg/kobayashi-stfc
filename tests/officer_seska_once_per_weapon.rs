//! Seska (`seska-848b5b`) below-decks "Increases Critical Hit Damage by X for 4 rounds each time
//! you score a hit (Once per weapon)": modeled as [`AbilityEffect::OnHitCritDamageStack`] — at
//! most one concurrent stack per weapon, refreshed when that weapon hits again.

use std::collections::HashMap;
use std::sync::OnceLock;

use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, AbilityEffect, Combatant, CrewConfiguration,
    ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    LcarsOfficer, ResolveOptions,
};

fn lcars_officers_by_id() -> &'static HashMap<String, LcarsOfficer> {
    static OFFICERS: OnceLock<HashMap<String, LcarsOfficer>> = OnceLock::new();
    OFFICERS.get_or_init(|| {
        let file = build_officer_model_file_default().expect("build officer model");
        index_lcars_officers_by_id(file.officers)
    })
}

fn crew(captain: &str, below_decks: &[String]) -> CrewConfiguration {
    let officers = lcars_officers_by_id();
    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };
    resolve_crew_to_buff_set(captain, &[], below_decks, officers, &opts).crew
}

fn seska_below_decks() -> Vec<String> {
    vec!["seska-848b5b".to_string()]
}

/// All-crit attacker so the crit-damage stacks are observable; zero proc chance keeps damage
/// deterministic per seed (RNG rolls are drawn but cannot change outcomes).
fn crit_attacker(weapon_count: usize) -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 2_500.0 * weapon_count as f64,
        pierce: 0.9,
        crit_chance: 1.0,
        crit_multiplier: 1.5,
        proc_multiplier: 1.0,
        hull_health: 500_000.0,
        shield_health: 5_000.0,
        shield_mitigation: 0.3,
        weapons: (0..weapon_count)
            .map(|_| WeaponStats {
                attack: 2_500.0,
                shots: Some(1),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn npc_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 50.0,
        mitigation: 0.2,
        crit_multiplier: 1.0,
        proc_multiplier: 1.0,
        hull_health: 5_000_000.0,
        shield_health: 8_000.0,
        shield_mitigation: 0.5,
        ..Default::default()
    }
}

fn total_damage(attacker: &Combatant, crew: &CrewConfiguration, seed: u64, rounds: u32) -> f64 {
    let config = SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Events,
        defender_level: Some(50),
        ..Default::default()
    };
    let setup = build_combat_setup(
        attacker,
        &npc_defender(),
        &config,
        crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    simulate_combat_from_setup(&setup, seed).total_damage
}

#[test]
fn seska_below_decks_resolves_on_hit_crit_stack() {
    let crew = crew("kirk-1323b6", &seska_below_decks());
    let seat = crew
        .seats
        .iter()
        .find(|s| matches!(s.ability.effect, AbilityEffect::OnHitCritDamageStack { .. }))
        .expect("Seska below-decks row should compile to OnHitCritDamageStack");
    match seat.ability.effect {
        AbilityEffect::OnHitCritDamageStack {
            bonus,
            duration_rounds,
        } => {
            assert!(bonus > 0.0, "positive additive crit-damage bonus");
            assert_eq!(duration_rounds, 4, "canonical num_rounds=4");
        }
        _ => unreachable!(),
    }
    assert!(
        seat.ability.condition.is_some(),
        "EnemyHostile / TargetNotArmada gates must survive compilation"
    );
}

#[test]
fn seska_increases_crit_damage_on_multi_weapon_ship() {
    let baseline = crew("kirk-1323b6", &[]);
    let with_seska = crew("kirk-1323b6", &seska_below_decks());
    let attacker = crit_attacker(3);
    for seed in 0..8_u64 {
        let base = total_damage(&attacker, &baseline, seed, 4);
        let seska = total_damage(&attacker, &with_seska, seed, 4);
        assert!(
            seska > base,
            "seed {seed}: on-hit crit stacks should raise all-crit damage (base={base}, seska={seska})"
        );
    }
}

#[test]
fn seska_single_weapon_single_round_matches_baseline() {
    // Round 1, one weapon: the stack arms on that weapon's hit but no later shot exists to
    // benefit — total damage must equal the no-Seska baseline.
    let baseline = crew("kirk-1323b6", &[]);
    let with_seska = crew("kirk-1323b6", &seska_below_decks());
    let attacker = crit_attacker(1);
    for seed in 0..8_u64 {
        assert_eq!(
            total_damage(&attacker, &with_seska, seed, 1),
            total_damage(&attacker, &baseline, seed, 1),
            "seed {seed}: no shot after the arming hit in a 1-round, 1-weapon fight"
        );
    }
}

#[test]
fn seska_later_weapons_benefit_within_round_one() {
    // Within round 1 of a 3-weapon ship, weapons 2 and 3 fire after weapon 1's hit armed a
    // stack, so even a single-round fight shows a gain vs baseline.
    let baseline = crew("kirk-1323b6", &[]);
    let with_seska = crew("kirk-1323b6", &seska_below_decks());
    let attacker = crit_attacker(3);
    for seed in 0..8_u64 {
        let base = total_damage(&attacker, &baseline, seed, 1);
        let seska = total_damage(&attacker, &with_seska, seed, 1);
        assert!(
            seska > base,
            "seed {seed}: later weapons in round 1 should benefit from earlier hits (base={base}, seska={seska})"
        );
    }
}

#[test]
fn seska_same_seed_is_deterministic() {
    let with_seska = crew("kirk-1323b6", &seska_below_decks());
    let attacker = crit_attacker(3);
    let a = total_damage(&attacker, &with_seska, 42, 6);
    let b = total_damage(&attacker, &with_seska, 42, 6);
    assert_eq!(a, b, "same seed must reproduce the same fight exactly");
}
