//! Breen Warship "Energy-Dampening Field":
//! 100% of incoming damage routes to the hostile's shields (overflow spills to hull) and the
//! hostile regenerates 25% of max shield HP at the start of each round. Per the ability text
//! ("This cannot be altered by officers, Forbidden Tech, etc.") officer/FT shield-bypass does
//! not punch through; the tag-gated U.S.S. Vengeance Advanced Sabotage hull ability — the
//! designed counter — does.

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode, WeaponStats, HOSTILE_TAG_MASK_BREEN_WARSHIP,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::loader::{resolve_hostile, resolve_ship_with_tier_level};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

fn attacker(attack_per_round: f64) -> Combatant {
    Combatant {
        id: "attacker".into(),
        attack: attack_per_round,
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
            attack: attack_per_round,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

/// Breen-shaped defender: 1M shields / 1M hull, harmless weapons, normal 80% shield mitigation
/// (so the non-routed baseline still leaks 20% of damage to hull).
fn breen_defender() -> Combatant {
    Combatant {
        id: "breen".into(),
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
        hull_health: 1_000_000.0,
        shield_health: 1_000_000.0,
        shield_mitigation: 0.8,
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
    }
}

fn config(rounds: u32, defender_hostile_tag_mask: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    }
}

/// Defender crew carrying only the Energy-Dampening Field routing seat.
fn routing_crew(regen_max_fraction: f64) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "Energy-Dampening Field".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileShieldDamageRouting { regen_max_fraction },
                condition: None,
                weapon_scope: Default::default(),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    }
}

fn run(
    attacker: &Combatant,
    defender: &Combatant,
    cfg: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_crew: &CrewConfiguration,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        cfg,
        attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        defender_crew,
    )
}

#[test]
fn breen_warship_record_resolves_shield_damage_routing_seat() {
    // Breen Warship L73 (loca 80600); the whole 8-hostile family shares ability 3780549486.
    let rec = resolve_hostile("1114493593").expect("breen warship 73");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileShieldDamageRouting { regen_max_fraction }
                if (regen_max_fraction - 0.25).abs() < 1e-12
        )),
        "expected a HostileShieldDamageRouting(0.25) seat from Energy-Dampening Field, got {:?}",
        crew.seats
            .iter()
            .map(|s| &s.ability.effect)
            .collect::<Vec<_>>()
    );
}

#[test]
fn sustained_dps_below_regen_break_even_never_touches_hull_and_times_out() {
    // 100k/round vs a 1M shield pool healing 250k/round: the shields never break, the hull is
    // never touched, and the fight is a timeout loss at the round cap.
    let att = attacker(100_000.0);
    let def = breen_defender();
    let cfg = config(20, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(
        &att,
        &def,
        &cfg,
        &CrewConfiguration::default(),
        &routing_crew(0.25),
    );
    assert_eq!(
        result.defender_hull_remaining, def.hull_health,
        "chip damage below the regen break-even must never reach the hull"
    );
    assert!(!result.attacker_won, "timeout must be a loss");
    assert!(result.winner_by_round_limit);
}

#[test]
fn burst_dps_above_break_even_breaks_through_and_kills() {
    // 2M in one round vs 1M shields: 1M absorbed, 1M overflow spills to the hull and kills.
    let att = attacker(2_000_000.0);
    let def = breen_defender();
    let cfg = config(20, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(
        &att,
        &def,
        &cfg,
        &CrewConfiguration::default(),
        &routing_crew(0.25),
    );
    assert!(
        result.attacker_won,
        "burst damage above the shield pool + regen must break through (hull remaining {})",
        result.defender_hull_remaining
    );
}

#[test]
fn without_routing_the_same_chip_damage_reaches_hull() {
    // Control for the sustained-DPS test: with no routing seat, the normal 80/20 mitigation
    // split leaks hull damage every round.
    let att = attacker(100_000.0);
    let def = breen_defender();
    let cfg = config(20, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(
        &att,
        &def,
        &cfg,
        &CrewConfiguration::default(),
        &CrewConfiguration::default(),
    );
    assert!(
        result.defender_hull_remaining < def.hull_health,
        "without routing, (1 - shield_mitigation) of each hit reaches the hull"
    );
}

#[test]
fn officer_shield_bypass_cannot_punch_through_routing() {
    // A Harrison-Sabotage-shaped officer bridge seat with a 100% bypass: per the ability text
    // ("cannot be altered by officers, Forbidden Tech, etc.") it must NOT reach the hull while
    // the routed shields are up.
    let officer_bypass_crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "Sabotage".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::ShieldMitigationBypassFraction(1.0),
                condition: None,
                weapon_scope: Default::default(),
            },
            boosted: false,
            officer_id: Some("harrison".into()),
            contribution_batch: 0,
        }],
    };
    let att = attacker(100_000.0);
    let def = breen_defender();
    let cfg = config(10, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(&att, &def, &cfg, &officer_bypass_crew, &routing_crew(0.25));
    assert_eq!(
        result.defender_hull_remaining, def.hull_health,
        "officer shield-bypass must not alter the Energy-Dampening Field routing"
    );
    assert!(!result.attacker_won);
}

