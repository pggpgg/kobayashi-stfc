//! Catalog-to-combat regressions for officer abilities against NPC ships.
use std::collections::HashMap;

use kobayashi::combat::{
    simulate_combat, simulate_combat_with_defender_faction_and_defender_crew, AbilityClass,
    AbilityEffect, AttackerStats, Combatant, CrewConfiguration, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig, TimingWindow,
    TraceMode, WeaponStats, WeaponType,
};
use kobayashi::data::profile::{
    apply_profile_to_attacker, compute_officer_stat_runtime_bonus_with_officers,
    evaluate_officer_stats, OfficerStatConditionContext, PlayerProfile,
};
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
use kobayashi::lcars::{
    build_officer_model_default, index_lcars_officers_by_id, resolve_crew_to_buff_set, BuffSet,
    ResolveOptions,
};

const OFFICER_0718: &str = "718-0-2509d7";
const ANNORAX: &str = "annorax-830d35";
const ARIX: &str = "arix-b3d602";
const NPC_FACTION: i64 = 2064723306;
const ECLIPSE_FACTION: i64 = 1750120904;

fn crew(captain: &str, bridge: &[String], below: &[String], tier: u8) -> BuffSet {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    resolve_crew_to_buff_set(
        captain,
        bridge,
        below,
        &officers,
        &ResolveOptions {
            tier: Some(tier),
            ..Default::default()
        },
    )
}

