//! Q Junior's Twist (Q Trials Borg): ability 755115993 ("defeat the Borg Polygon within 20
//! rounds", loca 73055) caps the engagement at 20 combat rounds. A hostile still alive at the
//! cap is a timeout — always a loss for the attacker (DESIGN.md §4.4), which matches the trial
//! failing when the target is not destroyed within the limit. The 1v1 variant (1104294321,
//! loca 73051) carries no modelable single-ship clause and resolves to no seat.

use kobayashi::combat::{
    hostile_engagement_round_limit, simulate_combat_with_defender_faction_and_defender_crew,
    AbilityEffect, Combatant, CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig,
    TraceMode, WeaponStats,
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

/// Hostile 1063917329 (Q Trials Borg) carries Q Junior's Twist 755115993 — the "within 20
/// rounds" variant — and nothing else modeled.
#[test]
fn q_junior_twist_resolves_a_twenty_round_engagement_limit_seat() {
    let rec = resolve_hostile("1063917329").expect("q trials borg hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileEngagementRoundLimit { rounds: 20 }
        )),
        "expected a 20-round engagement-limit seat from Q Junior's Twist"
    );
    assert_eq!(hostile_engagement_round_limit(&crew), Some(20));
}

/// Hostile 1098304183 carries the 1v1 variant (1104294321) whose text has no round limit —
/// it must resolve to no engagement-limit seat.
#[test]
fn one_v_one_variant_resolves_no_engagement_limit_seat() {
    let rec = resolve_hostile("1098304183").expect("q trials borg 1v1 hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);
    assert_eq!(hostile_engagement_round_limit(&crew), None);
}

/// An attacker that would destroy the hostile after round 20 now times out at round 20 and
/// loses; without the seat the same fight is a win past round 20.
#[test]
fn win_after_the_limit_becomes_a_round_twenty_timeout_loss() {
    let rec = resolve_hostile("1063917329").expect("q trials borg hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    // 10k flat damage per round into a 300k hull: kill lands on round 30.
    let attacker = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1063917329",
        300_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(100, 42);

    let baseline = run(
        &attacker,
        &defender,
        &cfg,
        &empty_catalog_crew(&rec.ability),
    );
    assert!(
        baseline.attacker_won,
        "baseline (no engagement seat) must be a win"
    );
    assert!(
        baseline.rounds_simulated > 20,
        "baseline kill must land after round 20 (got {})",
        baseline.rounds_simulated
    );

    let limited = run(&attacker, &defender, &cfg, &crew);
    assert!(!limited.attacker_won, "timeout at the limit must be a loss");
    assert_eq!(limited.rounds_simulated, 20);
    assert!(limited.winner_by_round_limit);
    assert!(
        limited.defender_hull_remaining > 0.0,
        "hostile must still be alive at the cap"
    );
}

/// A kill inside the limit is unaffected: same outcome and same round count as the baseline.
#[test]
fn win_before_the_limit_is_unaffected() {
    let rec = resolve_hostile("1063917329").expect("q trials borg hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    // 30k per round into 300k hull: kill lands on round 10, inside the 20-round limit.
    let attacker = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 30_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1063917329",
        300_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(100, 42);

    let baseline = run(
        &attacker,
        &defender,
        &cfg,
        &empty_catalog_crew(&rec.ability),
    );
    let limited = run(&attacker, &defender, &cfg, &crew);

    assert!(baseline.attacker_won);
    assert!(baseline.rounds_simulated < 20);
    assert!(limited.attacker_won);
    assert_eq!(limited.rounds_simulated, baseline.rounds_simulated);
    assert!(!limited.winner_by_round_limit);
    assert_eq!(limited.total_damage, baseline.total_damage);
}

/// The engagement limit never *extends* a fight: a config round budget tighter than the
/// hostile limit still wins.
#[test]
fn config_round_budget_tighter_than_the_limit_still_applies() {
    let rec = resolve_hostile("1063917329").expect("q trials borg hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog, rec.level);

    let attacker = combatant(
        "att",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1063917329",
        300_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(15, 42);

    let limited = run(&attacker, &defender, &cfg, &crew);
    assert!(!limited.attacker_won);
    assert_eq!(limited.rounds_simulated, 15);
    assert!(limited.winner_by_round_limit);
}
