//! Integration coverage for hostile-ability fidelity mechanics added 2026-07:
//! combat-start burning applied to the player (Persistence Hunter), round-capped
//! crit buffs (Ruthless Pursuit via `round_cap` → RoundRange), the counter-fire
//! pierce percentage multiplier (Pen of Kahless), and faction-gated lethal strikes
//! (Tal Shiar / Mo'Kai / S31 Elite, Q Almost Omnipotent / Strike Down).

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::types::{OpponentFactionTag, ShipType};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
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
    hostile_abilities_to_defender_crew(abilities, Some(&noop))
}

#[allow(clippy::too_many_arguments)]
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

/// Hostile 481341631 (Hunter-family, level 63) carries Persistence Hunter (986116981,
/// "Applies Burning for 6 rounds at the start of combat") and Ruthless Pursuit (390948510).
#[test]
fn persistence_hunter_burns_the_player_for_six_rounds() {
    let rec = resolve_hostile("481341631").expect("hunter hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(
            |s| matches!(s.ability.effect, AbilityEffect::Burning { chance, duration_rounds }
                if (chance - 1.0).abs() < 1e-9 && duration_rounds == 6)
        ),
        "expected a 100% / 6-round Burning seat from Persistence Hunter"
    );

    // Defender deals no weapon damage; the only player hull loss is the burn tick
    // (1% of max hull per round × 6 rounds = 6%).
    let attacker = combatant(
        "att",
        500_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "481341631",
        1_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let cfg = pve_config(8, 42);

    let baseline = run(
        &attacker,
        &defender,
        &cfg,
        &empty_catalog_crew(&rec.ability),
    );
    assert!(
        (baseline.attacker_hull_remaining - 500_000.0).abs() < 1e-6,
        "without abilities the attacker should be untouched (got {})",
        baseline.attacker_hull_remaining
    );

    let burned = run(&attacker, &defender, &cfg, &crew);
    let expected = 500_000.0 - 6.0 * 0.01 * 500_000.0;
    assert!(
        (burned.attacker_hull_remaining - expected).abs() < 1.0,
        "expected exactly six 1%-of-max-hull burn ticks (expected {expected}, got {})",
        burned.attacker_hull_remaining
    );
}

/// Ruthless Pursuit: +100% crit chance gated to combat rounds 1..=4 (`round_cap` →
/// RoundRange), plus a constant +350% crit damage seat. Round 5 counter-fire must
/// drop back to non-crit damage.
#[test]
fn ruthless_pursuit_crit_chance_expires_after_round_four() {
    let rec = resolve_hostile("481341631").expect("hunter hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);

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
        "481341631",
        50_000_000.0,
        WeaponStats {
            attack: 100_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    let d4 = {
        let cfg = pve_config(4, 7);
        let r = run(&attacker, &defender, &cfg, &crew);
        10_000_000.0 - r.attacker_hull_remaining
    };
    let d5 = {
        let cfg = pve_config(5, 7);
        let r = run(&attacker, &defender, &cfg, &crew);
        10_000_000.0 - r.attacker_hull_remaining
    };

    // Rounds 1-4 counter hits are guaranteed crits at ×4.5 (plus the burn tick);
    // round 5 falls back to non-crit weapon damage (plus the still-active burn tick).
    let round5_damage = d5 - d4;
    let earlier_average = d4 / 4.0;
    assert!(
        round5_damage < earlier_average * 0.6,
        "round 5 should not crit anymore: round5={round5_damage}, rounds1-4 avg={earlier_average}"
    );
    assert!(
        d4 > 4.0 * 100_000.0 * 3.0,
        "rounds 1-4 should be boosted by guaranteed ×4.5 crits (got total {d4})"
    );
}

/// Hostile 2024832621 carries Pen of Kahless (3363560125): +75% to shield piercing,
/// armor piercing, and accuracy for the first 5 rounds — modeled as a counter-fire
/// pierce multiplier, which must increase damage against an armored attacker.
#[test]
fn pen_of_kahless_pierce_multiplier_increases_counter_damage() {
    let rec = resolve_hostile("2024832621").expect("pen of kahless hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileCounterPierceMultiplier { bonus } if (bonus - 0.75).abs() < 1e-9
        )),
        "expected a +75% counter-pierce seat from Pen of Kahless"
    );

    let attacker = combatant(
        "att",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    // Engine pierce is a fraction-scale pierce-through term in the additive
    // damage-through factor; +75% scales 0.2 → 0.35.
    let defender = combatant(
        "2024832621",
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
        &empty_catalog_crew(&rec.ability),
    );
    let boosted = run(&attacker, &defender, &cfg, &crew);
    let baseline_loss = 10_000_000.0 - baseline.attacker_hull_remaining;
    let boosted_loss = 10_000_000.0 - boosted.attacker_hull_remaining;
    assert!(
        boosted_loss > baseline_loss * 1.05,
        "pierce multiplier should raise counter damage (baseline {baseline_loss}, boosted {boosted_loss})"
    );
}

/// Tal Shiar Elite (`2518573064` on hostile 1107147565): only Federation or Klingon designed
/// ships may engage; others are lethally struck at combat start (`rounds_simulated == 0`).
#[test]
fn tal_shiar_elite_faction_gate_instant_loss_and_allowed_fight() {
    let rec = resolve_hostile("1107147565").expect("tal shiar elite sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileLethalUnlessAttackerFaction {
                allow_federation: true,
                allow_klingon: true,
                allow_romulan: false,
                allow_uss_vengeance: false,
            }
        )),
        "expected Tal Shiar Fed|Klingon lethal gate seat"
    );

    let mut attacker = combatant(
        "uss_enterprise",
        500_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1107147565",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    let mut wrong = pve_config(5, 42);
    wrong.attacker_owner_faction = OpponentFactionTag::Romulan;
    let loss = run(&attacker, &defender, &wrong, &crew);
    assert!(
        !loss.attacker_won && loss.rounds_simulated == 0 && loss.attacker_hull_remaining == 0.0,
        "Romulan hull should be lethally struck: {loss:?}"
    );

    let mut ok = pve_config(5, 42);
    ok.attacker_owner_faction = OpponentFactionTag::Federation;
    let fight = run(&attacker, &defender, &ok, &crew);
    assert!(
        fight.rounds_simulated > 0,
        "Federation hull should engage normally (got rounds={})",
        fight.rounds_simulated
    );

    attacker.id = "korinar".into();
    let mut klingon = pve_config(5, 42);
    klingon.attacker_owner_faction = OpponentFactionTag::Klingon;
    let fight_k = run(&attacker, &defender, &klingon, &crew);
    assert!(
        fight_k.rounds_simulated > 0,
        "Klingon hull should engage normally"
    );
}

