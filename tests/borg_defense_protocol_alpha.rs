//! Q Trials Borg **Defense Protocol α** (loca 73050 / 73054, ability ids `3797564109` /
//! `750815541`, 10 hostiles): "on the second weapon of every round the Borg Type 03 / Polygon
//! fires its Cutting Beam dealing lethal damage to enemy ships."
//!
//! Modeled as `hostile_lethal_end_of_round` with `round_interval: 1` — see
//! docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md. The row stayed `combat_noop` until the lethal hook
//! learned to skip a hostile destroyed in the same round; without that gate an interval of 1 turns
//! every kill, including a legitimate round-1 kill, into a mutual-death loss.

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect, Combatant,
    CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::loader::resolve_hostile;

/// Borg Type 03.0 (level 44) and Borg Polygon carriers, one per ability id.
const Q_TRIALS_BORG: [(&str, &str); 2] =
    [("1098304183", "3797564109"), ("1366188504", "750815541")];

fn combatant(id: &str, hull: f64, attack: f64) -> Combatant {
    Combatant {
        id: id.into(),
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
        hull_health: hull,
        shield_health: 0.0,
        shield_mitigation: 0.0,
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

fn pve_config(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
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

fn run(
    attacker: &Combatant,
    defender: &Combatant,
    cfg: &SimulationConfig,
    defender_crew: &CrewConfiguration,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        cfg,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        defender_crew,
    )
}

fn defender_crew_for(hostile_id: &str) -> CrewConfiguration {
    let rec = resolve_hostile(hostile_id).expect("defense protocol carrier");
    hostile_abilities_to_defender_crew(&rec.ability, hostile_ability_catalog_for_default_path())
}

#[test]
fn defense_protocol_alpha_resolves_an_every_round_lethal_seat() {
    for (hostile_id, ability_id) in Q_TRIALS_BORG {
        let crew = defender_crew_for(hostile_id);
        assert!(
            crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::HostileLethalEndOfRound { round_interval, .. }
                    if round_interval == 1
            )),
            "{hostile_id} (ability {ability_id}): expected an interval-1 lethal seat"
        );
    }
}

#[test]
fn defense_protocol_alpha_kills_an_attacker_that_cannot_win_in_one_round() {
    for (hostile_id, _) in Q_TRIALS_BORG {
        let crew = defender_crew_for(hostile_id);
        // The real hostile's HHP is in the trillions; nothing survives round 1 against it.
        let attacker = combatant("att", 1_000_000.0, 1_000.0);
        let defender = combatant(hostile_id, 50_000_000.0, 0.0);
        let result = run(&attacker, &defender, &pve_config(12, 5), &crew);
        assert!(
            !result.attacker_won,
            "{hostile_id}: the Cutting Beam must end the fight as a loss"
        );
        assert_eq!(
            result.rounds_simulated, 1,
            "{hostile_id}: an every-round beam fires at the end of round 1"
        );
        assert_eq!(result.attacker_hull_remaining, 0.0);
    }
}

#[test]
fn defense_protocol_alpha_does_not_deny_a_round_one_kill() {
    // The gate this modeling depends on: destroy the hostile inside round 1 and the beam that was
    // scheduled for the end of that same round does not fire.
    for (hostile_id, _) in Q_TRIALS_BORG {
        let crew = defender_crew_for(hostile_id);
        let attacker = combatant("att", 1_000_000.0, 100_000_000.0);
        let defender = combatant(hostile_id, 10_000_000.0, 0.0);
        let result = run(&attacker, &defender, &pve_config(12, 5), &crew);
        assert!(
            result.attacker_won,
            "{hostile_id}: a hostile destroyed in round 1 cannot fire its round-1 beam"
        );
        assert_eq!(result.rounds_simulated, 1);
        assert!(result.attacker_hull_remaining > 0.0);
    }
}
