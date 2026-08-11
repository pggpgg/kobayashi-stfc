//! Temporal Dreadnought phase-alignment regeneration (combat-fidelity backlog item 6).
//!
//! Charged variants restore shields and hull to full at the start of every round; Static
//! variants restore shields only while retaining their existing isolytic-damage seat. The
//! U.S.S. Relativity's always-active Anti-Charged / Anti-Static Shift hull abilities disable
//! the corresponding regeneration. Neutral variants have no phase alignment and stay inert.

use kobayashi::combat::{
    hostile_full_regen_unless_attacker_ship,
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode, WeaponStats, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::loader::resolve_hostile;

fn combatant(id: &str, hull: f64, shield: f64, shield_mitigation: f64, attack: f64) -> Combatant {
    Combatant {
        id: id.into(),
        attack,
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
        hull_health: hull,
        shield_health: shield,
        shield_mitigation,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn config(rounds: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed: 42,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
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

fn regen_crew(restore_shields: bool, restore_hull: bool) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "Temporal Dreadnought phase alignment".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileFullRegenUnlessAttackerShip {
                    restore_shields,
                    restore_hull,
                    allow_uss_relativity: true,
                },
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
    rounds: u32,
    defender_crew: &CrewConfiguration,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        &config(rounds),
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        defender_crew,
    )
}

#[test]
fn real_temporal_dreadnought_records_resolve_the_correct_regeneration() {
    let catalog = hostile_ability_catalog_for_default_path();

    // Charged L53 carries Charged Quantum Hull Repair (1004890011).
    let charged = resolve_hostile("2253944433").expect("charged temporal dreadnought");
    let charged_crew = hostile_abilities_to_defender_crew(&charged.ability, catalog, charged.level);
    assert!(charged_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileFullRegenUnlessAttackerShip {
            restore_shields: true,
            restore_hull: true,
            allow_uss_relativity: true,
        }
    )));

    // Static L63 carries Static Collider Cannon (1070977437): isolytic damage plus shield-only
    // regeneration. The existing isolytic seat must remain present after adding the extra seat.
    let static_rec = resolve_hostile("1337474913").expect("static temporal dreadnought");
    let static_crew =
        hostile_abilities_to_defender_crew(&static_rec.ability, catalog, static_rec.level);
    assert!(static_crew
        .seats
        .iter()
        .any(|s| matches!(s.ability.effect, AbilityEffect::IsolyticDamageBonus(_))));
    assert!(static_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileFullRegenUnlessAttackerShip {
            restore_shields: true,
            restore_hull: false,
            allow_uss_relativity: true,
        }
    )));

    // Neutral L53 explicitly has no phase alignment and must not resolve a regeneration seat.
    let neutral = resolve_hostile("1420655807").expect("neutral temporal dreadnought");
    let neutral_crew = hostile_abilities_to_defender_crew(&neutral.ability, catalog, neutral.level);
    assert!(neutral_crew.seats.iter().all(|s| !matches!(
        s.ability.effect,
        AbilityEffect::HostileFullRegenUnlessAttackerShip { .. }
    )));
}

#[test]
fn charged_restores_both_pools_between_rounds_for_non_relativity_attackers() {
    // A 40k hit against 50% shield mitigation deals 20k to each pool. Full regeneration at the
    // start of rounds 2 and 3 means only the final round's damage remains.
    let attacker = combatant("ordinary_ship", 1_000_000.0, 0.0, 0.0, 40_000.0);
    let defender = combatant("charged", 100_000.0, 100_000.0, 0.5, 0.0);
    let crew = regen_crew(true, true);
    assert_eq!(
        hostile_full_regen_unless_attacker_ship(&crew, &attacker.id),
        Some((true, true))
    );

    let result = run(&attacker, &defender, 3, &crew);
    assert_eq!(result.defender_shield_remaining, 80_000.0);
    assert_eq!(result.defender_hull_remaining, 80_000.0);
    assert!(!result.attacker_won);
}

#[test]
fn static_restores_shields_but_hull_damage_accumulates() {
    let attacker = combatant("ordinary_ship", 1_000_000.0, 0.0, 0.0, 40_000.0);
    let defender = combatant("static", 100_000.0, 100_000.0, 0.5, 0.0);
    let crew = regen_crew(true, false);

    let result = run(&attacker, &defender, 3, &crew);
    assert_eq!(result.defender_shield_remaining, 80_000.0);
    assert_eq!(
        result.defender_hull_remaining, 40_000.0,
        "Static text promises instant shields, not hull restoration"
    );
}

#[test]
fn relativity_disables_temporal_regeneration() {
    let relativity = combatant("uss_relativity", 1_000_000.0, 0.0, 0.0, 40_000.0);
    let defender = combatant("charged", 100_000.0, 100_000.0, 0.5, 0.0);
    let crew = regen_crew(true, true);
    assert_eq!(
        hostile_full_regen_unless_attacker_ship(&crew, &relativity.id),
        None
    );
    assert_eq!(
        hostile_full_regen_unless_attacker_ship(&crew, "442815157"),
        None,
        "the upstream numeric ship id must resolve as the Relativity counter"
    );
    assert_eq!(
        hostile_full_regen_unless_attacker_ship(&crew, "U.S.S. RELATIVITY"),
        None,
        "display-name ship lookup must resolve as the Relativity counter"
    );

    let result = run(&relativity, &defender, 3, &crew);
    assert_eq!(result.defender_shield_remaining, 40_000.0);
    assert_eq!(result.defender_hull_remaining, 40_000.0);
}

#[test]
fn non_relativity_burst_can_kill_before_the_next_regeneration() {
    // Regeneration happens at round start, so a one-round burst through both pools still wins.
    let attacker = combatant("ordinary_ship", 1_000_000.0, 0.0, 0.0, 250_000.0);
    let defender = combatant("charged", 100_000.0, 100_000.0, 0.5, 0.0);
    let result = run(&attacker, &defender, 5, &regen_crew(true, true));
    assert!(result.attacker_won);
    assert_eq!(result.rounds_simulated, 1);
    assert_eq!(result.defender_hull_remaining, 0.0);
}
