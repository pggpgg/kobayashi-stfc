//! End-to-end regression tests for Batch B (2026-11-26 Fights A–F) verified combat mechanics:
//! 1. Defensive Tactics (`rid 1914017763`, `bid 2859405133`) additive non-Armada hostile weapon
//!    damage reduction stacking with Chen (`1.00 - 0.30 - 0.66 = 0.04`).
//! 2. Ship `officer_bonus.attack` additive in the `weapon_damage` MULT pool (Realta vs Vi'Dar).
//! 3. Unified `op 6` (`MultiplyBaseSub`) `ShieldHPRepair` flat enemy shield drain (Ent-E Troi,
//!    Vemet, Six Of Eleven captain, Michael Burnham).
//! 4. Suder (`2041170794`) `RoundEnd` `HullRepair` at the round boundary.
//! 5. Annorax (`2281926825`) reactive `HitTaken` flat Apex Barrier (`num_rounds=2`, Tal gate).
//! 6. Single-axis `ArmorPiercing` pool scaling (M'Ral launch, Azetbur on-hit, Paul Stamets PvP).

use std::collections::HashMap;

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass,
    AbilityCondition, AbilityEffect, AttackerStats, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, CrewStatAxis, CrewStatScaledTarget, DamageTakenTrigger, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ReactiveDamageTakenKind, ShipType,
    SimulationConfig, TimingWindow, WeaponStats, WeaponType, WeaponTypeScope,
};
use kobayashi::data::import::GlobalBuffEntry;
use kobayashi::data::native_research_buffs::combat_start_opponent_weapon_damage_reduction_seats_for_imported_buffs;
use kobayashi::data::profile::{apply_profile_to_attacker, OfficerStatRuntimeBonus, PlayerProfile};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    LcarsOfficer, ResolveOptions,
};

fn load_officers() -> HashMap<String, LcarsOfficer> {
    index_lcars_officers_by_id(
        build_officer_model_file_default()
            .expect("default officer model")
            .officers,
    )
}

fn combatant_with_weapon(
    id: &str,
    attack: f64,
    shots: u32,
    weapon_type: WeaponType,
    hull: f64,
    shield: f64,
) -> Combatant {
    Combatant {
        id: id.to_string(),
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
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack,
            shots: Some(shots),
            pierce: Some(0.0),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.0),
            proc_chance: Some(0.0),
            proc_multiplier: Some(1.0),
            weapon_type,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

#[test]
fn defensive_tactics_stacks_additively_with_chen_to_four_percent_and_gates_armadas() {
    let imported = [GlobalBuffEntry {
        bid: 2_859_405_133,
        level: 5,
        expiry_time: None,
    }];
    let dt_seats = combat_start_opponent_weapon_damage_reduction_seats_for_imported_buffs(
        &imported,
        1_700_000_000,
    );
    assert_eq!(dt_seats.len(), 1, "Defensive Tactics L5 should emit 1 seat");

    let hull = 10_000_000.0;
    let attacker = combatant_with_weapon("vidar", 10.0, 1, WeaponType::Energy, hull, 0.0);
    let defender = combatant_with_weapon(
        "fed_explorer_50",
        451_256.5,
        1,
        WeaponType::Energy,
        hull,
        0.0,
    );
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 7,
        defender_hull_faction_id: 2_064_723_306,
        defender_level: Some(50),
        ..Default::default()
    };

    // Baseline: Defensive Tactics L5 (-30%) alone -> 0.70 * 451_256.5 = 315_879.55
    let dt_only = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration {
            seats: dt_seats.clone(),
        },
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let dt_dmg = hull - dt_only.attacker_hull_remaining;
    assert!(
        (dt_dmg - 451_256.5 * 0.70).abs() < 1e-3,
        "Defensive Tactics alone should leave 70% hostile damage, got {dt_dmg}"
    );

    // PMC: Defensive Tactics (-30%) + Pike×Chen (-66% Energy) -> 1.00 - 0.30 - 0.66 = 0.04
    let mut pmc_seats = dt_seats.clone();
    pmc_seats.push(CrewSeatContext {
        seat: CrewSeat::Bridge,
        ability: Ability {
            name: "chen_pike_boosted".to_string(),
            class: AbilityClass::BridgeAbility,
            timing: TimingWindow::CombatBegin,
            boostable: true,
            effect: AbilityEffect::OpponentWeaponDamageReduction {
                reduction: 0.66,
                cumulative: false,
            },
            condition: Some(AbilityCondition::DefenderIsNpcHostile),
            weapon_scope: WeaponTypeScope::EnergyOnly,
        },
        boosted: false,
        officer_id: Some("chen-cdb1ca".to_string()),
        contribution_batch: 0,
    });
    let pmc = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: pmc_seats },
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let pmc_dmg = hull - pmc.attacker_hull_remaining;
    assert!(
        (pmc_dmg - 451_256.5 * 0.04).abs() < 1e-3,
        "Defensive Tactics (-0.30) + Pike×Chen (-0.66) must stack additively to 0.04x (18,050.26), got {pmc_dmg}"
    );

    // Armada target: Defensive Tactics is gated on TargetNotArmada -> 100% damage.
    let vs_armada = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration {
            seats: dt_seats.clone(),
        },
        OpponentFactionTag::Federation,
        ShipType::Armada,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let armada_dmg = hull - vs_armada.attacker_hull_remaining;
    assert!(
        (armada_dmg - 451_256.5).abs() < 1e-3,
        "Defensive Tactics must not trigger vs Armadas, got {armada_dmg}"
    );

    // Player target (PvP): gated on EnemyHostile -> 100% damage.
    let vs_player = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &CrewConfiguration { seats: dt_seats },
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        false,
        true,
        &CrewConfiguration::default(),
    );
    let player_dmg = hull - vs_player.attacker_hull_remaining;
    assert!(
        (player_dmg - 451_256.5).abs() < 1e-3,
        "Defensive Tactics must not trigger in PvP, got {player_dmg}"
    );
}

