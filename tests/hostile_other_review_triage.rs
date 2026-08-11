//! Integration coverage for the 2026-07-19 `other_review` triage slice documented in
//! docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md: Exploitation / Pre-Assimilation ship-class-gated
//! counter-fire damage, Ravager's Lance flat pierce multipliers, and the Species 8472
//! Energy Focused Beam scheduled lethal.

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect, Combatant,
    CrewConfiguration, DefenderOnHitGate, DefenderOnHitStat, OpponentFactionTag, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
    HostileAbilityCatalog,
};
use kobayashi::data::loader::resolve_hostile;

fn combatant(id: &str, hull: f64, weapon: WeaponStats) -> Combatant {
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
        weapons: vec![weapon],
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

fn empty_catalog_crew(abilities: &[serde_json::Value]) -> CrewConfiguration {
    let noop = HostileAbilityCatalog {
        description: None,
        entries: std::collections::HashMap::new(),
    };
    hostile_abilities_to_defender_crew(abilities, Some(&noop), 1)
}

fn run(
    attacker: &Combatant,
    defender: &Combatant,
    cfg: &SimulationConfig,
    attacker_ship_type: ShipType,
    defender_crew: &CrewConfiguration,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        cfg,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        attacker_ship_type,
        true,
        false,
        defender_crew,
    )
}

/// Species 8472 Shield Disruptors (loca 55049): every hostile weapon hit pushes a −10
/// percentage-point player shield-mitigation stack for the rest of that round. Hit N earns the
/// stack after its damage is resolved, so only subsequent hits use it.
#[test]
fn shield_disruptors_reduce_player_shield_mitigation_per_hit_for_one_round() {
    let rec = resolve_hostile("1848004292").expect("shield disruptors carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let resolved = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let disruptor_seats: Vec<_> = resolved
        .seats
        .iter()
        .filter(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::DefenderOnHitStack {
                    stat: DefenderOnHitStat::ShieldMitigationReduction,
                    per_hit,
                    duration_rounds: 1,
                    requires: DefenderOnHitGate::Always,
                } if (per_hit - 0.10).abs() < 1e-9
            )
        })
        .cloned()
        .collect();
    assert_eq!(
        disruptor_seats.len(),
        1,
        "expected one real-data Shield Disruptors seat"
    );
    let disruptor_crew = CrewConfiguration {
        seats: disruptor_seats,
    };

    let mut attacker = combatant(
        "att",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    attacker.shield_health = 10_000_000.0;
    attacker.shield_mitigation = 0.80;
    let defender = combatant(
        "1848004292",
        50_000_000.0,
        WeaponStats {
            attack: 100_000.0,
            shots: Some(3),
            ..Default::default()
        },
    );
    let empty = CrewConfiguration { seats: vec![] };

    for rounds in [1, 2] {
        let cfg = pve_config(rounds, 29);
        let baseline = run(&attacker, &defender, &cfg, ShipType::Explorer, &empty);
        let disrupted = run(
            &attacker,
            &defender,
            &cfg,
            ShipType::Explorer,
            &disruptor_crew,
        );
        let baseline_hull_loss = 10_000_000.0 - baseline.attacker_hull_remaining;
        let disrupted_hull_loss = 10_000_000.0 - disrupted.attacker_hull_remaining;
        let expected_baseline = 60_000.0 * f64::from(rounds);
        let expected_disrupted = 90_000.0 * f64::from(rounds);
        assert!(
            (baseline_hull_loss - expected_baseline).abs() < 1e-6,
            "rounds={rounds}: baseline should keep 80% SM on all three hits (got {baseline_hull_loss})"
        );
        assert!(
            (disrupted_hull_loss - expected_disrupted).abs() < 1e-6,
            "rounds={rounds}: hits should use 80%, 70%, 60% SM each round (got {disrupted_hull_loss})"
        );
    }
}