#[test]
fn launch_attack_bonuses_raise_all_officer_ratings_before_weapon_damage_breakpoints() {
    let mariner = "beckett-mariner-d93865".to_owned();
    let eight = "eight-of-eleven-cb5f3c".to_owned();
    let below = ["tom-paris-3640cc".to_owned()];
    let captain = "arrock-0791b2";
    let profile = PlayerProfile {
        bonuses: [
            ("officer_attack".to_owned(), 0.10),
            ("officer_attack_below_decks".to_owned(), 0.20),
        ]
        .into(),
        ..Default::default()
    };
    let no_crew_bonuses = HashMap::new();

    for (bridge, rank, expected) in [
        (vec![mariner.clone()], 1, 0.05),
        (vec![eight.clone()], 1, 0.45),
        (vec![mariner.clone(), eight], 5, 1.15),
    ] {
        let buff = crew(captain, &bridge, &below, rank);
        // Set the threshold between a bridge-only and a full-crew contribution.
        let base =
            evaluate_officer_stats(&buff.officer_stat_rows, &profile, None, &no_crew_bonuses);
        let attack = buff.officer_stat_totals.attack;
        let below_attack = attack - buff.bridge_officer_stat_totals.attack;
        assert_eq!(buff.officer_stat_rows.len(), bridge.len() + 2);
        assert!(below_attack > 0.0);
        let ship = ShipRecord {
            officer_bonus: OfficerBonusTable {
                attack: vec![OfficerBonusBreakpoint {
                    value: base.effective_totals.attack + expected * (attack - below_attack / 2.0),
                    bonus: 0.50,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let compute = |is_player, static_buffs| {
            compute_officer_stat_runtime_bonus_with_officers(
                buff.officer_stat_totals,
                buff.bridge_officer_stat_totals,
                &buff.officer_stat_rows,
                &ship,
                &profile,
                None,
                static_buffs,
                &buff.pending_officer_stat_contributions,
                &OfficerStatConditionContext {
                    defender_is_player_ship: is_player,
                    ..Default::default()
                },
                &[],
            )
        };
        for is_player in [false, true] {
            let actual = compute(is_player, &buff.static_buffs);
            assert_eq!(actual.attack_bonus, 0.50, "rank {rank}, {bridge:?}");
            let unboosted = compute(is_player, &no_crew_bonuses);
            assert_eq!(unboosted.attack_bonus, 0.0);
        }
        let actual = buff.static_buffs["officer_attack"];
        assert!((actual - expected).abs() < 1e-12);
        let evaluated =
            evaluate_officer_stats(&buff.officer_stat_rows, &profile, None, &buff.static_buffs);
        for (before, after) in base.rows.iter().zip(evaluated.rows.iter()) {
            assert!(
                (after.effective.attack
                    - before.effective.attack
                    - expected * before.officer.raw.attack)
                    .abs()
                    < 1e-9
            );
            assert_eq!(after.effective.defense, before.effective.defense);
            assert_eq!(after.effective.health, before.effective.health);
        }

        let attacker = Combatant {
            attack: 100.0,
            hull_health: 10_000.0,
            ..Default::default()
        };
        let defender = Combatant {
            hull_health: 10_000.0,
            ..Default::default()
        };
        let attacker =
            apply_profile_to_attacker(attacker, &profile, None, compute(false, &buff.static_buffs));
        let result = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 1,
                ..Default::default()
            },
            buff.to_crew_config(),
        );
        assert_eq!(result.total_damage, 150.0);
    }
    let below_decks = crew(captain, &[], &[mariner], 5);
    assert!(!below_decks.static_buffs.contains_key("officer_attack"));
}

#[test]
fn computer_precision_raises_defense_of_all_crewed_seats_before_breakpoints() {
    let bridge = [OFFICER_0718.to_owned()];
    let below = ["tom-paris-3640cc".to_owned()];
    let resolve = |rank| crew("spock-c04738", &bridge, &below, rank);
    let rank_five = resolve(5);
    let rank_one = resolve(1);
    for (buff, expected) in [(&rank_one, 0.05), (&rank_five, 0.15)] {
        assert_eq!(buff.officer_stat_rows.len(), 3);
        let [bonus] = buff.pending_officer_stat_contributions.as_slice() else {
            panic!("Computer Precision must resolve to one crew Defense contribution");
        };
        assert!(bonus.target_attacker);
        assert_eq!(bonus.stat_key, "officer_defense");
        assert_eq!(bonus.value, expected);
        assert!(!buff.static_buffs.contains_key("armor"));
    }

    let profile = PlayerProfile {
        bonuses: [("officer_defense_below_decks".to_owned(), 0.20)].into(),
        ..Default::default()
    };
    // Put the breakpoint between a bridge-only bonus and the full-crew bonus.
    let totals = rank_five.officer_stat_totals;
    let below_defense = totals.defense - rank_five.bridge_officer_stat_totals.defense;
    assert!(below_defense > 0.0 && rank_five.bridge_officer_stat_totals.defense > 0.0);
    let ship = ShipRecord {
        ship_class: "explorer".to_owned(),
        armor: 400.0,
        shield_deflection: 600.0,
        dodge: 300.0,
        officer_bonus: OfficerBonusTable {
            defense: vec![OfficerBonusBreakpoint {
                value: totals.defense * 1.15 + below_defense * 0.125,
                bonus: 0.50,
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let compute = |buff: &BuffSet, faction, pvp| {
        compute_officer_stat_runtime_bonus_with_officers(
            buff.officer_stat_totals,
            buff.bridge_officer_stat_totals,
            &buff.officer_stat_rows,
            &ship,
            &profile,
            None,
            &buff.static_buffs,
            &buff.pending_officer_stat_contributions,
            &OfficerStatConditionContext {
                defender_faction_id: Some(faction),
                defender_is_player_ship: pvp,
                ..Default::default()
            },
            &[],
        )
    };
    let enabled = compute(&rank_five, NPC_FACTION, false);
    assert_eq!(enabled.defense_shield_deflection_add, 300.0);
    assert_eq!(enabled.defense_armor_add, 0.0);
    assert_eq!(enabled.defense_dodge_add, 0.0);
    assert_eq!(enabled.attack_bonus, 0.0);
    assert_eq!(enabled.health_bonus, 0.0);
    assert_eq!(
        compute(&rank_one, NPC_FACTION, false).defense_shield_deflection_add,
        0.0
    );
    assert_eq!(
        compute(&rank_five, ECLIPSE_FACTION, false).defense_shield_deflection_add,
        0.0
    );
    assert_eq!(
        compute(&rank_five, NPC_FACTION, true).defense_shield_deflection_add,
        0.0
    );
    assert!(crew("", &[], &[OFFICER_0718.to_owned()], 5)
        .pending_officer_stat_contributions
        .is_empty());
}

fn defense_fixture() -> (Combatant, Combatant) {
    let player = Combatant {
        id: "battleship".to_owned(),
        hull_health: 10_000.0,
        armor: 0.20,
        shield_deflection: 0.10,
        dodge: 0.25,
        weapons: vec![WeaponStats {
            attack: 10.0,
            shots: Some(3),
            weapon_type: WeaponType::Energy,
            ..Default::default()
        }],
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: true,
            defender_stats: DefenderStats {
                armor: 1000.0,
                shield_deflection: 300.0,
                dodge: 200.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 1000.0,
                shield_piercing: 500.0,
                accuracy: 500.0,
            },
            profile_piercing_fractions: None,
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Battleship,
            mystery_mitigation_factor: 0.0,
            floor: 0.25,
            ceiling: 0.25,
        }),
        ..Default::default()
    };
    let hostile = Combatant {
        id: "hostile".to_owned(),
        hull_health: 100_000.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(2),
            ..Default::default()
        }],
        ..Default::default()
    };
    (player, hostile)
}

#[test]
fn powered_armor_samples_actual_fight_start_hull_with_a_strict_threshold() {
    let (player, hostile) = defense_fixture();
    let config = SimulationConfig {
        rounds: 4,
        initial_attacker_hull_damage: 5_000.0,
        defender_hull_faction_id: NPC_FACTION,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let armor = |buff: &BuffSet, config: &SimulationConfig, player_opponent: bool| {
        let fight = simulate_combat_with_defender_faction_and_defender_crew(
            &player,
            &hostile,
            config,
            &buff.crew,
            OpponentFactionTag::Unknown,
            ShipType::Explorer,
            ShipType::Battleship,
            !player_opponent,
            player_opponent,
            &CrewConfiguration::default(),
        );
        fight
            .events
            .iter()
            .filter(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
            .map(|e| {
                assert_eq!(
                    e.values["effective_shield_deflection"].as_f64(),
                    Some(330.0)
                );
                assert_eq!(e.values["effective_dodge"].as_f64(), Some(250.0));
                (e.round_index, e.values["effective_armor"].as_f64().unwrap())
            })
            .collect::<Vec<_>>()
    };
    // With 750 hull lost between round starts, the four samples are 5000,
    // 4250, 3500, and 2750. Exactly 3500 is 70% of the actual starting hull.
    for (rank, expected_buffed_armor) in [(1, 1350.0), (5, 1700.0)] {
        let arix = crew("", &[ARIX.to_owned()], &[], rank);
        let stats = armor(&arix, &config, false);
        assert!(!stats.is_empty());
        assert!(stats.iter().any(|(round, _)| *round == 4));
        for (round, measured) in stats {
            assert_eq!(
                measured,
                if round < 4 {
                    1200.0
                } else {
                    expected_buffed_armor
                }
            );
        }
    }

    let arix = crew("", &[ARIX.to_owned()], &[], 5);
    for (faction, player_opponent) in [(ECLIPSE_FACTION, false), (NPC_FACTION, true)] {
        let outside_scope = SimulationConfig {
            defender_hull_faction_id: faction,
            ..config.clone()
        };
        let stats = armor(&arix, &outside_scope, player_opponent);
        assert!(stats.iter().any(|(round, _)| *round == 4));
        assert!(stats.iter().all(|(_, measured)| *measured == 1200.0));
    }
}

#[test]
fn powered_armor_reduces_counter_damage_only_with_raw_armor_context() {
    let (mut player, hostile) = defense_fixture();
    player.hostile_mitigation_params.as_mut().unwrap().floor = 0.16;
    player.hostile_mitigation_params.as_mut().unwrap().ceiling = 0.72;
    let config = SimulationConfig {
        rounds: 4,
        initial_attacker_hull_damage: 5_000.0,
        defender_hull_faction_id: NPC_FACTION,
        ..Default::default()
    };
    let arix = crew("", &[ARIX.to_owned()], &[], 5);
    let base = CrewConfiguration::default();
    let control = simulate_combat(&player, &hostile, &config, &base);
    let buffed = simulate_combat(&player, &hostile, &config, &arix.crew);
    assert!(buffed.attacker_hull_remaining > control.attacker_hull_remaining);
    assert_eq!(buffed.total_damage, control.total_damage);

    player.hostile_mitigation_params = None;
    let control = simulate_combat(&player, &hostile, &config, &base);
    let buffed = simulate_combat(&player, &hostile, &config, &arix.crew);
    assert_eq!(
        buffed.attacker_hull_remaining,
        control.attacker_hull_remaining
    );
}

#[test]
fn unresolved_hit_mechanics_remain_inert_in_multishot_hostile_fights() {
    let (player, hostile) = defense_fixture();
    let config = SimulationConfig {
        rounds: 4,
        defender_hull_faction_id: NPC_FACTION,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let base = simulate_combat(&player, &hostile, &config, &CrewConfiguration::default());
    let annorax = crew("", &[ANNORAX.to_owned()], &[], 5);
    let arix = crew(ARIX, &[], &[], 1);

    // Annorax (verified 2026-11-26 Batch B Fight D): resolves to reactive HitTaken flat Apex
    // Barrier (+2000..+3200 once per volley for 2 rounds) and reduces incoming hull damage.
    let annorax_seat = annorax
        .crew
        .seats
        .into_iter()
        .find(|s| s.ability.class == AbilityClass::BridgeAbility)
        .unwrap();
    assert_eq!(annorax_seat.ability.timing, TimingWindow::DefensePhase);
    assert!(matches!(
        annorax_seat.ability.effect,
        AbilityEffect::ReactiveDamageTaken {
            trigger: kobayashi::combat::DamageTakenTrigger::AnyDamage,
            kind: kobayashi::combat::ReactiveDamageTakenKind::ApexBarrierFlatBonus(v),
            duration_rounds: 2,
            ..
        } if (v - 3200.0).abs() < 1e-6
    ));
    let annorax_res = simulate_combat(
        &player,
        &hostile,
        &config,
        &CrewConfiguration {
            seats: vec![annorax_seat],
        },
    );
    assert!(
        annorax_res.attacker_hull_remaining > base.attacker_hull_remaining,
        "Annorax reactive Apex Barrier must reduce incoming damage over multi-round fights"
    );

    // Arix remains unresolved and inert.
    let disabled = arix
        .crew
        .seats
        .into_iter()
        .find(|s| s.ability.class == AbilityClass::CaptainManeuver)
        .unwrap();
    assert_eq!(disabled.ability.timing, TimingWindow::AttackPhase);
    assert!(matches!(
        disabled.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect { .. }
    ));
    let test_crew = CrewConfiguration {
        seats: vec![disabled],
    };
    let result = simulate_combat(&player, &hostile, &config, &test_crew);
    assert_eq!(result.attacker_hull_remaining, base.attacker_hull_remaining);
    assert_eq!(result.total_damage, base.total_damage);
    assert!(!result
        .events
        .iter()
        .any(|e| e.event_type == "burning_trigger"));
}

#[test]
fn launch_officer_stat_scopes_isolate_all_bridge_captain_and_self_officer_pools() {
    use kobayashi::lcars::OfficerStatSelfScope;

    let below = ["tom-paris-3640cc".to_owned()];
    let profile = PlayerProfile::default();
    let ctx = OfficerStatConditionContext::default();

    // 1. SelfAll Defense: Nine Of Eleven (+45%..+85%) and Reginald Barclay (+5%..+30%).
    for (id, expected) in [
        ("nine-of-eleven-8c475b", 0.85),
        ("reginald-barclay-111169", 0.30),
    ] {
        let buff = crew("arrock-0791b2", &[id.to_owned()], &below, 5);
        let actual = buff
            .static_buffs
            .get("officer_defense")
            .copied()
            .unwrap_or(0.0);
        assert!(
            (actual - expected).abs() < 1e-12,
            "{id}: got {actual}, expected {expected}"
        );
    }

    // 2. SelfBridge: Cadet Kirk captain (+8% A/D/H on captain+bridge, NOT below decks).
    let kirk_buff = crew(
        "cadet-kirk-a80563",
        &["arrock-0791b2".to_owned()],
        &below,
        1,
    );
    assert!(
        kirk_buff
            .pending_officer_stat_contributions
            .iter()
            .any(|c| {
                c.target_attacker
                    && c.stat_key == "officer_stat_all"
                    && c.self_scope == OfficerStatSelfScope::BridgeOfficers
                    && (c.value - 0.08).abs() < 1e-12
            }),
        "Cadet Kirk must emit BridgeOfficers officer_stat_all contribution"
    );
    let bridge_atk = kirk_buff.bridge_officer_stat_totals.attack;
    let total_atk = kirk_buff.officer_stat_totals.attack;
    let below_atk = total_atk - bridge_atk;
    assert!(below_atk > 0.0 && bridge_atk > 0.0);
    // Threshold strictly between (total + 0.08 * bridge) and (total + 0.08 * total):
    // proves below decks did NOT receive Cadet Kirk's bridge-only bonus.
    let ship_bridge_only = ShipRecord {
        officer_bonus: OfficerBonusTable {
            attack: vec![
                OfficerBonusBreakpoint {
                    value: total_atk + 0.08 * bridge_atk - 1.0,
                    bonus: 0.40,
                },
                OfficerBonusBreakpoint {
                    value: total_atk + 0.08 * bridge_atk + 0.04 * below_atk,
                    bonus: 0.80,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let kirk_osr = compute_officer_stat_runtime_bonus_with_officers(
        kirk_buff.officer_stat_totals,
        kirk_buff.bridge_officer_stat_totals,
        &kirk_buff.officer_stat_rows,
        &ship_bridge_only,
        &profile,
        None,
        &kirk_buff.static_buffs,
        &kirk_buff.pending_officer_stat_contributions,
        &ctx,
        &[],
    );
    assert_eq!(kirk_osr.attack_bonus, 0.40);

    // 3. SelfCaptain: Marla bridge (+90% A/D/H at T5 for captain ONLY, not Marla or below decks).
    let marla_buff = crew("arrock-0791b2", &["marla-9732c7".to_owned()], &below, 5);
    assert!(marla_buff
        .pending_officer_stat_contributions
        .iter()
        .any(|c| {
            c.target_attacker
                && c.stat_key == "officer_stat_all"
                && c.self_scope == OfficerStatSelfScope::CaptainOnly
                && (c.value - 0.90).abs() < 1e-12
        }));
    let cap_raw_atk = marla_buff.officer_stat_rows[0].raw.attack;
    let marla_raw_atk = marla_buff.officer_stat_rows[1].raw.attack;
    let marla_total_atk = marla_buff.officer_stat_totals.attack;
    assert!(cap_raw_atk > 0.0 && marla_raw_atk > 0.0);
    let ship_cap_only = ShipRecord {
        officer_bonus: OfficerBonusTable {
            attack: vec![
                OfficerBonusBreakpoint {
                    value: marla_total_atk + 0.90 * cap_raw_atk - 1.0,
                    bonus: 0.30,
                },
                OfficerBonusBreakpoint {
                    value: marla_total_atk + 0.90 * cap_raw_atk + 0.45 * marla_raw_atk,
                    bonus: 0.90,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let marla_osr = compute_officer_stat_runtime_bonus_with_officers(
        marla_buff.officer_stat_totals,
        marla_buff.bridge_officer_stat_totals,
        &marla_buff.officer_stat_rows,
        &ship_cap_only,
        &profile,
        None,
        &marla_buff.static_buffs,
        &marla_buff.pending_officer_stat_contributions,
        &ctx,
        &[],
    );
    assert_eq!(marla_osr.attack_bonus, 0.30);

    // 4. SelfOfficer: Wesley Crusher (+200% A/D/H at T5 for Wesley ONLY).
    let wesley_buff = crew(
        "arrock-0791b2",
        &["wesley-crusher-834fce".to_owned()],
        &below,
        5,
    );
    assert!(wesley_buff
        .pending_officer_stat_contributions
        .iter()
        .any(|c| {
            c.target_attacker
                && c.stat_key == "officer_stat_all"
                && c.self_scope
                    == OfficerStatSelfScope::SingleOfficer("wesley-crusher-834fce".into())
                && (c.value - 2.0).abs() < 1e-12
        }));
    let wesley_raw_atk = wesley_buff
        .officer_stat_rows
        .iter()
        .find(|r| r.officer_id == "wesley-crusher-834fce")
        .unwrap()
        .raw
        .attack;
    let wesley_other_atk = wesley_buff.officer_stat_totals.attack - wesley_raw_atk;
    let ship_self_only = ShipRecord {
        officer_bonus: OfficerBonusTable {
            attack: vec![
                OfficerBonusBreakpoint {
                    value: wesley_buff.officer_stat_totals.attack + 2.0 * wesley_raw_atk - 1.0,
                    bonus: 0.35,
                },
                OfficerBonusBreakpoint {
                    value: wesley_buff.officer_stat_totals.attack
                        + 2.0 * wesley_raw_atk
                        + 0.5 * wesley_other_atk,
                    bonus: 0.95,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let wesley_osr = compute_officer_stat_runtime_bonus_with_officers(
        wesley_buff.officer_stat_totals,
        wesley_buff.bridge_officer_stat_totals,
        &wesley_buff.officer_stat_rows,
        &ship_self_only,
        &profile,
        None,
        &wesley_buff.static_buffs,
        &wesley_buff.pending_officer_stat_contributions,
        &ctx,
        &[],
    );
    assert_eq!(wesley_osr.attack_bonus, 0.35);

    // 5. Flat Add (Origins Burnham) must remain quarantined pending native placement evidence.
    let origins = crew("origins-burnham-e854d6", &[], &[], 1);
    assert!(origins.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect { .. }
    )));
}