#[test]
fn ship_officer_attack_bonus_shares_additive_weapon_damage_pool() {
    let profile = PlayerProfile {
        bonuses: HashMap::from([("weapon_damage".to_string(), 93.0)]),
        ..Default::default()
    };
    let realta_base = combatant_with_weapon("realta", 44.0, 1, WeaponType::Energy, 1000.0, 1000.0);
    let vidar_base =
        combatant_with_weapon("vidar", 12_941.0, 1, WeaponType::Energy, 1000.0, 1000.0);

    let realta = apply_profile_to_attacker(
        realta_base,
        &profile,
        None,
        OfficerStatRuntimeBonus {
            attack_bonus: 1.0,
            ..Default::default()
        },
    );
    let vidar = apply_profile_to_attacker(
        vidar_base,
        &profile,
        None,
        OfficerStatRuntimeBonus {
            attack_bonus: 3.5,
            ..Default::default()
        },
    );

    let realta_mult = realta.weapon_attack(0).unwrap() / 44.0;
    let vidar_mult = vidar.weapon_attack(0).unwrap() / 12_941.0;
    assert!(
        (realta_mult - 95.0).abs() < 1e-9,
        "realta mult={realta_mult}"
    );
    assert!((vidar_mult - 97.5).abs() < 1e-9, "vidar mult={vidar_mult}");
    // Ratio is 97.5/95.0 ≈ 1.0263x (matching Fight A vs C-F ~1.037x), NOT multiplicative 4.5/2.0 = 2.25x.
    assert!((vidar_mult / realta_mult - 97.5 / 95.0).abs() < 1e-9);
}

