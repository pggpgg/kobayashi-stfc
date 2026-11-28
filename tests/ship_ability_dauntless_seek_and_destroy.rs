//! U.S.S. Dauntless hull abilities (Seek and Destroy kit).
//!
//! Provenance:
//! - Feature article (qualitative only):
//!   <https://startrekfleetcommand.com/news/the-u-s-s-dauntless-revolutionizing-the-hostile-hunt/>
//! - Support FAQ: <https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/8588-the-dauntless/>
//! - Numeric values pinned by the stfc.space render of upstream ship 754460943
//!   (<https://stfc.space/ships/754460943>): Active Sweep shows 145,000% at level 1 up to
//!   1,200,000% at level 75 — i.e. upstream `values[]` (1450 … 12000) are engine-ready bonus
//!   fractions under the `{0:#.#%}` ×100 render convention, exactly like Athena's Fury.
//!
//! The four upstream ability rows (translations loca 83001–83004):
//! - **Active Sweep** (`1905773933`, loca 83002): +base Damage vs **non-Armada** hostiles while
//!   Seek and Destroy is active; per-level curve 1450 (L1) → 12000 (L75). The sim has no concept
//!   of the Seek and Destroy toggle, so the bonus is modeled always-on gated on
//!   `condition_opponent_ship_class_not: armada` + `condition_defender_is_npc_hostile` (a
//!   Dauntless hostile sim is by definition a Seek-and-Destroy hunt; the toggle cannot apply to
//!   armadas or players). Previously miscatalogued as a second flat `apex_barrier` because the
//!   combined loca 83002 description also contains the Active Apex Barrier paragraph.
//! - **Active Apex Barrier** (`985810609`, loca 83004): +40,000 Apex Barrier vs non-Armada
//!   hostiles while Seek and Destroy is active (flat across all levels); same gate.
//! - **Aft Expansion** (`3658971555`, loca 83001) and **Aggregation Plunder** (`915894112`,
//!   loca 83003): loot economy rows, catalogued `combat_noop`.

use kobayashi::combat::abilities::{
    attacker_apex_barrier_bonus_active, AbilityCondition, AbilityEffect, CombatContext,
};
use kobayashi::combat::condition::evaluate_ability_condition;
use kobayashi::combat::types::{EnemyTypes, OpponentFactionTag, ShipType};
use kobayashi::combat::CrewConfiguration;
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship::ShipAbility;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

const DAUNTLESS: &str = "uss_dauntless";
const ACTIVE_SWEEP: &str = "1905773933";
const ACTIVE_APEX_BARRIER: &str = "985810609";
const AFT_EXPANSION: &str = "3658971555";
const AGGREGATION_PLUNDER: &str = "915894112";

fn dauntless_ability(tier: u32, level: u32, ability_id: &str) -> ShipAbility {
    let record = resolve_ship_with_tier_level(DAUNTLESS, Some(tier), Some(level))
        .unwrap_or_else(|| panic!("{DAUNTLESS} should resolve from data/ships_extended"));
    record
        .abilities
        .iter()
        .flatten()
        .find(|a| a.id == ability_id)
        .unwrap_or_else(|| panic!("{DAUNTLESS} should carry hull ability {ability_id}"))
        .clone()
}

fn dauntless_seat(tier: u32, level: u32, ability_id: &str) -> kobayashi::combat::CrewSeatContext {
    let ability = dauntless_ability(tier, level, ability_id);
    ship_ability_to_crew_seat_context(&ability).unwrap_or_else(|| {
        panic!("ability {ability_id} should resolve to a combat seat context (not combat_noop)")
    })
}

/// Non-armada NPC hostiles only: Seek and Destroy cannot target armadas or player ships.
fn seek_and_destroy_gate() -> AbilityCondition {
    AbilityCondition::And(vec![
        AbilityCondition::Not(Box::new(AbilityCondition::DefenderShipTypeIs(
            ShipType::Armada,
        ))),
        AbilityCondition::DefenderIsNpcHostile,
    ])
}

fn ctx_vs(defender_ship_type: ShipType) -> CombatContext {
    CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_hull_pct_of_fight_start: None,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction: OpponentFactionTag::Unknown,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        defender_hull_faction_id: 0,
        defender_ship_type,
        attacker_ship_type: ShipType::Interceptor,
        attacker_ship_id: std::sync::Arc::from(DAUNTLESS),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: false,
        defender_hostile_tag_mask: 0,
        engagement_enemy_types: std::sync::Arc::new(EnemyTypes::default()),
        combat_battle_type_id: None,
        combat_game_context: kobayashi::combat::CombatGameContext::OpenSpace,
        attacker_is_defending: false,
        attacker_at_station: false,
        defender_level: Some(51),
    }
}