/// Almost Omnipotent (`1206267116`): Fed/Rom/Klingon or U.S.S. Vengeance; +300% crit floor.
#[test]
fn almost_omnipotent_vengeance_exception_and_crit_floor() {
    let rec = resolve_hostile("1073900199").expect("almost omnipotent sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileLethalUnlessAttackerFaction {
                allow_federation: true,
                allow_klingon: true,
                allow_romulan: true,
                allow_uss_vengeance: true,
            }
        )),
        "expected Q faction gate with Vengeance exception"
    );
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileCritDamageFloorBonus(v) if (v - 3.0).abs() < 1e-9
        )),
        "expected hostile crit damage floor 3.0 (300%)"
    );

    let mut attacker = combatant(
        "borg_sphere",
        500_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1073900199",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    let mut wrong = pve_config(3, 9);
    wrong.attacker_owner_faction = OpponentFactionTag::Borg;
    let loss = run(&attacker, &defender, &wrong, &crew);
    assert_eq!(loss.rounds_simulated, 0, "non-exempt hull should die instantly");

    attacker.id = "uss_vengeance".into();
    let mut vengeance = pve_config(3, 9);
    vengeance.attacker_owner_faction = OpponentFactionTag::Unknown;
    let fight = run(&attacker, &defender, &vengeance, &crew);
    assert!(
        fight.rounds_simulated > 0,
        "U.S.S. Vengeance must engage despite missing faction slug"
    );
}

/// Strike Down (`1567589326`): same Q gate + SM→0% for allowed ships (more hull damage taken).
#[test]
fn strike_down_zeros_attacker_shield_mitigation() {
    let rec = resolve_hostile("1029134381").expect("strike down sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats
            .iter()
            .any(|s| matches!(s.ability.effect, AbilityEffect::HostileAttackerShieldMitigationZero)),
        "expected Strike Down shield-mitigation-zero seat"
    );

    let mut attacker = combatant(
        "uss_enterprise",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    attacker.shield_health = 5_000_000.0;
    attacker.shield_mitigation = 0.8;

    let defender = combatant(
        "1029134381",
        50_000_000.0,
        WeaponStats {
            attack: 200_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    let mut cfg = pve_config(3, 13);
    cfg.attacker_owner_faction = OpponentFactionTag::Federation;

    let baseline = run(
        &attacker,
        &defender,
        &cfg,
        &empty_catalog_crew(&rec.ability),
    );
    let struck = run(&attacker, &defender, &cfg, &crew);
    assert!(
        struck.rounds_simulated > 0,
        "Federation hull should engage Strike Down"
    );
    let baseline_loss = 10_000_000.0 - baseline.attacker_hull_remaining;
    let struck_loss = 10_000_000.0 - struck.attacker_hull_remaining;
    assert!(
        struck_loss > baseline_loss * 1.2,
        "SM→0% should increase hull damage taken (baseline {baseline_loss}, struck {struck_loss})"
    );
}