#[test]
fn annorax_reactive_flat_apex_barrier_steps_once_per_volley_for_two_rounds() {
    let officers = load_officers();
    let buff_set = resolve_crew_to_buff_set(
        "stonn-95a1f6",
        &["annorax-830d35".to_string()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(1),
            ..Default::default()
        },
    );
    assert!(
        buff_set.crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::ReactiveDamageTaken {
                trigger: DamageTakenTrigger::AnyDamage,
                kind: ReactiveDamageTakenKind::ApexBarrierFlatBonus(v),
                duration_rounds: 2,
                ..
            } if (v - 2000.0).abs() < 1e-6
        )),
        "Annorax T1 must resolve to 2-round HitTaken ApexBarrierFlatBonus(2000), seats={:?}",
        buff_set.crew.seats
    );

    // Replicate Fight D: 3 incoming 3-shot volleys per round, baseline apex_barrier = 800, Annorax +2000/volley.
    let hull = 100_000_000.0;
    let mut attacker = combatant_with_weapon("vidar", 1.0, 1, WeaponType::Energy, hull, 0.0);
    attacker.apex_barrier = 800.0;
    let mut defender =
        combatant_with_weapon("explorer", 100_000.0, 3, WeaponType::Energy, hull, 0.0);
    defender.weapons = vec![
        WeaponStats {
            attack: 100_000.0,
            shots: Some(3),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.0),
            weapon_type: WeaponType::Energy,
            ..Default::default()
        };
        3
    ];
    let cfg = SimulationConfig {
        rounds: 2,
        seed: 11,
        defender_level: Some(50),
        ..Default::default()
    };

    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &buff_set.crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );

    // Expected damage across the 6 volleys (3 shots × 100k = 300k each) with barrier:
    // R1.S1: 800, R1.S2: 2800, R1.S3: 4800, R2.S1: 6800, R2.S2: 8800, R2.S3: 10800
    let expected: f64 = [800.0, 2800.0, 4800.0, 6800.0, 8800.0, 10800.0]
        .iter()
        .map(|b| 300_000.0 * (10_000.0 / (10_000.0 + b)))
        .sum();
    let dmg = hull - result.attacker_hull_remaining;
    assert!(
        (dmg - expected).abs() < 1e-2,
        "Annorax 6-volley total hull damage expected {expected:.2}, got {dmg:.2}"
    );

    // Tal on the bridge must suppress Annorax's Apex Barrier (SelfOfficerTalNotOnBridge).
    let with_tal = resolve_crew_to_buff_set(
        "stonn-95a1f6",
        &["annorax-830d35".to_string(), "tal-c3e4eb".to_string()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(1),
            ..Default::default()
        },
    );
    let result_with_tal = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &with_tal.crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let expected_no_annorax = 6.0 * 300_000.0 * (10_000.0 / 10_800.0);
    let dmg_with_tal = hull - result_with_tal.attacker_hull_remaining;
    assert!(
        (dmg_with_tal - expected_no_annorax).abs() < 1e-2,
        "Tal on bridge must suppress Annorax barrier: expected {expected_no_annorax:.2}, got {dmg_with_tal:.2}"
    );
}

#[test]
fn op6_enemy_shield_drain_and_suder_round_end_repair_resolve_and_apply() {
    let officers = load_officers();
    let opts = ResolveOptions {
        tier: Some(3),
        ..Default::default()
    };
    let buff_set = resolve_crew_to_buff_set(
        "stonn-95a1f6",
        &["ent-e-troi-46cdc3".to_string(), "suder-d348a9".to_string()],
        &[],
        &officers,
        &opts,
    );

    // Suder T3 -> HullRegenPrevRoundFraction(0.18)
    assert!(
        buff_set.crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HullRegenPrevRoundFraction(f) if (f - 0.18).abs() < 1e-6
        )),
        "Suder T3 must resolve to HullRegenPrevRoundFraction(0.18), seats={:?}",
        buff_set.crew.seats
    );

    // Ent-E Troi T3 -> CrewStatScaledDeferred(DefenderShieldDrainFlat, Attack, coeff=8.0)
    assert!(
        buff_set.crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::CrewStatScaledDeferred {
                target: CrewStatScaledTarget::DefenderShieldDrainFlat,
                axis: CrewStatAxis::Attack,
                coefficient,
                ..
            } if (coefficient - 8.0).abs() < 1e-6
        )),
        "Ent-E Troi T3 must resolve to op 6 DefenderShieldDrainFlat(8.0 * Attack), seats={:?}",
        buff_set.crew.seats
    );

    // Verify Vemet, Six Of Eleven cap, and Michael Burnham all compile into DefenderShieldDrainFlat
    for (cap, bridge, label) in [
        ("stonn-95a1f6", vec!["vemet-00a218".to_string()], "Vemet"),
        ("six-of-eleven-20cbe8", vec![], "Six Of Eleven cap"),
        (
            "stonn-95a1f6",
            vec!["michael-burnham-6c711c".to_string()],
            "Michael Burnham",
        ),
    ] {
        let bs = resolve_crew_to_buff_set(cap, &bridge, &[], &officers, &ResolveOptions::default());
        assert!(
            bs.crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::CrewStatScaledDeferred {
                    target: CrewStatScaledTarget::DefenderShieldDrainFlat,
                    ..
                }
            )),
            "{label} must resolve to CrewStatScaledDeferred::DefenderShieldDrainFlat, seats={:?}",
            bs.crew.seats
        );
    }

    // Engine verification: Ent-E Troi flat drain (370,176/round) + Suder 0.18 prev-round hull repair
    let hull = 10_000_000.0;
    let attacker = combatant_with_weapon("vidar", 10.0, 1, WeaponType::Energy, hull, 0.0);
    let defender = combatant_with_weapon(
        "fed_explorer_50",
        100_000.0,
        1,
        WeaponType::Energy,
        hull,
        1_000_000.0,
    );
    let cfg = SimulationConfig {
        rounds: 2,
        seed: 19,
        defender_level: Some(50),
        ..Default::default()
    };
    let crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "ent_e_troi_drain".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::DefenderShieldDrainFlat(370_176.0),
                    condition: Some(AbilityCondition::DefenderIsNpcHostile),
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: Some("ent-e-troi-46cdc3".to_string()),
                contribution_batch: 0,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "suder_repair".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: true,
                    effect: AbilityEffect::HullRegenPrevRoundFraction(0.18),
                    condition: Some(AbilityCondition::DefenderIsNpcHostile),
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: Some("suder-d348a9".to_string()),
                contribution_batch: 0,
            },
        ],
    };

    let res = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    // R1 takes 100k hull dmg; R2 start Suder heals 18k; R2 takes 100k -> net 182k hull dmg.
    let net_hull_dmg = hull - res.attacker_hull_remaining;
    assert!(
        (net_hull_dmg - 182_000.0).abs() < 1e-3,
        "Suder 0.18 repair over 2 rounds should leave 182,000 hull damage, got {net_hull_dmg}"
    );
    // 2 rounds of 370,176 drain + 2 shots of 8 shield dmg (80% of 10) = 740,368 shield removed from 1,000,000.
    let expected_shield_remaining = 1_000_000.0 - 2.0 * 370_176.0 - 2.0 * 8.0;
    assert!(
        (res.defender_shield_remaining - expected_shield_remaining).abs() < 1e-2,
        "Ent-E Troi should leave {expected_shield_remaining} shield, got {}",
        res.defender_shield_remaining
    );
}

