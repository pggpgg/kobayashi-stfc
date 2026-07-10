//! Integration coverage for hostile-ability fidelity mechanics added 2026-07:
//! combat-start burning applied to the player (Persistence Hunter), round-capped
//! crit buffs (Ruthless Pursuit via `round_cap` → RoundRange), the counter-fire
//! pierce percentage multiplier (Pen of Kahless), faction-gated lethal strikes
//! (Tal Shiar / Mo'Kai / S31 Elite, Q Almost Omnipotent / Strike Down),
//! Dilithium Destabilization chance-gated combat-begin instant kill,
//! Xindi per-hit stacking counter buffs (Critical Breach / Rising Fire) with
//! Hole Puncher / Immolator combat-start player breach/burn companions, and
//! Intraluminary hostile self-morale at combat begin.

use kobayashi::combat::{
    hostile_crit_damage_floor_bonus_from_defender_crew,
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    CombatStateSnapshot, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext,
    DefenderOnHitGate, DefenderOnHitStat, OpponentFactionTag, ShipType, SimulationConfig,
    TimingWindow, TraceMode, WeaponStats, NO_EXPLICIT_CONTRIBUTION_BATCH,
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
    assert_eq!(
        loss.rounds_simulated, 0,
        "non-exempt hull should die instantly"
    );

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
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileAttackerShieldMitigationZero
        )),
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

/// Rising Fire (`3353377682`) + Immolator (`3687094821`) on hostile 1150472432:
/// combat-start burn opens the gate; each counter hit stacks +standard damage for 2 rounds.
#[test]
fn rising_fire_with_immolator_ramps_counter_damage() {
    let rec = resolve_hostile("1150472432").expect("rising fire carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::Burning { chance, duration_rounds }
                if (chance - 1.0).abs() < 1e-9 && duration_rounds == 100
        )),
        "expected Immolator rest-of-combat Burning seat"
    );
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::DefenderOnHitStack {
                stat: DefenderOnHitStat::WeaponDamage,
                duration_rounds: 2,
                requires: DefenderOnHitGate::AttackerBurning,
                ..
            }
        )),
        "expected Rising Fire DefenderOnHitStack seat"
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
    let defender = combatant(
        "1150472432",
        50_000_000.0,
        WeaponStats {
            attack: 100_000.0,
            shots: Some(2),
            ..Default::default()
        },
    );

    // Empty catalog: no burn, no stacks. Full crew: Immolator burn + Rising Fire stacks.
    // Over several rounds the stacked weapon channel must clearly exceed baseline weapon damage
    // (burn ticks alone are only 1% max-hull/round and cannot explain a large gap).
    let baseline = run(
        &attacker,
        &defender,
        &pve_config(6, 21),
        &empty_catalog_crew(&rec.ability),
    );
    let boosted = run(&attacker, &defender, &pve_config(6, 21), &crew);
    let baseline_loss = 50_000_000.0 - baseline.attacker_hull_remaining;
    let boosted_loss = 50_000_000.0 - boosted.attacker_hull_remaining;
    assert!(
        boosted_loss > baseline_loss * 1.5,
        "with stacks+burn, counter damage must exceed empty-catalog baseline ({baseline_loss} vs {boosted_loss})"
    );

    // Synthetic: permanent burn via Immolator-equivalent seat only vs burn+stack — stacks add damage.
    let burn_only = CrewConfiguration {
        seats: crew
            .seats
            .iter()
            .filter(|s| matches!(s.ability.effect, AbilityEffect::Burning { .. }))
            .cloned()
            .collect(),
    };
    let burn_only_loss = {
        let r = run(&attacker, &defender, &pve_config(6, 21), &burn_only);
        50_000_000.0 - r.attacker_hull_remaining
    };
    assert!(
        boosted_loss > burn_only_loss * 1.05,
        "Rising Fire stacks must add damage beyond burn alone (burn_only={burn_only_loss}, full={boosted_loss})"
    );
}

