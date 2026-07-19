//! U.S.S. Athena hull abilities (Update 91, June 2026 stfc.space refresh).
//!
//! Provenance for the modeled values (all three sources quote consistent numbers):
//! - Update 91 feature highlight:
//!   <https://startrekfleetcommand.com/news/update-91-feature-highlight-the-u-s-s-athena/>
//! - Support FAQ: <https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/8799-the-uss-athena/>
//! - Critical Mitigation guide (formula + worked example):
//!   <https://startrekfleetcommand.com/news/starfleet-academy-remote-campus-critical-mitigation/>
//!
//! The four upstream ability rows (translations loca 91000–91003):
//! - **Athena's Fury** (`2357321655`): +Weapon Damage `{0:#.#%}` vs Venari Ral. The official
//!   "11,000,000%" quote matches upstream `values[24]` = 110,000 (level 25) under the
//!   `{0:#.#%}` ×100 render convention — i.e. raw values are engine-ready bonus fractions,
//!   85,000 at L1 up to 20,000,000 at L75. The huge magnitude is intentional (hard-counter).
//! - **Athena's Revenge** (`1913694321`): +Critical Mitigation `{0:#}` (flat rating) vs
//!   Venari Ral. Modeled as `crit_mitigation_rating` → `HostileCritDamageReduction` with
//!   `reduction = CM / (CM + 50,000)`, pinned by the official worked example
//!   (83,000 ⇒ 62.41% of the full crit damage).
//! - **Athena's Valor** (`2506949026`): +Apex Barrier 10,000,000 vs Venari Ral (defensive:
//!   counter-fire apex factor only).
//! - **Athena's Wrath** (`39689355`): +Apex Barrier 15,000,000 vs Academy Drones in Wave
//!   Defense — out of simulated scenario scope, catalogued `combat_noop`.

use kobayashi::combat::abilities::{
    attacker_apex_barrier_bonus_active, hostile_crit_damage_reduction_active_at_round,
    AbilityCondition, AbilityEffect, CombatContext,
};
use kobayashi::combat::condition::evaluate_ability_condition;
use kobayashi::combat::types::{EnemyTypes, OpponentFactionTag, ShipType};
use kobayashi::combat::CrewConfiguration;
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship::ShipAbility;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

const ATHENA: &str = "uss_athena";
const ATHENA_FURY: &str = "2357321655";
const ATHENA_REVENGE: &str = "1913694321";
const ATHENA_VALOR: &str = "2506949026";
const ATHENA_WRATH: &str = "39689355";

fn athena_ability(tier: u32, level: u32, ability_id: &str) -> ShipAbility {
    let record = resolve_ship_with_tier_level(ATHENA, Some(tier), Some(level))
        .unwrap_or_else(|| panic!("{ATHENA} should resolve from data/ships_extended"));
    record
        .abilities
        .iter()
        .flatten()
        .find(|a| a.id == ability_id)
        .unwrap_or_else(|| panic!("{ATHENA} should carry hull ability {ability_id}"))
        .clone()
}

fn athena_seat(tier: u32, level: u32, ability_id: &str) -> kobayashi::combat::CrewSeatContext {
    let ability = athena_ability(tier, level, ability_id);
    ship_ability_to_crew_seat_context(&ability).unwrap_or_else(|| {
        panic!("ability {ability_id} should resolve to a combat seat context (not combat_noop)")
    })
}

fn athena_fury_seat() -> kobayashi::combat::CrewSeatContext {
    athena_seat(15, 75, ATHENA_FURY)
}