#[test]
fn single_axis_armor_piercing_scales_only_ap_leaving_sp_and_accuracy_untouched() {
    let attacker =
        combatant_with_weapon("vidar", 100_000.0, 1, WeaponType::Energy, 10_000_000.0, 0.0);
    // Defender with high Armor only (Battleship) vs high Dodge only (Interceptor) to verify single-axis AP isolation!
    let make_defender = |ship_type: ShipType, armor: f64, sd: f64, dodge: f64| {
        let mut d = combatant_with_weapon("target", 1.0, 1, WeaponType::Energy, 10_000_000.0, 0.0);
        d.hostile_mitigation_params = Some(HostileMitigationParams {
            defender_stats: DefenderStats {
                armor,
                shield_deflection: sd,
                dodge,
            },
            base_attacker_stats: AttackerStats {
                armor_piercing: 52_356.0,
                shield_piercing: 4_176.0,
                accuracy: 3_132.0,
            },
            base_attacker_weapon_stats: vec![],
            profile_piercing_fractions: Some(AttackerStats {
                armor_piercing: 3.0,
                shield_piercing: 1.0,
                accuracy: 1.0,
            }),
            ship_type,
            floor: 0.0,
            ceiling: 0.712,
            mystery_mitigation_factor: 0.0,
            weapon_pierce_defender_stats: None,
            round_native_defense_stats: false,
        });
        d
    };
    let cfg = SimulationConfig {
        rounds: 2,
        seed: 23,
        defender_level: Some(50),
        ..Default::default()
    };

    let mral_and_azetbur = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "m_ral_ap".to_string(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::BaseArmorPiercingBonus(0.35),
                    condition: None,
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: Some("m-ral-986e7a".to_string()),
                contribution_batch: 0,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "azetbur_ap".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::AttackPhase,
                    boostable: true,
                    effect: AbilityEffect::OnHitBaseArmorPiercingBonus(0.40),
                    condition: None,
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: Some("azetbur-7eff22".to_string()),
                contribution_batch: 0,
            },
        ],
    };

    // Against high-Armor Battleship: AP bonus increases outbound hull damage!
    let bb = make_defender(ShipType::Battleship, 200_000.0, 0.0, 0.0);
    let bb_base = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &bb,
        &cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Battleship,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let bb_with_ap = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &bb,
        &cfg,
        &mral_and_azetbur,
        OpponentFactionTag::Federation,
        ShipType::Battleship,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        bb_with_ap.total_damage > bb_base.total_damage + 100.0,
        "M'Ral + Azetbur AP must increase damage vs Armor-heavy target: base={}, with_ap={}",
        bb_base.total_damage,
        bb_with_ap.total_damage
    );

    // Against pure-Dodge Interceptor (armor = 0): single-axis AP must NOT touch Accuracy or Dodge mitigation!
    let dodge_int = make_defender(ShipType::Interceptor, 0.0, 0.0, 200_000.0);
    let int_base = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &dodge_int,
        &cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let int_with_ap = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &dodge_int,
        &cfg,
        &mral_and_azetbur,
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Interceptor,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (int_with_ap.total_damage - int_base.total_damage).abs() < 1e-6,
        "Single-axis ArmorPiercing must not alter Accuracy/Dodge mitigation: base={}, with_ap={}",
        int_base.total_damage,
        int_with_ap.total_damage
    );

    // Paul Stamets (PvP CombatStart HostileBaseArmorPiercingReduction):
    // reduces enemy ArmorPiercing against Armor without altering enemy Accuracy vs Dodge.
    let officers = load_officers();
    let stamets_bs = resolve_crew_to_buff_set(
        "stonn-95a1f6",
        &["paul-stamets-4aa6ab".to_string()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    assert!(
        stamets_bs.crew.seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::HostileBaseArmorPiercingReduction(v) if v > 0.0
        )),
        "Paul Stamets must resolve to HostileBaseArmorPiercingReduction, seats={:?}",
        stamets_bs.crew.seats
    );

    let make_pvp_attacker = |ship_type: ShipType, armor: f64, dodge: f64| {
        let mut a = combatant_with_weapon("self", 1.0, 1, WeaponType::Energy, 10_000_000.0, 0.0);
        a.hostile_mitigation_params = Some(HostileMitigationParams {
            defender_stats: DefenderStats {
                armor,
                shield_deflection: 0.0,
                dodge,
            },
            base_attacker_stats: AttackerStats {
                armor_piercing: 50_000.0,
                shield_piercing: 50_000.0,
                accuracy: 50_000.0,
            },
            base_attacker_weapon_stats: vec![],
            profile_piercing_fractions: Some(AttackerStats {
                armor_piercing: 0.5,
                shield_piercing: 0.5,
                accuracy: 0.5,
            }),
            ship_type,
            floor: 0.0,
            ceiling: 0.712,
            mystery_mitigation_factor: 0.0,
            weapon_pierce_defender_stats: None,
            round_native_defense_stats: false,
        });
        a
    };
    let pvp_enemy = combatant_with_weapon(
        "enemy_player",
        100_000.0,
        1,
        WeaponType::Energy,
        10_000_000.0,
        0.0,
    );
    let defending_cfg = SimulationConfig {
        attacker_is_defending: true,
        ..cfg.clone()
    };

    let armor_ship = make_pvp_attacker(ShipType::Battleship, 200_000.0, 0.0);
    let armor_base = simulate_combat_with_defender_faction_and_defender_crew(
        &armor_ship,
        &pvp_enemy,
        &defending_cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    let armor_stamets = simulate_combat_with_defender_faction_and_defender_crew(
        &armor_ship,
        &pvp_enemy,
        &defending_cfg,
        &stamets_bs.crew,
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        armor_stamets.attacker_hull_remaining > armor_base.attacker_hull_remaining + 100.0,
        "Paul Stamets AP reduction must reduce inbound PvP damage vs Armor while defending: base_rem={}, stamets_rem={}",
        armor_base.attacker_hull_remaining,
        armor_stamets.attacker_hull_remaining
    );
    // When attacking (`attacker_is_defending: false`), SelfDefending gate suppresses Paul Stamets.
    let armor_stamets_attacking = simulate_combat_with_defender_faction_and_defender_crew(
        &armor_ship,
        &pvp_enemy,
        &cfg,
        &stamets_bs.crew,
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        (armor_stamets_attacking.attacker_hull_remaining - armor_base.attacker_hull_remaining)
            .abs()
            < 1e-6,
        "Paul Stamets SelfDefending gate must keep ability inert when attacking"
    );

    let dodge_ship = make_pvp_attacker(ShipType::Interceptor, 0.0, 200_000.0);
    let dodge_base = simulate_combat_with_defender_faction_and_defender_crew(
        &dodge_ship,
        &pvp_enemy,
        &defending_cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Interceptor,
        false,
        true,
        &CrewConfiguration::default(),
    );
    let dodge_stamets = simulate_combat_with_defender_faction_and_defender_crew(
        &dodge_ship,
        &pvp_enemy,
        &defending_cfg,
        &stamets_bs.crew,
        OpponentFactionTag::Federation,
        ShipType::Interceptor,
        ShipType::Interceptor,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        (dodge_stamets.attacker_hull_remaining - dodge_base.attacker_hull_remaining).abs() < 1e-6,
        "Paul Stamets single-axis AP reduction must not affect Dodge mitigation: base={}, stamets={}",
        dodge_base.attacker_hull_remaining,
        dodge_stamets.attacker_hull_remaining
    );
}