/// Critical Breach (`3358683912`) + Hole Puncher (`3503588487`) on hostile 1744721896:
/// combat-start hull breach + per-hit crit-chance stacks + 150% crit floor seat.
#[test]
fn critical_breach_with_hole_puncher_seats_and_crit_floor() {
    let rec = resolve_hostile("1744721896").expect("critical breach carrier");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HullBreach {
                chance,
                duration_rounds,
                requires_critical: false,
            } if (chance - 1.0).abs() < 1e-9 && duration_rounds == 100
        )),
        "expected Hole Puncher rest-of-combat HullBreach seat"
    );
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::DefenderOnHitStack {
                stat: DefenderOnHitStat::CritChance,
                duration_rounds: 2,
                requires: DefenderOnHitGate::AttackerHullBreach,
                ..
            }
        )),
        "expected Critical Breach DefenderOnHitStack seat"
    );
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileCritDamageFloorBonus(v) if (v - 1.5).abs() < 1e-9
        )),
        "expected Critical Breach hostile crit damage floor 1.5"
    );
    let floor = hostile_crit_damage_floor_bonus_from_defender_crew(&crew);
    assert!(
        (floor - 1.5).abs() < 1e-9,
        "scenario helper should sum crit floor to 1.5 (got {floor})"
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
    let mut defender = combatant(
        "1744721896",
        50_000_000.0,
        WeaponStats {
            attack: 80_000.0,
            shots: Some(2),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.5),
            ..Default::default()
        },
    );
    defender.crit_damage_floor = floor;

    let baseline = run(
        &attacker,
        &defender,
        &pve_config(5, 33),
        &empty_catalog_crew(&rec.ability),
    );
    let stacked = run(&attacker, &defender, &pve_config(5, 33), &crew);
    let baseline_loss = 50_000_000.0 - baseline.attacker_hull_remaining;
    let stacked_loss = 50_000_000.0 - stacked.attacker_hull_remaining;
    assert!(
        stacked_loss > baseline_loss * 1.15,
        "crit-chance stacks + floor should raise counter damage (baseline {baseline_loss}, stacked {stacked_loss})"
    );
}

/// Without player burn/breach, Rising Fire / Critical Breach seats must not change outcomes
/// (bit-identical to a fight that only lacks the stack seats — use empty catalog as baseline
/// when companions are also absent).
#[test]
fn on_hit_stacks_without_player_state_match_empty_catalog() {
    // Synthetic crew: Rising Fire seat only (no Immolator) → gate never opens.
    let stack_only = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "rising_fire_only".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::DefenderOnHitStack {
                    stat: DefenderOnHitStat::WeaponDamage,
                    per_hit: 0.15,
                    duration_rounds: 2,
                    requires: DefenderOnHitGate::AttackerBurning,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let empty = CrewConfiguration { seats: vec![] };

    let attacker = combatant(
        "att",
        5_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "def",
        5_000_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(3),
            ..Default::default()
        },
    );
    let cfg = pve_config(8, 99);
    let a = run(&attacker, &defender, &cfg, &empty);
    let b = run(&attacker, &defender, &cfg, &stack_only);
    assert!(
        (a.attacker_hull_remaining - b.attacker_hull_remaining).abs() < 1e-6
            && (a.defender_hull_remaining - b.defender_hull_remaining).abs() < 1e-6
            && a.rounds_simulated == b.rounds_simulated
            && a.attacker_won == b.attacker_won,
        "stack seat with closed gate must be bit-identical to empty crew"
    );
}

/// Dilithium Destabilization (`167520385` on hostile 1072466025): 90% chance at combat begin
/// to instantly destroy the player. Chance is upstream `values[0].chance` (0.9), not `value`.
#[test]
fn dilithium_destabilization_90_percent_instant_loss_rate_and_determinism() {
    let rec = resolve_hostile("1072466025").expect("dilithium 90% sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileLethalCombatBegin { chance }
            if (chance - 0.9).abs() < 1e-9
        )),
        "expected HostileLethalCombatBegin chance 0.9, seats={:?}",
        crew.seats
            .iter()
            .map(|s| &s.ability.effect)
            .collect::<Vec<_>>()
    );

    let attacker = combatant(
        "player",
        500_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1072466025",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    const N: u64 = 5_000;
    let mut instant_losses = 0usize;
    for seed in 0..N {
        let r = run(&attacker, &defender, &pve_config(5, seed), &crew);
        if !r.attacker_won && r.rounds_simulated == 0 {
            instant_losses += 1;
        }
    }
    let p = instant_losses as f64 / N as f64;
    assert!(
        (p - 0.9).abs() < 0.03,
        "empirical instant-loss rate {p} should ≈ 0.9 over {N} seeds (got {instant_losses})"
    );

    let a = run(&attacker, &defender, &pve_config(5, 12345), &crew);
    let b = run(&attacker, &defender, &pve_config(5, 12345), &crew);
    assert_eq!(a.attacker_won, b.attacker_won);
    assert_eq!(a.rounds_simulated, b.rounds_simulated);
    assert_eq!(a.attacker_hull_remaining, b.attacker_hull_remaining);
}

/// Dilithium Destabilization variant (`3566779117` on hostile 1527858129): 30% chance.
#[test]
fn dilithium_destabilization_30_percent_instant_loss_rate() {
    let rec = resolve_hostile("1527858129").expect("dilithium 30% sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileLethalCombatBegin { chance }
            if (chance - 0.3).abs() < 1e-9
        )),
        "expected HostileLethalCombatBegin chance 0.3"
    );

    let attacker = combatant(
        "player",
        500_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1527858129",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );

    const N: u64 = 5_000;
    let mut instant_losses = 0usize;
    for seed in 0..N {
        let r = run(&attacker, &defender, &pve_config(5, seed), &crew);
        if !r.attacker_won && r.rounds_simulated == 0 {
            instant_losses += 1;
        }
    }
    let p = instant_losses as f64 / N as f64;
    assert!(
        (p - 0.3).abs() < 0.03,
        "empirical instant-loss rate {p} should ≈ 0.3 over {N} seeds (got {instant_losses})"
    );
}