#[test]
fn active_sweep_resolves_as_non_armada_gated_component_base_weapon_damage() {
    // Level 1: stfc.space renders 145,000% ⇒ upstream values[0] = 1450 is the component-base bonus
    // coefficient (same additive-to-component-base family as Borg Sphere Omicron, NX-01, Monaveen;
    // verified against live T6 L30 U.S.S. Dauntless vs L65 Imperial Starfleet Planet Harvester).
    match dauntless_seat(1, 1, ACTIVE_SWEEP).ability.effect {
        AbilityEffect::ComponentBaseWeaponDamageBonus(v) => assert!(
            (v - 1450.0).abs() < 1e-6,
            "Active Sweep at T1/L1 resolved to {v}; expected 1450 (145,000% render)"
        ),
        ref other => panic!("expected ComponentBaseWeaponDamageBonus, got {other:?}"),
    }
    // Level 30: values[29] = 5335 (533,500% component-base damage).
    match dauntless_seat(6, 30, ACTIVE_SWEEP).ability.effect {
        AbilityEffect::ComponentBaseWeaponDamageBonus(v) => assert!(
            (v - 5335.0).abs() < 1e-6,
            "Active Sweep at T6/L30 resolved to {v}; expected 5335 (533,500% render)"
        ),
        ref other => panic!("expected ComponentBaseWeaponDamageBonus, got {other:?}"),
    }
    // Max level: 1,200,000% render ⇒ values[74] = 12000.
    let seat = dauntless_seat(15, 75, ACTIVE_SWEEP);
    match seat.ability.effect {
        AbilityEffect::ComponentBaseWeaponDamageBonus(v) => assert!(
            (v - 12_000.0).abs() < 1e-6,
            "Active Sweep at T15/L75 resolved to {v}; expected 12000 (1,200,000% render)"
        ),
        ref other => panic!("expected ComponentBaseWeaponDamageBonus, got {other:?}"),
    }
    assert_eq!(
        seat.ability.condition,
        Some(seek_and_destroy_gate()),
        "Active Sweep must gate on non-armada NPC hostiles"
    );
}

#[test]
fn active_sweep_adds_component_base_damage_without_multiplying_profile_pool() {
    use kobayashi::combat::{
        simulate_combat_with_defender_faction_and_defender_crew, Combatant, SimulationConfig,
        TraceMode, WeaponStats,
    };

    // Component base 13,053.5 scaled by ~72.57x profile pool -> attack 947,325.6.
    // With Active Sweep L30 (+5335 * component_base), expected effective_attack is
    // 947,325.6 + 5335 * 13,053.5 = 70,587,748.1 (NOT 947,325.6 * 5336 = 5.05 billion).
    let seat = dauntless_seat(6, 30, ACTIVE_SWEEP);
    let attacker = Combatant {
        id: DAUNTLESS.into(),
        attack: 947_325.6,
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
        hull_health: 100_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 947_325.6,
            component_base_attack: Some(13_053.5),
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "1381341814".into(),
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
        hull_health: 1_000_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    };
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let res = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: vec![seat] },
        OpponentFactionTag::MirrorUniverse,
        ShipType::Survey,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration { seats: vec![] },
    );
    let expected = 947_325.6 + 5335.0 * 13_053.5;
    assert!(
        (res.total_damage - expected).abs() < 1.0,
        "Active Sweep L30 dealt {:.1}, expected {:.1} (component_base_weapon_damage)",
        res.total_damage,
        expected
    );
}

#[test]
fn active_sweep_condition_fires_only_vs_non_armada_defenders() {
    let cond = dauntless_seat(15, 75, ACTIVE_SWEEP)
        .ability
        .condition
        .expect("Active Sweep carries the non-armada NPC-hostile gate");

    for ship_type in [
        ShipType::Interceptor,
        ShipType::Explorer,
        ShipType::Battleship,
        ShipType::Survey,
    ] {
        assert!(
            evaluate_ability_condition(&cond, &ctx_vs(ship_type)),
            "Active Sweep should fire against {ship_type:?} hostiles"
        );
    }
    assert!(
        !evaluate_ability_condition(&cond, &ctx_vs(ShipType::Armada)),
        "Active Sweep must stay inert against armada targets"
    );

    // Seek and Destroy only targets NPC hostiles — the bonus must stay inert in PvP.
    let pvp_ctx = CombatContext {
        defender_is_npc_hostile: false,
        defender_is_player_ship: true,
        ..ctx_vs(ShipType::Battleship)
    };
    assert!(
        !evaluate_ability_condition(&cond, &pvp_ctx),
        "Active Sweep must stay inert against player ships"
    );
}

#[test]
fn active_apex_barrier_resolves_flat_40k_with_non_armada_gate() {
    for (tier, level) in [(1u32, 1u32), (15, 75)] {
        let seat = dauntless_seat(tier, level, ACTIVE_APEX_BARRIER);
        match seat.ability.effect {
            AbilityEffect::ApexBarrierBonus(v) => assert!(
                (v - 40_000.0).abs() < 1e-6,
                "Active Apex Barrier at T{tier}/L{level} resolved to {v}; expected flat 40,000"
            ),
            ref other => panic!("expected ApexBarrierBonus, got {other:?}"),
        }
        assert_eq!(
            seat.ability.condition,
            Some(seek_and_destroy_gate()),
            "Active Apex Barrier must gate on non-armada NPC hostiles"
        );
    }

    // Defensive wiring: the barrier feeds the counter-fire apex factor only vs non-armada
    // defenders.
    let crew = CrewConfiguration {
        seats: vec![dauntless_seat(15, 75, ACTIVE_APEX_BARRIER)],
    };
    assert!(
        (attacker_apex_barrier_bonus_active(&crew, &ctx_vs(ShipType::Explorer)) - 40_000.0).abs()
            < 1e-6,
        "Active Apex Barrier should be active against non-armada hostiles"
    );
    assert_eq!(
        attacker_apex_barrier_bonus_active(&crew, &ctx_vs(ShipType::Armada)),
        0.0,
        "Active Apex Barrier must stay inert against armada targets"
    );
}

#[test]
fn loot_abilities_are_combat_noop() {
    for (name, id) in [
        ("Aft Expansion", AFT_EXPANSION),
        ("Aggregation Plunder", AGGREGATION_PLUNDER),
    ] {
        let ability = dauntless_ability(15, 75, id);
        assert_eq!(
            ability.effect_type, "combat_noop",
            "{name} ({id}) is a loot economy row and must stay catalogued combat_noop"
        );
        assert!(
            ship_ability_to_crew_seat_context(&ability).is_none(),
            "{name} ({id}) must not produce a combat seat"
        );
    }
}