/// Hostile 1362229790 carries Interceptor Exploitation (354606680): +100% damage against
/// Interceptors for the first 5 rounds ({0:#.#%} fraction, values[0] = 1).
#[test]
fn exploitation_resolves_ship_class_gated_attack_multiplier() {
    let rec = resolve_hostile("1362229790").expect("interceptor exploitation carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    let seat = crew
        .seats
        .iter()
        .find(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::ProcAttackMultiplier { chance, multiplier }
                    if (chance - 1.0).abs() < 1e-9 && (multiplier - 2.0).abs() < 1e-9
            )
        })
        .expect("expected a deterministic x2 ProcAttackMultiplier seat from Exploitation");
    assert!(
        seat.ability.condition.is_some(),
        "the Exploitation seat must be gated (attacker hull class + rounds 1..=5)"
    );
}

/// Counter-fire damage doubles for the first 5 rounds against the matching attacker hull
/// class, then reverts: over 10 rounds the matched class takes exactly 1.5x the baseline
/// hull loss. A mismatched class is bit-identical to baseline.
#[test]
fn exploitation_boosts_counter_fire_vs_matching_class_first_five_rounds_only() {
    let rec = resolve_hostile("1362229790").expect("interceptor exploitation carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    let attacker = combatant(
        "att",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1362229790",
        50_000_000.0,
        WeaponStats {
            attack: 100_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(10, 7);

    let baseline = run(
        &attacker,
        &defender,
        &cfg,
        ShipType::Interceptor,
        &empty_catalog_crew(&rec.ability),
    );
    let matched = run(&attacker, &defender, &cfg, ShipType::Interceptor, &crew);
    let mismatched = run(&attacker, &defender, &cfg, ShipType::Battleship, &crew);

    let baseline_loss = 10_000_000.0 - baseline.attacker_hull_remaining;
    let matched_loss = 10_000_000.0 - matched.attacker_hull_remaining;
    let mismatched_loss = 10_000_000.0 - mismatched.attacker_hull_remaining;

    assert!(
        (matched_loss - 1.5 * baseline_loss).abs() < 1.0,
        "x2 for rounds 1-5 of 10 => exactly 1.5x total (baseline {baseline_loss}, matched {matched_loss})"
    );
    assert!(
        (mismatched_loss - baseline_loss).abs() < 1e-6,
        "a non-Interceptor attacker must be unaffected (baseline {baseline_loss}, mismatched {mismatched_loss})"
    );
}

/// Ravager's Lance: "+500%" (hostile 434295500) and "+1500%" (hostile 1203412547) to all
/// piercing stats — counter-fire pierce multipliers from meaningful upstream values (5 / 15).
#[test]
fn ravagers_lance_resolves_both_tiers_and_boosts_counter_damage() {
    let catalog = hostile_ability_catalog_for_default_path();
    for (hostile_id, expected_bonus, min_ratio) in
        [("434295500", 5.0, 1.5), ("1203412547", 15.0, 3.0)]
    {
        let rec = resolve_hostile(hostile_id).expect("ravager's lance carrier");
        let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
        assert!(
            crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::HostileCounterPierceMultiplier { bonus }
                    if (bonus - expected_bonus).abs() < 1e-9
            )),
            "expected a +{expected_bonus}x counter-pierce seat on {hostile_id}"
        );

        let attacker = combatant(
            "att",
            50_000_000.0,
            WeaponStats {
                attack: 0.0,
                shots: Some(1),
                ..Default::default()
            },
        );
        // Pierce is a fraction-scale additive damage-through term; x(1+bonus) scales the
        // weapon's 0.2 pierce, raising counter damage well past the baseline.
        let defender = combatant(
            hostile_id,
            50_000_000.0,
            WeaponStats {
                attack: 100_000.0,
                shots: Some(1),
                pierce: Some(0.2),
                ..Default::default()
            },
        );
        let cfg = pve_config(3, 11);
        let baseline = run(
            &attacker,
            &defender,
            &cfg,
            ShipType::Explorer,
            &empty_catalog_crew(&rec.ability),
        );
        let boosted = run(&attacker, &defender, &cfg, ShipType::Explorer, &crew);
        let baseline_loss = 50_000_000.0 - baseline.attacker_hull_remaining;
        let boosted_loss = 50_000_000.0 - boosted.attacker_hull_remaining;
        assert!(
            boosted_loss > baseline_loss * min_ratio,
            "{hostile_id}: pierce x(1+{expected_bonus}) should raise counter damage by more \
             than {min_ratio}x (baseline {baseline_loss}, boosted {boosted_loss})"
        );
    }
}