/// Synthetic 0% Dilithium seat never instant-kills (short-circuit; no RNG needed for outcome).
#[test]
fn dilithium_destabilization_zero_chance_never_instant_kills() {
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "synthetic_dilithium".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileLethalCombatBegin { chance: 0.0 },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let attacker = combatant(
        "player",
        500_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "hostile",
        1_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    for seed in 0..200 {
        let r = run(&attacker, &defender, &pve_config(5, seed), &crew);
        assert!(
            r.rounds_simulated > 0,
            "0% chance must never instant-kill (seed {seed}): {r:?}"
        );
    }
}

/// Intraluminary (`4021963607` on Assimilated Coryn-class Explorers, sample `1295067482`):
/// combat-begin self-morale for the rest of combat. Asserts seat resolution + the snapshot flag
/// staying up through round 20 (counter-pierce impact is covered by the synthetic test below).
#[test]
fn intraluminary_self_morale_on_carrier_hostile() {
    let rec = resolve_hostile("1295067482").expect("intraluminary sample");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileSelfMorale {
                duration_rounds: 100
            }
        )),
        "expected HostileSelfMorale duration 100, seats={:?}",
        crew.seats
            .iter()
            .map(|s| &s.ability.effect)
            .collect::<Vec<_>>()
    );
    assert!(
        empty_catalog_crew(&rec.ability)
            .seats
            .iter()
            .all(|s| { !matches!(s.ability.effect, AbilityEffect::HostileSelfMorale { .. }) }),
        "empty catalog must not resolve Intraluminary"
    );

    let attacker = combatant(
        "player",
        5_000_000.0,
        WeaponStats {
            attack: 1_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let defender = combatant(
        "1295067482",
        5_000_000.0,
        WeaponStats {
            attack: 10_000.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    let mut cfg = pve_config(25, 42);
    cfg.trace_mode = TraceMode::Events;
    cfg.emit_state_snapshots = true;

    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &crew,
    );
    assert!(result.rounds_simulated >= 20);

    let round20_morale = result.events.iter().find_map(|ev| {
        if ev.event_type != "state_snapshot" || ev.round_index != 20 {
            return None;
        }
        let snap: CombatStateSnapshot =
            serde_json::from_value(ev.values.get("snapshot")?.clone()).ok()?;
        Some(snap.flags.defender_morale_active)
    });
    assert_eq!(
        round20_morale,
        Some(true),
        "defender_morale_active should stay true at round 20 with duration 100"
    );

    let baseline = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &empty_catalog_crew(&rec.ability),
    );
    let baseline_morale = baseline.events.iter().find_map(|ev| {
        if ev.event_type != "state_snapshot" || ev.round_index != 20 {
            return None;
        }
        let snap: CombatStateSnapshot =
            serde_json::from_value(ev.values.get("snapshot")?.clone()).ok()?;
        Some(snap.flags.defender_morale_active)
    });
    assert_eq!(
        baseline_morale,
        Some(false),
        "empty-catalog baseline must not have defender morale"
    );
}

/// Synthetic Battleship defender with HostileSelfMorale: +10% counter pierce (any hull class
/// qualifies) increases damage taken by the player vs an empty-seat baseline.
#[test]
fn intraluminary_self_morale_boosts_battleship_counter_pierce() {
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: "synthetic_intraluminary".into(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::HostileSelfMorale {
                    duration_rounds: 100,
                },
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    };
    let empty = CrewConfiguration { seats: vec![] };

    // High player mitigation so pierce matters; no crits for a clean delta.
    let mut attacker = combatant(
        "player",
        10_000_000.0,
        WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        },
    );
    attacker.mitigation = 0.8;
    attacker.armor = 100_000.0;
    attacker.shield_deflection = 100_000.0;
    attacker.dodge = 100_000.0;

    let mut defender = combatant(
        "hostile_bb",
        10_000_000.0,
        WeaponStats {
            attack: 50_000.0,
            shots: Some(3),
            pierce: Some(0.5),
            crit_chance: Some(0.0),
            ..Default::default()
        },
    );
    defender.pierce = 0.5;

    let cfg = pve_config(5, 7);
    let with_morale = run(&attacker, &defender, &cfg, &crew);
    let without = run(&attacker, &defender, &cfg, &empty);
    assert!(
        with_morale.attacker_hull_remaining < without.attacker_hull_remaining - 1.0,
        "BB defender morale should increase counter damage via pierce: with={} without={}",
        with_morale.attacker_hull_remaining,
        without.attacker_hull_remaining
    );
}