#[test]
fn vengeance_tag_gated_hull_bypass_punches_through_routing() {
    // The designed counter: Advanced Sabotage (CrewSeat::Ship, gated on the breen_warship tag)
    // ignores the Breen shields entirely, so damage lands on the hull despite the routing.
    let rec = resolve_ship_with_tier_level("uss_vengeance", None, None).expect("vengeance record");
    let sabotage = rec
        .abilities
        .as_ref()
        .and_then(|abs| abs.iter().find(|a| a.id == "2432056626"))
        .expect("Advanced Sabotage ability")
        .clone();
    let vengeance_crew = CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(std::slice::from_ref(&sabotage)),
    };
    let att = attacker(100_000.0);
    let def = breen_defender();
    let cfg = config(10, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(&att, &def, &cfg, &vengeance_crew, &routing_crew(0.25));
    assert!(
        result.defender_hull_remaining < def.hull_health,
        "the tag-gated Vengeance hull bypass is the designed counter and must reach the hull"
    );
    // With a 100% bypass every shot goes straight to hull: 100k × 10 rounds.
    let expected_hull = def.hull_health - 100_000.0 * 10.0;
    assert!(
        (result.defender_hull_remaining - expected_hull).abs() < 1.0,
        "expected all weapon damage on the hull (expected {expected_hull}, got {})",
        result.defender_hull_remaining
    );
}

#[test]
fn vengeance_bypass_stays_inert_vs_routing_without_breen_tag() {
    // Same crews, but the defender is NOT tagged breen_warship: the tag-gated bypass fails its
    // condition, so the routing absorbs everything again (hypothetical guard — in the shipped
    // catalog only Breen Warships carry the routing seat).
    let rec = resolve_ship_with_tier_level("uss_vengeance", None, None).expect("vengeance record");
    let sabotage = rec
        .abilities
        .as_ref()
        .and_then(|abs| abs.iter().find(|a| a.id == "2432056626"))
        .expect("Advanced Sabotage ability")
        .clone();
    let vengeance_crew = CrewConfiguration {
        seats: ship_abilities_to_crew_seat_contexts(std::slice::from_ref(&sabotage)),
    };
    let att = attacker(100_000.0);
    let def = breen_defender();
    let cfg = config(10, 0);
    let result = run(&att, &def, &cfg, &vengeance_crew, &routing_crew(0.25));
    assert_eq!(
        result.defender_hull_remaining, def.hull_health,
        "with the condition unmet the counter bypass must not fold into the routing override"
    );
}

#[test]
fn shield_regen_tops_up_a_quarter_of_max_each_round() {
    // 400k/round vs 1M shields regenerating 250k/round: net -150k/round after the first hit.
    // Round-by-round shield HP after fire: r1 600k, r2 450k, r3 300k, r4 150k, r5 0 (exactly
    // drained, zero overflow). Stop at round 4 and check the intermediate value.
    let att = attacker(400_000.0);
    let def = breen_defender();
    let cfg = config(4, HOSTILE_TAG_MASK_BREEN_WARSHIP);
    let result = run(
        &att,
        &def,
        &cfg,
        &CrewConfiguration::default(),
        &routing_crew(0.25),
    );
    assert!(
        (result.defender_shield_remaining - 150_000.0).abs() < 1.0,
        "expected 150k shields after 4 rounds of (regen 250k, fire 400k), got {}",
        result.defender_shield_remaining
    );
    assert_eq!(result.defender_hull_remaining, def.hull_health);
}