#[test]
fn athena_fury_resolves_as_faction_gated_attack_multiplier() {
    // Exact per-level curve endpoints, validated against the official 11,000,000% quote
    // (= upstream values[24] = 110,000 at level 25 under the {0:#.#%} render convention).
    match athena_seat(1, 1, ATHENA_FURY).ability.effect {
        AbilityEffect::AttackMultiplier(v) => assert!(
            (v - 85_000.0).abs() < 1e-6,
            "Athena's Fury at T1/L1 resolved to {v}; expected 85,000 (upstream values[0])"
        ),
        ref other => panic!("expected AttackMultiplier, got {other:?}"),
    }
    let seat = athena_fury_seat();
    match seat.ability.effect {
        AbilityEffect::AttackMultiplier(v) => assert!(
            (v - 20_000_000.0).abs() < 1e-6,
            "Athena's Fury at T15/L75 resolved to {v}; expected 20,000,000 (upstream values[74])"
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
fn athena_valor_resolves_as_faction_gated_apex_barrier() {
    let seat = athena_seat(15, 75, ATHENA_VALOR);
    match seat.ability.effect {
        AbilityEffect::ApexBarrierBonus(v) => assert!(
            (v - 10_000_000.0).abs() < 1e-6,
            "Athena's Valor resolved to {v}; expected the official flat 10,000,000"
        ),
        ref other => panic!("expected ApexBarrierBonus, got {other:?}"),
    }
    assert_eq!(
        seat.ability.condition,
        Some(AbilityCondition::DefenderFactionIs(
            OpponentFactionTag::VenariRal
        )),
        "Athena's Valor must gate on the Venari Ral defender faction"
    );

    // Defensive wiring: the barrier feeds the counter-fire apex factor only when the
    // fight is actually against a Venari Ral defender.
    let crew = CrewConfiguration { seats: vec![seat] };
    assert!(
        (attacker_apex_barrier_bonus_active(&crew, &ctx_for(OpponentFactionTag::VenariRal))
            - 10_000_000.0)
            .abs()
            < 1e-6,
        "Valor's barrier should be active against Venari Ral defenders"
    );
    assert_eq!(
        attacker_apex_barrier_bonus_active(&crew, &ctx_for(OpponentFactionTag::Federation)),
        0.0,
        "Valor's barrier must stay inert against non-Venari-Ral defenders"
    );
}

#[test]
fn athena_revenge_resolves_as_faction_gated_crit_mitigation() {
    // Level 1: rating 140,000 ⇒ reduction 140,000 / 190,000.
    match athena_seat(1, 1, ATHENA_REVENGE).ability.effect {
        AbilityEffect::HostileCritDamageReduction { reduction, .. } => assert!(
            (reduction - 140_000.0 / 190_000.0).abs() < 1e-9,
            "Athena's Revenge at L1 resolved to {reduction}; expected 140,000/190,000"
        ),
        ref other => panic!("expected HostileCritDamageReduction, got {other:?}"),
    }
    // Max level: rating 2,000,000 ⇒ 0.9756… clamped to the resolver's 0.95 cap.
    let seat = athena_seat(15, 75, ATHENA_REVENGE);
    match seat.ability.effect {
        AbilityEffect::HostileCritDamageReduction {
            reduction,
            duration_rounds,
            additive_percentage_points,
            stacks,
        } => {
            assert!(
                (reduction - 0.95).abs() < 1e-9,
                "Athena's Revenge at T15/L75 resolved to {reduction}; expected the 0.95 clamp"
            );
            assert_eq!(
                duration_rounds,
                u32::MAX,
                "always active (no duration in text)"
            );
            assert!(!additive_percentage_points);
            assert!(!stacks);
        }
        ref other => panic!("expected HostileCritDamageReduction, got {other:?}"),
    }
    assert_eq!(
        seat.ability.condition,
        Some(AbilityCondition::DefenderFactionIs(
            OpponentFactionTag::VenariRal
        )),
        "Athena's Revenge must gate on the Venari Ral defender faction"
    );

    // The counter-fire helper honors the gate, deep into the fight (always active).
    let crew = CrewConfiguration { seats: vec![seat] };
    let vs_venra = hostile_crit_damage_reduction_active_at_round(
        &crew,
        &ctx_for(OpponentFactionTag::VenariRal),
        50,
    );
    assert!(
        (vs_venra.multiplicative_fraction - 0.95).abs() < 1e-9,
        "Revenge should reduce counter-fire crit damage vs Venari Ral at round 50"
    );
    let vs_fed = hostile_crit_damage_reduction_active_at_round(
        &crew,
        &ctx_for(OpponentFactionTag::Federation),
        50,
    );
    assert_eq!(
        vs_fed.multiplicative_fraction, 0.0,
        "Revenge must stay inert against non-Venari-Ral defenders"
    );
}

/// The rating → reduction conversion, pinned against the official worked example:
/// a Critical Mitigation of 83,000 reduces incoming critical damage by 62.41%
/// (83,000 / 133,000 = 0.62406…, Remote Campus Critical Mitigation guide).
#[test]
fn crit_mitigation_rating_conversion_matches_official_example() {
    let ability = ShipAbility {
        id: "synthetic".to_string(),
        timing: "combat_begin".to_string(),
        effect_type: "crit_mitigation_rating".to_string(),
        value: 83_000.0,
        duration_rounds: None,
        condition_morale: false,
        condition_defender_burning: false,
        condition_defender_hull_breach: false,
        condition_opponent_faction: None,
        condition_opponent_ship_class: None,
        condition_opponent_ship_class_not: None,
        condition_defender_is_npc_hostile: false,
        condition_opponent_hostile_tags: None,
        round_cap: None,
        level_scaled_values: None,
    };
    let seat = ship_ability_to_crew_seat_context(&ability)
        .expect("crit_mitigation_rating should resolve to a combat seat");
    match seat.ability.effect {
        AbilityEffect::HostileCritDamageReduction { reduction, .. } => assert!(
            (reduction - 0.6241).abs() < 1e-4,
            "rating 83,000 resolved to {reduction}; official example says 62.41%"
        ),
        ref other => panic!("expected HostileCritDamageReduction, got {other:?}"),
    }
}

#[test]
fn athena_wrath_is_combat_noop() {
    // Wave Defense vs Academy Drones is outside the simulated scenarios; the row must not
    // produce a combat seat (it previously leaked an ungated 15M apex barrier).
    let ability = athena_ability(15, 75, ATHENA_WRATH);
    assert_eq!(ability.effect_type, "combat_noop");
    assert!(
        ship_ability_to_crew_seat_context(&ability).is_none(),
        "Athena's Wrath must stay a combat noop"
    );
}

fn ctx_for(faction: OpponentFactionTag) -> CombatContext {
    CombatContext {
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
    }
}

#[test]
fn athena_fury_condition_fires_only_vs_venari_ral() {
    let cond = athena_fury_seat()
        .ability
        .condition
        .expect("Athena's Fury carries a faction condition");

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
