//! Plausible Deniability (S31-era hostiles, e.g. ability 932011628 on hostile 2116861042):
//! "Recovers {0:#.#%} of total SHP for the first 5 rounds of combat." The catalog resolves it
//! to a defender `ShieldRegenMaxFraction` seat at round end, gated to rounds 1..=5 via
//! `round_cap` → `AbilityCondition::RoundRange`.

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityCondition, AbilityEffect,
    Combatant, CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, TimingWindow,
    TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
    HostileAbilityCatalog,
};
use kobayashi::data::loader::resolve_hostile;

fn combatant(
    id: &str,
    hull: f64,
    shield: f64,
    shield_mitigation: f64,
    weapon: WeaponStats,
) -> Combatant {
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
        shield_health: shield,
        shield_mitigation,
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
    hostile_abilities_to_defender_crew(abilities, Some(&noop))
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

/// Hostile 2116861042 carries Plausible Deniability (932011628, value 0.2): a round-end
/// max-fraction shield regen seat gated to rounds 1..=5.
#[test]
fn plausible_deniability_resolves_round_end_max_fraction_seat_with_round_range() {
    let rec = resolve_hostile("2116861042").expect("plausible deniability hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    let seat = crew
        .seats
        .iter()
        .find(|s| matches!(s.ability.effect, AbilityEffect::ShieldRegenMaxFraction(_)))
        .expect("expected a ShieldRegenMaxFraction seat from Plausible Deniability");
    match seat.ability.effect {
        AbilityEffect::ShieldRegenMaxFraction(f) => {
            assert!(
                (f - 0.2).abs() < 1e-9,
                "fraction of max SHP per round, got {f}"
            )
        }
        _ => unreachable!(),
    }
    assert_eq!(seat.ability.timing, TimingWindow::RoundEnd);
    assert_eq!(
        seat.ability.condition,
        Some(AbilityCondition::RoundRange { min: 1, max: 5 })
    );
}

/// With a constant per-round shield hit `d` smaller than the 20%-of-max regen, shields refill
/// to full at the end of each of rounds 1..=5 and only decay from round 6 on:
/// final SHP after 8 rounds = max − 3·d (baseline without the seat: max − 8·d).
#[test]
fn shields_refill_during_the_first_five_rounds_and_stop_at_round_six() {
    let rec = resolve_hostile("2116861042").expect("plausible deniability hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);

    let max_shp = 40_000.0;
    // shield_mitigation 1.0 routes all weapon damage to shields; the hull never drops, so the
    // fight runs the full configured budget and we can read the final shield pool directly.
    let attacker = combatant(
        "att",
        1_000_000.0,
        0.0,
        0.0,
        WeaponStats {
            attack: 3_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "2116861042",
        1_000_000.0,
        max_shp,
        1.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    let cfg8 = pve_config(8, 42);
    let baseline = run(
        &attacker,
        &defender,
        &cfg8,
        &empty_catalog_crew(&rec.ability),
    );
    // Derive the actual per-round shield hit from the baseline instead of re-deriving the
    // damage pipeline: 8 identical deterministic rounds (no crits, no procs).
    let d = (max_shp - baseline.defender_shield_remaining) / 8.0;
    assert!(d > 0.0, "attacker must deplete shields each round");
    assert!(
        d < 0.2 * max_shp,
        "per-round hit must stay below the regen so rounds 1..=5 refill to full (d = {d})"
    );

    let regen = run(&attacker, &defender, &cfg8, &crew);
    let expected = max_shp - 3.0 * d;
    assert!(
        (regen.defender_shield_remaining - expected).abs() < 1e-6,
        "rounds 6..=8 must be the only net shield loss: got {}, expected {expected}",
        regen.defender_shield_remaining
    );

    // Stopping at exactly round 5: a 5-round fight ends with full shields.
    let cfg5 = pve_config(5, 42);
    let regen5 = run(&attacker, &defender, &cfg5, &crew);
    assert!(
        (regen5.defender_shield_remaining - max_shp).abs() < 1e-6,
        "round-end regen must refill to full through round 5: got {}",
        regen5.defender_shield_remaining
    );
}