/// Species 8472 Energy Focused Beam (hostile 1892438128): the beam fires at the end of
/// round 8 and destroys the attacker — an attacker that cannot kill by then loses at
/// exactly round 8; one that kills in round 1 wins untouched by the beam.
#[test]
fn energy_focused_beam_destroys_the_attacker_at_round_eight() {
    let rec = resolve_hostile("1892438128").expect("energy focused beam carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileLethalEndOfRound { round_interval, .. } if round_interval == 8
        )),
        "expected an interval-8 lethal seat from Energy Focused Beam"
    );

    // Harmless on both sides: only the beam can end the fight.
    let weak = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1892438128",
        50_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(12, 3);
    let doomed = run(&weak, &defender, &cfg, ShipType::Explorer, &crew);
    assert!(
        !doomed.attacker_won,
        "the beam must end the fight as a loss"
    );
    assert_eq!(
        doomed.rounds_simulated, 8,
        "the beam fires at the end of round 8"
    );
    assert_eq!(doomed.attacker_hull_remaining, 0.0);

    // A burst attacker kills in round 1, well before the beam charges.
    let burst = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 100_000_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let small_defender = combatant(
        "1892438128",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let victory = run(&burst, &small_defender, &cfg, ShipType::Explorer, &crew);
    assert!(victory.attacker_won, "killing before round 8 must win");
    assert_eq!(victory.rounds_simulated, 1);
}

/// The kill and the beam land in the same round. A destroyed hostile does not fire, so round 8 is
/// a win — before the defender-alive gate the beam took the attacker with it and the run scored a
/// mutual-death loss.
#[test]
fn energy_focused_beam_does_not_fire_on_the_round_it_dies() {
    let rec = resolve_hostile("1892438128").expect("energy focused beam carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    let attacker = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 1_000_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let unkillable = combatant(
        "1892438128",
        f64::MAX / 4.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    // Size the hostile so the kill lands inside round 8 rather than guessing at the damage
    // formula: measure the attacker's cumulative output through round 7 and through round 8.
    let through_seven = run(
        &attacker,
        &unkillable,
        &pve_config(7, 3),
        ShipType::Explorer,
        &crew,
    );
    let through_eight = run(
        &attacker,
        &unkillable,
        &pve_config(12, 3),
        ShipType::Explorer,
        &crew,
    );
    assert_eq!(
        through_eight.rounds_simulated, 8,
        "control: an attacker that cannot kill still dies to the beam at round 8"
    );
    assert!(
        through_eight.total_damage > through_seven.total_damage,
        "round 8 must contribute damage for this test to place a kill inside it"
    );
    let hull_dying_in_round_eight = (through_seven.total_damage + through_eight.total_damage) / 2.0;

    let defender = combatant(
        "1892438128",
        hull_dying_in_round_eight,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let result = run(
        &attacker,
        &defender,
        &pve_config(12, 3),
        ShipType::Explorer,
        &crew,
    );
    assert_eq!(
        result.rounds_simulated, 8,
        "the kill should land in the same round the beam was scheduled to fire"
    );
    assert_eq!(result.defender_hull_remaining, 0.0, "the hostile died");
    assert!(
        result.attacker_won,
        "a hostile destroyed in round 8 cannot fire its round-8 beam"
    );
    assert!(
        result.attacker_hull_remaining > 0.0,
        "the attacker must survive its own winning round"
    );
}
