//! U.S.S. Vengeance Advanced Sabotage: 100% shield-mitigation bypass vs Breen Warships only.
//!
//! Backlog item 9 (docs/COMBAT_FIDELITY_BACKLOG.md): "ignores {0:#.#%} of Breen Warship [BRN]
//! shields" is a shield-mitigation bypass (Harrison Sabotage precedent), not pierce — bypass
//! helps only against a shielded target, pierce would also help against a shieldless one.

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode, WeaponStats,
    HOSTILE_TAG_MASK_BREEN_WARSHIP,
};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship::ShipAbility;
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, DefenderOpponent,
};
use std::sync::Arc;

#[test]
fn breen_warship_hostiles_carry_breen_warship_tag() {
    // Breen Warship L73 (loca 80600); the other seven family members share the tag.
    let rec = resolve_hostile("1114493593").expect("breen warship 73");
    assert_eq!(rec.loca_id, Some(80600));
    assert!(
        rec.hostile_tags.iter().any(|t| t == "breen_warship"),
        "1114493593 (Breen Warship 73) should carry breen_warship tag, got {:?}",
        rec.hostile_tags
    );
    assert_ne!(rec.hostile_tag_mask() & HOSTILE_TAG_MASK_BREEN_WARSHIP, 0);
}

fn vengeance_advanced_sabotage() -> ShipAbility {
    let rec = resolve_ship_with_tier_level("uss_vengeance", None, None).expect("vengeance record");
    rec.abilities
        .as_ref()
        .and_then(|abs| abs.iter().find(|a| a.id == "2432056626"))
        .expect("Advanced Sabotage ability")
        .clone()
}

#[test]
fn advanced_sabotage_is_full_shield_mitigation_bypass_gated_on_breen() {
    let ability = vengeance_advanced_sabotage();
    assert_eq!(ability.effect_type, "shield_mitigation_bypass");
    assert!(
        (ability.value - 1.0).abs() < 1e-12,
        "upstream value 1 with a {{0:#.#%}} placeholder is a 100% bypass fraction, got {}",
        ability.value
    );
    assert!(ability
        .condition_opponent_hostile_tags
        .as_ref()
        .is_some_and(|t| t == &["breen_warship".to_string()]));

    let seats = ship_abilities_to_crew_seat_contexts(std::slice::from_ref(&ability));
    assert_eq!(seats.len(), 1, "Advanced Sabotage should compile to a seat");
    assert!(
        matches!(
            seats[0].ability.effect,
            kobayashi::combat::AbilityEffect::ShieldMitigationBypassFraction(v) if (v - 1.0).abs() < 1e-12
        ),
        "expected ShieldMitigationBypassFraction(1.0), got {:?}",
        seats[0].ability.effect
    );
}

fn attacker() -> Combatant {
    Combatant {
        id: "attacker".into(),
        attack: 1_000.0,
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
        hull_health: 1_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 1_000.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn shielded_defender() -> Combatant {
    Combatant {
        id: "defender".into(),
        attack: 10.0,
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
        hull_health: 1_000_000.0,
        shield_health: 500_000.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn config(defender_hostile_tag_mask: u32) -> SimulationConfig {
    SimulationConfig {
        rounds: 5,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask,
        attacker_owner_faction: kobayashi::combat::OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    }
}

fn sabotage_crew() -> CrewConfiguration {
    let seats =
        ship_abilities_to_crew_seat_contexts(std::slice::from_ref(&vengeance_advanced_sabotage()));
    CrewConfiguration { seats }
}

#[test]
fn bypass_increases_hull_damage_vs_shielded_breen_warship() {
    let cfg = config(HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let plain = simulate_combat(
        &attacker(),
        &shielded_defender(),
        &cfg,
        &CrewConfiguration::default(),
    );
    let bypassed = simulate_combat(&attacker(), &shielded_defender(), &cfg, &sabotage_crew());
    assert!(
        bypassed.defender_hull_remaining < plain.defender_hull_remaining,
        "100% shield-mitigation bypass should route damage past the shields into the hull; \
         plain hull={} bypassed hull={}",
        plain.defender_hull_remaining,
        bypassed.defender_hull_remaining
    );
}

#[test]
fn bypass_is_inert_vs_shieldless_target_unlike_pierce() {
    let cfg = config(HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let shieldless = Combatant {
        shield_health: 0.0,
        ..shielded_defender()
    };
    let plain = simulate_combat(
        &attacker(),
        &shieldless,
        &cfg,
        &CrewConfiguration::default(),
    );
    let bypassed = simulate_combat(&attacker(), &shieldless, &cfg, &sabotage_crew());
    assert_eq!(
        plain.defender_hull_remaining, bypassed.defender_hull_remaining,
        "bypass must not change damage against a shieldless target (pierce would)"
    );
    assert_eq!(plain.total_damage, bypassed.total_damage);
}

#[test]
fn bypass_does_not_fire_vs_non_breen_hostiles() {
    let cfg = config(0);
    let plain = simulate_combat(
        &attacker(),
        &shielded_defender(),
        &cfg,
        &CrewConfiguration::default(),
    );
    let gated = simulate_combat(&attacker(), &shielded_defender(), &cfg, &sabotage_crew());
    assert_eq!(
        plain.defender_hull_remaining, gated.defender_hull_remaining,
        "breen_warship-gated bypass must be inert vs an untagged defender"
    );
    assert_eq!(
        plain.defender_shield_remaining,
        gated.defender_shield_remaining
    );
}

#[test]
fn vengeance_vs_breen_warship_resolves_end_to_end() {
    let registry = Arc::new(DataRegistry::load().expect("registry"));
    let candidate = CrewCandidate {
        captain: String::new(),
        bridge: vec![],
        below_decks: vec![],
    };
    let result = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "uss_vengeance",
        "1114493593",
        None,
        None,
        &candidate,
        0,
        0,
        None,
        500_000,
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !result.using_placeholder_combatants,
        "uss_vengeance vs Breen Warship 73 should resolve from the registry"
    );
}
