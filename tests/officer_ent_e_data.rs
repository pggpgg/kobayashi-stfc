//! Ent-E Data bridge: isolytic cascade at combat start vs non-Armada NPC hostiles only.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use kobayashi::combat::abilities::{
    active_effects_for_timing, AbilityCondition, CrewConfiguration, TimingWindow,
};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, AbilityEffect, Combatant, OpponentFactionTag,
    ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::combat_effect_spec::AbilityModifierSpec;
use kobayashi::lcars::{
    index_lcars_officers_by_id, lcars_effect_to_combat_effect_spec, load_lcars_file,
    resolve_crew_to_buff_set, LcarsOfficer, ResolveOptions,
};

fn lcars_officers_by_id() -> &'static HashMap<String, LcarsOfficer> {
    static OFFICERS: OnceLock<HashMap<String, LcarsOfficer>> = OnceLock::new();
    OFFICERS.get_or_init(|| {
        let path = Path::new("data/officers/officers.lcars.yaml");
        let file = load_lcars_file(path).expect("officers.lcars.yaml");
        index_lcars_officers_by_id(file.officers)
    })
}

fn resolve_crew(tier: u8) -> CrewConfiguration {
    let officers = lcars_officers_by_id();
    let opts = ResolveOptions {
        tier: Some(tier),
        officer_tiers: None,
        officer_levels: None,
    };
    resolve_crew_to_buff_set("", &["ent-e-data-871245".into()], &[], officers, &opts).crew
}

fn attacker_with_isolytic_base() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 500.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.5,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.1,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn passive_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 0.0,
        mitigation: 0.0,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

#[test]
fn ent_e_data_bridge_lcars_maps_non_armada_hostile_gate() {
    if !Path::new("data/officers/officers.lcars.yaml").exists() {
        return;
    }
    let file = load_lcars_file("data/officers/officers.lcars.yaml").expect("load lcars");
    let data = file
        .officers
        .into_iter()
        .find(|o| o.id == "ent-e-data-871245")
        .expect("ent-e data");
    let bridge = data.bridge_ability.expect("bridge");
    let effect = bridge
        .effects
        .iter()
        .find(|e| e.stat.as_deref() == Some("isolytic_cascade_damage"))
        .expect("isolytic cascade effect");
    assert_eq!(effect.trigger.as_deref(), Some("on_combat_start"));
    let cond = effect.condition.as_ref().expect("condition");
    assert_eq!(cond.condition_type, "and");
    let kids = cond.conditions.as_ref().expect("and children");
    assert!(kids
        .iter()
        .any(|c| c.condition_type == "defender_is_npc_hostile"));
    let not_armada = kids
        .iter()
        .find(|c| c.condition_type == "not")
        .expect("TargetNotArmada → not(defender_ship_type_is armada)");
    let inner = not_armada
        .conditions
        .as_ref()
        .and_then(|v| v.first())
        .expect("not inner");
    assert_eq!(inner.condition_type, "defender_ship_type_is");
    assert_eq!(inner.ship_type.as_deref(), Some("armada"));

    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:ent-e-data:cascade",
        "ent-e-data-871245",
        &bridge.name,
        Some(5),
        None,
    )
    .expect("compile spec");
    assert_eq!(spec.modifier, AbilityModifierSpec::IsolyticCascadeDamage);
    let (_, compiled, runtime_cond) = compile_officer_combat_spec(&spec).expect("runtime");
    assert!(matches!(
        compiled,
        AbilityEffect::IsolyticCascadeDamageBonus(v) if (v - 0.4).abs() < 1e-9
    ));
    match runtime_cond {
        Some(AbilityCondition::And(parts)) => {
            assert!(parts
                .iter()
                .any(|p| matches!(p, AbilityCondition::DefenderIsNpcHostile)));
            let not_armada = parts.iter().find_map(|p| match p {
                AbilityCondition::Not(inner) => match inner.as_ref() {
                    AbilityCondition::DefenderShipTypeIs(ShipType::Armada) => Some(()),
                    _ => None,
                },
                _ => None,
            });
            assert!(
                not_armada.is_some(),
                "expected Not(DefenderShipTypeIs(Armada))"
            );
        }
        other => panic!("expected And condition, got {other:?}"),
    }
}

#[test]
fn ent_e_data_isolytic_cascade_inactive_vs_armada_defender() {
    if !Path::new("data/officers/officers.lcars.yaml").exists() {
        return;
    }
    let crew = resolve_crew(5);
    assert!(
        active_effects_for_timing(&crew, TimingWindow::CombatBegin)
            .iter()
            .any(|e| matches!(e.effect, AbilityEffect::IsolyticCascadeDamageBonus(_))),
        "Ent-E Data should resolve combat-begin isolytic cascade on crew"
    );
    assert!(
        resolve_crew_to_buff_set(
            "",
            &["ent-e-data-871245".into()],
            &[],
            lcars_officers_by_id(),
            &ResolveOptions {
                tier: Some(5),
                ..Default::default()
            }
        )
        .static_buffs
        .is_empty(),
        "conditional cascade must not leak into unconditional static_buffs"
    );

    let config = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let attacker = attacker_with_isolytic_base();
    let defender = passive_defender();
    let empty = CrewConfiguration::default();

    for (label, defender_type) in [
        ("battleship", ShipType::Battleship),
        ("armada", ShipType::Armada),
    ] {
        let with = simulate_combat_from_setup(
            &build_combat_setup(
                &attacker,
                &defender,
                &config,
                &crew,
                OpponentFactionTag::Unknown,
                defender_type,
                ShipType::Battleship,
                true,
                false,
                &empty,
            ),
            11,
        )
        .total_damage;
        let base = simulate_combat_from_setup(
            &build_combat_setup(
                &attacker,
                &defender,
                &config,
                &empty,
                OpponentFactionTag::Unknown,
                defender_type,
                ShipType::Battleship,
                true,
                false,
                &empty,
            ),
            11,
        )
        .total_damage;
        let delta = with - base;
        if label == "battleship" {
            assert!(
                delta > 100.0,
                "vs non-Armada hostile cascade should increase damage (delta={delta})"
            );
        } else {
            assert!(
                delta.abs() < 1e-6,
                "vs Armada defender cascade must not apply (delta={delta})"
            );
        }
    }
}
