//! U.S.S. Athena hull ability "Athena's Fury" (`2357321655`): a faction-gated weapon-damage
//! multiplier that only fires against Venari Ral [VENRA] hostiles (June 2026 stfc.space refresh,
//! ROADMAP item 16). Guards the same two dormancy failure modes called out in
//! `SHIP_ABILITY_COMBAT_NOOP_AUDIT.md` §6.1 (catalog not regenerated → `combat_noop`; value
//! mis-scaled) plus the faction-gate wiring that is new for this ship:
//!   1. The catalog/override row resolves to an `AttackMultiplier` out of real `ships_extended`.
//!   2. The seat condition gates on `DefenderFactionIs(VenariRal)`.
//!   3. Venari Ral hostiles in `data/hostiles` map to `OpponentFactionTag::VenariRal`, so the gate
//!      actually closes over real hostile data (and stays shut for other factions).

use kobayashi::combat::abilities::{AbilityCondition, AbilityEffect, CombatContext};
use kobayashi::combat::condition::evaluate_ability_condition;
use kobayashi::combat::types::{EnemyTypes, OpponentFactionTag, ShipType};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

const ATHENA: &str = "uss_athena";
const ATHENA_FURY: &str = "2357321655";

fn athena_fury_seat() -> kobayashi::combat::CrewSeatContext {
    let record = resolve_ship_with_tier_level(ATHENA, Some(15), Some(75))
        .unwrap_or_else(|| panic!("{ATHENA} should resolve from data/ships_extended"));
    let ability = record
        .abilities
        .iter()
        .flatten()
        .find(|a| a.id == ATHENA_FURY)
        .unwrap_or_else(|| {
            panic!("{ATHENA} should carry hull ability {ATHENA_FURY} (Athena's Fury)")
        });
    ship_ability_to_crew_seat_context(ability).unwrap_or_else(|| {
        panic!("Athena's Fury should resolve to a combat seat context (not combat_noop)")
    })
}

#[test]
fn athena_fury_resolves_as_faction_gated_attack_multiplier() {
    let seat = athena_fury_seat();

    match seat.ability.effect {
        // Anti-faction hard-counter curve resolves large (mirrors the shipped Xindi ship);
        // the lower bound only guards against a double-scaled `value_is_percentage` (~0.001).
        AbilityEffect::AttackMultiplier(v) => assert!(
            v >= 1.0,
            "Athena's Fury attack multiplier resolved to {v}; expected a meaningful value"
        ),
        ref other => panic!("expected AttackMultiplier, got {other:?}"),
    }

    assert_eq!(
        seat.ability.condition,
        Some(AbilityCondition::DefenderFactionIs(
            OpponentFactionTag::VenariRal
        )),
        "Athena's Fury must gate on the Venari Ral defender faction"
    );
}

#[test]
fn athena_fury_condition_fires_only_vs_venari_ral() {
    let cond = athena_fury_seat()
        .ability
        .condition
        .expect("Athena's Fury carries a faction condition");

    let ctx_for = |faction| CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction: faction,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        defender_hull_faction_id: 0,
        defender_ship_type: ShipType::Battleship,
        attacker_ship_type: ShipType::Explorer,
        attacker_ship_id: std::sync::Arc::from(ATHENA),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: false,
        defender_hostile_tag_mask: 0,
        engagement_enemy_types: std::sync::Arc::new(EnemyTypes::default()),
        combat_battle_type_id: None,
        defender_level: Some(79),
    };

    assert!(
        evaluate_ability_condition(&cond, &ctx_for(OpponentFactionTag::VenariRal)),
        "Athena's Fury should fire against Venari Ral hostiles"
    );
    assert!(
        !evaluate_ability_condition(&cond, &ctx_for(OpponentFactionTag::Federation)),
        "Athena's Fury must stay inert against non-Venari-Ral defenders"
    );
}

#[test]
fn venari_ral_hostiles_map_to_venari_ral_faction_tag() {
    // Indexed Venari Ral hostiles (faction.id 331567901 / loca 90001).
    for id in ["3125794077", "311200395", "26336879"] {
        let rec = resolve_hostile(id)
            .unwrap_or_else(|| panic!("Venari Ral hostile {id} should resolve from data/hostiles"));
        assert_eq!(
            rec.opponent_faction_tag(),
            OpponentFactionTag::VenariRal,
            "hostile {id} should map to the Venari Ral faction tag"
        );
    }

    // A Klingon hostile must not match the gate.
    let klingon = resolve_hostile("2136016893").expect("Klingon hostile should resolve");
    assert_ne!(
        klingon.opponent_faction_tag(),
        OpponentFactionTag::VenariRal,
        "non-Venari-Ral hostiles must not match the Athena gate"
    );
}
