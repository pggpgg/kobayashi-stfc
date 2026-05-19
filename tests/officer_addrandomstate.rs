//! T'Ana / Zeph `AddRandomState` → weighted defender Morale / Hull Breach / Burning.

use std::path::Path;

use kobayashi::combat::abilities::{
    defender_morale_adjusted_pierce, pick_weighted_state_id, AbilityCondition, AbilityEffect,
};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration, ShipType,
    SimulationConfig, TraceMode,
};
use kobayashi::data::combat_effect_spec::AbilityModifierSpec;
use kobayashi::lcars::{
    index_lcars_officers_by_id, lcars_effect_to_combat_effect_spec, load_lcars_file,
    resolve_crew_to_buff_set, ResolveOptions,
};
const TANA_WEIGHTS: &[(u32, u32)] = &[(8, 8), (4, 4), (2, 2)];

fn passive_attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 500.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 50_000.0,
        shield_health: 10_000.0,
        shield_mitigation: 0.5,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

fn counter_attacker_defender() -> Combatant {
    Combatant {
        id: "hostile_bb".into(),
        attack: 800.0,
        mitigation: 0.3,
        pierce: 0.25,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 80_000.0,
        shield_health: 20_000.0,
        shield_mitigation: 0.6,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

fn sim_config(seed: u64, rounds: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Events,
        defender_level: Some(50),
        ..Default::default()
    }
}

fn random_state_events(events: &[kobayashi::combat::CombatEvent]) -> Vec<&kobayashi::combat::CombatEvent> {
    events
        .iter()
        .filter(|e| e.event_type == "random_defender_state_trigger")
        .collect()
}

fn event_bool(ev: &kobayashi::combat::CombatEvent, key: &str) -> bool {
    ev.values
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn event_str(ev: &kobayashi::combat::CombatEvent, key: &str) -> String {
    ev.values
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn pick_weighted_state_id_respects_eight_four_two_weights() {
    assert_eq!(pick_weighted_state_id(TANA_WEIGHTS, 0), 8);
    assert_eq!(pick_weighted_state_id(TANA_WEIGHTS, 7), 8);
    assert_eq!(pick_weighted_state_id(TANA_WEIGHTS, 8), 4);
    assert_eq!(pick_weighted_state_id(TANA_WEIGHTS, 11), 4);
    assert_eq!(pick_weighted_state_id(TANA_WEIGHTS, 12), 2);
}

#[test]
fn defender_morale_adjusted_pierce_boosts_battleship_primary_channel() {
    let base = 0.4;
    let boosted = defender_morale_adjusted_pierce(base, ShipType::Battleship, true);
    assert!((boosted - base * 1.1).abs() < 1e-9);
    let unchanged = defender_morale_adjusted_pierce(base, ShipType::Battleship, false);
    assert!((unchanged - base).abs() < 1e-9);
}

#[test]
fn zeph_bridge_compiles_random_defender_state_with_weights_and_rank_chance() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return;
    }
    let file = load_lcars_file(path).unwrap();
    let zeph = file
        .officers
        .iter()
        .find(|o| o.id == "zeph-21ee5c")
        .expect("zeph in LCARS");
    let bridge = zeph.bridge_ability.as_ref().expect("Zeph bridge");
    let effect = bridge
        .effects
        .iter()
        .find(|e| e.tag.as_deref() == Some("addrandomstate:unmapped"))
        .expect("addrandomstate bridge effect");
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "zeph:bridge:0",
        &zeph.id,
        &bridge.name,
        Some(5),
        None,
    )
    .expect("spec");
    assert_eq!(spec.modifier, AbilityModifierSpec::RandomDefenderState);
    let (_, compiled, cond) = compile_officer_combat_spec(&spec).unwrap();
    match compiled {
        AbilityEffect::RandomDefenderState {
            chance,
            duration_rounds,
            state_outcome_count,
            state_outcomes,
        } => {
            assert!((chance - 1.0).abs() < 1e-9, "rank 5 chance");
            assert_eq!(duration_rounds, 3);
            assert_eq!(state_outcome_count, 3);
            assert_eq!(state_outcomes[0], (8, 8));
            assert_eq!(state_outcomes[1], (4, 4));
            assert_eq!(state_outcomes[2], (2, 2));
        }
        other => panic!("expected RandomDefenderState, got {other:?}"),
    }
    assert!(
        matches!(
            &cond,
            Some(AbilityCondition::And(parts))
                if parts.iter().any(|c| matches!(c, AbilityCondition::DefenderIsNpcHostile))
        ),
        "Zeph gate should include DefenderIsNpcHostile, got {cond:?}"
    );
}

#[test]
fn zeph_fires_random_defender_state_vs_npc_hostile() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return;
    }
    let file = load_lcars_file(path).unwrap();
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };
    let crew = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &["zeph-21ee5c".to_string()],
        &[],
        &officers,
        &opts,
    )
    .crew;
    let setup = build_combat_setup(
        &passive_attacker(),
        &counter_attacker_defender(),
        &sim_config(77_031, 5),
        &crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let result = simulate_combat_from_setup(&setup, 77_031);
    let events = random_state_events(&result.events);
    assert!(
        !events.is_empty(),
        "Zeph vs hostile should emit random_defender_state_trigger"
    );
    assert!(
        events.iter().any(|e| event_bool(e, "triggered")),
        "rank-5 Zeph should proc at least once"
    );
    assert!(
        events.iter().any(|e| {
            matches!(
                event_str(e, "state").as_str(),
                "morale" | "hull_breach" | "burning"
            )
        }),
        "states should be morale/hull_breach/burning, not assimilated proxy"
    );
}

#[test]
fn tana_fires_random_defender_state_vs_player_defender_only() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return;
    }
    let file = load_lcars_file(path).unwrap();
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };
    let crew = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &["doctor-t-ana-b98f82".to_string()],
        &[],
        &officers,
        &opts,
    )
    .crew;

    let pvp_setup = build_combat_setup(
        &passive_attacker(),
        &counter_attacker_defender(),
        &sim_config(88_042, 5),
        &crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    let pvp = simulate_combat_from_setup(&pvp_setup, 88_042);
    let pvp_events = random_state_events(&pvp.events);
    assert!(
        pvp_events.iter().any(|e| event_bool(e, "triggered")),
        "T'Ana vs player defender should proc at rank 5"
    );

    let pve_setup = build_combat_setup(
        &passive_attacker(),
        &counter_attacker_defender(),
        &sim_config(88_042, 5),
        &crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let pve = simulate_combat_from_setup(&pve_setup, 88_042);
    assert!(
        random_state_events(&pve.events).is_empty(),
        "T'Ana should not fire vs NPC hostiles (defender_is_player_ship gate)"
    );
}

#[test]
fn zeph_weighted_pick_skews_toward_morale_over_burning() {
    let mut morale = 0u32;
    let mut burning = 0u32;
    for draw in 0..14_u64 {
        match pick_weighted_state_id(TANA_WEIGHTS, draw) {
            8 => morale += 1,
            2 => burning += 1,
            _ => {}
        }
    }
    assert!(morale > burning, "weight 8 should dominate weight 2 over draws 0..13");
}
