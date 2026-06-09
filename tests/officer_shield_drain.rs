//! SNW Sam Kirk captain: enemy-targeted per-round shield drain vs non-Armada hostiles.

use std::collections::HashMap;
use std::sync::OnceLock;

use kobayashi::combat::abilities::{active_effects_for_timing, CrewConfiguration, TimingWindow};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, AbilityEffect, Combatant, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::combat_effect_spec::AbilityModifierSpec;
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id,
    lcars_effect_to_combat_effect_spec, resolve_crew_to_buff_set, LcarsOfficer, ResolveOptions,
};

fn lcars_officers_by_id() -> &'static HashMap<String, LcarsOfficer> {
    static OFFICERS: OnceLock<HashMap<String, LcarsOfficer>> = OnceLock::new();
    OFFICERS.get_or_init(|| {
        let file = build_officer_model_file_default().expect("build officer model");
        index_lcars_officers_by_id(file.officers)
    })
}

fn resolve_crew(captain: &str, tier: u8) -> CrewConfiguration {
    let officers = lcars_officers_by_id();
    let opts = ResolveOptions {
        tier: Some(tier),
        officer_tiers: None,
        officer_levels: None,
    };
    resolve_crew_to_buff_set(captain, &[], &[], officers, &opts).crew
}

fn hostile_defender() -> Combatant {
    Combatant {
        id: "hostile".into(),
        attack: 80.0,
        mitigation: 0.15,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 80_000.0,
        shield_health: 20_000.0,
        shield_mitigation: 0.5,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 80.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 2_000.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.85,
        crit_chance: 0.0,
        crit_multiplier: 1.5,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 60_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 2_000.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

#[test]
fn snw_sam_kirk_captain_compiles_defender_shield_drain_per_round() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let kirk = file
        .officers
        .into_iter()
        .find(|o| o.id == "snw-sam-kirk-0a77f9")
        .expect("sam kirk");
    let cap = kirk.captain_ability.expect("captain");
    let effect = cap
        .effects
        .iter()
        .find(|e| e.stat.as_deref() == Some("shield_regen") && e.target.as_deref() == Some("enemy"))
        .expect("enemy shield drain effect");
    assert_eq!(effect.trigger.as_deref(), Some("on_round_start"));
    assert_eq!(effect.operator.as_deref(), Some("sub"));
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:sam-kirk:drain",
        "snw-sam-kirk-0a77f9",
        &cap.name,
        Some(1),
        None,
    )
    .expect("compile spec");
    assert_eq!(spec.modifier, AbilityModifierSpec::OfficerShieldRegenFlat);
    let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("runtime");
    match compiled {
        AbilityEffect::DefenderShieldDrainPerRound {
            fraction,
            duration_rounds,
        } => {
            assert!((fraction - 0.1).abs() < 1e-9);
            assert!(duration_rounds >= 10);
        }
        other => panic!("expected DefenderShieldDrainPerRound, got {other:?}"),
    }
}

#[test]
fn snw_sam_kirk_drains_npc_hostile_shields_at_round_start() {
    let crew = resolve_crew("snw-sam-kirk-0a77f9", 1);
    assert!(
        active_effects_for_timing(&crew, TimingWindow::RoundStart)
            .iter()
            .any(|e| matches!(e.effect, AbilityEffect::DefenderShieldDrainPerRound { .. })),
        "Sam Kirk should resolve round-start defender shield drain on crew"
    );

    let config = SimulationConfig {
        rounds: 4,
        seed: 7,
        trace_mode: TraceMode::Off,
        defender_level: Some(40),
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker(),
        &hostile_defender(),
        &config,
        &crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let baseline = build_combat_setup(
        &attacker(),
        &hostile_defender(),
        &config,
        &CrewConfiguration::default(),
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with_kirk = simulate_combat_from_setup(&setup, 7);
    let base = simulate_combat_from_setup(&baseline, 7);
    assert!(
        with_kirk.defender_shield_remaining < base.defender_shield_remaining,
        "round-start drain should leave fewer defender shields (kirk={}, base={})",
        with_kirk.defender_shield_remaining,
        base.defender_shield_remaining
    );
}
