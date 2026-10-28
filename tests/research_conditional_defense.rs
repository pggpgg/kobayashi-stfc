//! Source-backed research mappings exercised through controlled combat inputs.
//! These verify engine semantics, not historical player-profile accuracy.

use kobayashi::combat::abilities::NO_EXPLICIT_CONTRIBUTION_BATCH;
use kobayashi::combat::mitigation::{
    apply_base_mitigation_stat_bonuses, mitigation_for_hostile, pierce_damage_through_bonus,
};
use kobayashi::combat::{
    build_combat_setup_with_officer_stat, simulate_combat, simulate_combat_from_setup, Ability,
    AbilityClass, AbilityCondition, AbilityEffect, AttackerStats, CombatEvent, Combatant,
    CrewConfiguration, CrewOfficerStatTotals, CrewSeat, CrewSeatContext, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig, SimulationResult,
    TimingWindow, TraceMode, WeaponStats,
};
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::officer_stat_round::OfficerStatRoundContext;
use kobayashi::data::profile::{
    apply_profile_to_attacker, research_derived_attack_phase_seats, OfficerStatConditionContext,
    PlayerProfile, SupportBuffResearchGateState,
};
use kobayashi::data::research::{
    load_research_canonical_overrides, load_research_catalog, DEFAULT_RESEARCH_CANONICAL_PATH,
    DEFAULT_RESEARCH_CATALOG_PATH,
};
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
use kobayashi::lcars::DynamicOfficerStatContribution;

fn research(rid: i64, level: u32) -> CrewConfiguration {
    CrewConfiguration {
        seats: research_derived_attack_phase_seats(
            &[ResearchEntry {
                rid,
                level: i64::from(level),
            }],
            &load_research_catalog(DEFAULT_RESEARCH_CATALOG_PATH).unwrap(),
            &SupportBuffResearchGateState::default(),
            &load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH),
        ),
    }
}

fn state(effect: AbilityEffect, timing: TimingWindow, rounds: u32) -> CrewSeatContext {
    CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: "controlled_state".into(),
            class: AbilityClass::ShipAbility,
            timing,
            boostable: false,
            effect,
            condition: Some(AbilityCondition::RoundRange {
                min: 1,
                max: rounds,
            }),
            weapon_scope: Default::default(),
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    }
}

fn raw_context() -> HostileMitigationParams {
    HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: DefenderStats {
            armor: 20.0,
            shield_deflection: 40.0,
            dodge: 70.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: AttackerStats {
            armor_piercing: 100.0,
            shield_piercing: 160.0,
            accuracy: 220.0,
        },
        base_attacker_weapon_stats: vec![
            AttackerStats {
                armor_piercing: 100.0,
                shield_piercing: 160.0,
                accuracy: 220.0,
            },
            AttackerStats {
                armor_piercing: 300.0,
                shield_piercing: 80.0,
                accuracy: 110.0,
            },
        ],
        ship_type: ShipType::Battleship,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    }
}

fn attacker() -> Combatant {
    Combatant {
        id: "research_owner".into(),
        attack: 10.0,
        hull_health: 1_000_000.0,
        armor: 0.1,
        shield_deflection: 0.2,
        dodge: 0.3,
        hostile_mitigation_params: Some(raw_context()),
        ..Default::default()
    }
}

fn defender() -> Combatant {
    Combatant {
        id: "controlled_hostile".into(),
        hull_health: 1_000_000.0,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn run(attacker: &Combatant, crew: &CrewConfiguration, rounds: u32) -> SimulationResult {
    simulate_combat(
        attacker,
        &defender(),
        &SimulationConfig {
            rounds,
            seed: 7,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        crew,
    )
}

fn counter_rows(result: &SimulationResult) -> Vec<&CombatEvent> {
    result
        .events
        .iter()
        .filter(|event| event.event_type == "mitigation_calc" && event.phase == "counter")
        .collect()
}

fn value(row: &CombatEvent, key: &str) -> f64 {
    row.values[key]
        .as_f64()
        .unwrap_or_else(|| panic!("missing {key}: {row:?}"))
}

fn close(actual: f64, expected: f64) {
    // Traces round to six decimal places.
    assert!(
        (actual - expected).abs() < 0.000001,
        "{actual} != {expected}"
    );
}

#[test]
fn each_research_gate_uses_raw_base_stats_and_each_opponent_weapon() {
    let cases = [
        (
            2_047_743_532,
            AbilityEffect::Burning {
                chance: 1.0,
                duration_rounds: 1,
            },
        ),
        (535_909_811, AbilityEffect::Morale(1.0)),
        (
            1_233_598_019,
            AbilityEffect::HullBreach {
                chance: 1.0,
                duration_rounds: 1,
                requires_critical: false,
            },
        ),
    ];
    for (rid, source) in cases {
        let mut crew = research(rid, 10);
        assert_eq!(crew.seats.len(), 1);
        assert!(
            matches!(crew.seats[0].ability.effect, AbilityEffect::BaseMitigationStatsBonus(v) if v == 5.0)
        );
        assert_eq!(crew.seats[0].ability.timing, TimingWindow::RoundStart);
        let inactive = run(&attacker(), &crew, 1);
        assert!(counter_rows(&inactive)
            .iter()
            .all(|row| { row.values["base_mitigation_stats_applied"] == false }));
        assert!(counter_rows(&inactive)
            .iter()
            .all(|row| row.values["raw_mitigation_stats_applied"] == true));
        for row in counter_rows(&inactive) {
            close(value(row, "effective_armor"), 22.0);
            close(value(row, "effective_shield_deflection"), 48.0);
            close(value(row, "effective_dodge"), 91.0);
        }
        crew.seats.push(state(source, TimingWindow::RoundStart, 1));
        let active = run(&attacker(), &crew, 1);
        let rows = counter_rows(&active);
        assert_eq!(rows.len(), 2);
        // The rank's 5.0 is +500% in a shared base-stat additive pool.
        let effective = DefenderStats {
            armor: 122.0,
            shield_deflection: 248.0,
            dodge: 441.0,
        };
        let mut expected_damage_taken = 0.0;
        for (i, row) in rows.iter().enumerate() {
            close(value(row, "effective_armor"), effective.armor);
            close(
                value(row, "effective_shield_deflection"),
                effective.shield_deflection,
            );
            close(value(row, "effective_dodge"), effective.dodge);
            let opponent = raw_context().base_attacker_weapon_stats[i];
            let mitigation =
                mitigation_for_hostile(effective, opponent, ShipType::Battleship, 0.0, 0.16, 0.72);
            let pierce = pierce_damage_through_bonus(effective, opponent, ShipType::Battleship);
            close(value(row, "mitigation"), mitigation);
            close(value(row, "base_stat_pierce"), pierce);
            expected_damage_taken += 100.0 * (1.0 - mitigation + pierce);
        }
        close(
            active.attacker_hull_remaining,
            attacker().hull_health - expected_damage_taken,
        );
        assert!(
            active.attacker_hull_remaining > inactive.attacker_hull_remaining,
            "a positive defense bonus must improve the same raw-stat baseline"
        );
        assert_ne!(value(rows[0], "mitigation"), value(rows[1], "mitigation"));
        assert_eq!(
            active.total_isolytic_damage, 0.0,
            "defense cannot become isolytic damage"
        );
    }
}

#[test]
fn owned_rank_improves_the_same_baseline_monotonically() {
    let owner = attacker();
    let mut previous_hull = run(&owner, &CrewConfiguration::default(), 1).attacker_hull_remaining;
    for rank in [1, 2, 10] {
        let mut crew = research(535_909_811, rank);
        crew.seats.push(state(
            AbilityEffect::Morale(1.0),
            TimingWindow::RoundStart,
            1,
        ));
        let result = run(&owner, &crew, 1);
        assert!(
            result.attacker_hull_remaining > previous_hull,
            "rank {rank} should improve survival without selecting a different mitigation model"
        );
        previous_hull = result.attacker_hull_remaining;
    }
}

#[test]
fn round_officer_defense_is_a_raw_delta_added_once_after_fractional_bonuses() {
    let ship = ShipRecord {
        ship_class: "battleship".into(),
        armor: 20.0,
        shield_deflection: 40.0,
        dodge: 70.0,
        officer_bonus: OfficerBonusTable {
            defense: vec![
                OfficerBonusBreakpoint {
                    value: 100.0,
                    bonus: 0.5,
                },
                OfficerBonusBreakpoint {
                    value: 200.0,
                    bonus: 1.0,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let totals = CrewOfficerStatTotals {
        defense: 100.0,
        ..Default::default()
    };
    let profile = PlayerProfile::default();
    let context = OfficerStatRoundContext::try_from_ship_and_buffs(
        &ship,
        &profile,
        &Default::default(),
        totals,
        totals,
        &[],
        &[],
        &[DynamicOfficerStatContribution {
            stat_key: "officer_defense".into(),
            value: 1.0,
            target_attacker: true,
            opponent_scope: Default::default(),
            runtime_condition: Some(AbilityCondition::MoraleActive),
            timing: TimingWindow::RoundStart,
        }],
        OfficerStatConditionContext::default(),
    )
    .unwrap();
    // Baseline officer Defense adds ten raw armor, converted to +0.5 once.
    close(context.baseline_osr.defense_armor_add, 10.0);
    let owner = apply_profile_to_attacker(attacker(), &profile, None, context.baseline_osr);
    close(owner.armor, 0.6);
    let mut crew = research(535_909_811, 1);
    crew.seats.push(state(
        AbilityEffect::Morale(1.0),
        TimingWindow::RoundStart,
        1,
    ));
    let config = SimulationConfig {
        rounds: 2,
        seed: 7,
        trace_mode: TraceMode::Events,
        ..Default::default()
    };
    let setup = build_combat_setup_with_officer_stat(
        &owner,
        &defender(),
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
        Some(context),
    );
    let result = simulate_combat_from_setup(&setup, config.seed);
    let rows = counter_rows(&result);
    assert_eq!(rows.len(), 4);
    for row in rows {
        let (armor, shield, dodge) = if row.round_index == 1 {
            // New breakpoint: +10 RAW armor, research: +50% of each BASE stat.
            // 20*(1 + .1 profile + .5 baseline officer + .5 research) + 10.
            (52.0, 68.0, 126.0)
        } else {
            // Both dynamic gates expire; the original ten raw armor remain.
            (32.0, 48.0, 91.0)
        };
        close(value(row, "effective_armor"), armor);
        close(value(row, "effective_shield_deflection"), shield);
        close(value(row, "effective_dodge"), dodge);
    }
}

#[test]
fn inactive_bonus_keeps_raw_baseline_and_missing_raw_stats_keep_legacy_results() {
    let base = attacker();
    let crew = research(535_909_811, 1);
    let no_seats = run(&base, &CrewConfiguration::default(), 1);
    let inactive = run(&base, &crew, 1);
    assert_eq!(
        inactive.attacker_hull_remaining,
        no_seats.attacker_hull_remaining
    );
    let mut no_raw = base.clone();
    no_raw.hostile_mitigation_params = None;
    let legacy = run(&no_raw, &CrewConfiguration::default(), 1);
    assert_ne!(
        no_seats.attacker_hull_remaining, legacy.attacker_hull_remaining,
        "raw context, not research ownership or activation, selects the calculation"
    );
    let mut active_crew = crew;
    active_crew.seats.push(state(
        AbilityEffect::Morale(1.0),
        TimingWindow::RoundStart,
        1,
    ));
    let active_without_raw = run(&no_raw, &active_crew, 1);
    assert_eq!(
        active_without_raw.attacker_hull_remaining,
        legacy.attacker_hull_remaining
    );
    let mut zero_raw = base;
    zero_raw
        .hostile_mitigation_params
        .as_mut()
        .unwrap()
        .defender_stats = DefenderStats::default();
    let active_with_zero_raw = run(&zero_raw, &active_crew, 1);
    assert_eq!(
        active_with_zero_raw.attacker_hull_remaining,
        legacy.attacker_hull_remaining
    );
    assert!(counter_rows(&active_with_zero_raw)
        .iter()
        .chain(counter_rows(&active_without_raw).iter())
        .all(|row| row.values["raw_mitigation_stats_applied"] == false));
}

#[test]
fn base_stat_bonus_samples_round_start_not_later_hits_and_expires() {
    let mut crew = research(2_047_743_532, 1);
    crew.seats.push(state(
        AbilityEffect::Burning {
            chance: 1.0,
            duration_rounds: 2,
        },
        TimingWindow::AttackPhase,
        1,
    ));
    let result = run(&attacker(), &crew, 3);
    let rows = counter_rows(&result);
    assert_eq!(rows.len(), 6);
    for row in rows {
        let expected = if row.round_index == 2 { 0.5 } else { 0.0 };
        close(value(row, "base_mitigation_stats_bonus"), expected);
        assert_eq!(row.values["raw_mitigation_stats_applied"], true);
    }
}

#[test]
fn simultaneous_research_bonuses_add_to_the_same_base_pool_once() {
    let mut crew = research(2_047_743_532, 1);
    crew.seats.extend(research(535_909_811, 2).seats);
    crew.seats.push(state(
        AbilityEffect::Burning {
            chance: 1.0,
            duration_rounds: 2,
        },
        TimingWindow::RoundStart,
        2,
    ));
    crew.seats.push(state(
        AbilityEffect::Morale(1.0),
        TimingWindow::RoundStart,
        2,
    ));
    let result = run(&attacker(), &crew, 2);
    for row in counter_rows(&result) {
        close(value(row, "base_mitigation_stats_bonus"), 0.5 + 0.75);
        close(
            value(row, "effective_armor"),
            20.0 * (1.0 + 0.1 + 0.5 + 0.75),
        );
    }
}

#[test]
fn base_stat_arithmetic_preserves_zero_components_and_fraction_units() {
    let actual = apply_base_mitigation_stat_bonuses(
        DefenderStats {
            armor: 0.0,
            shield_deflection: 10.0,
            dodge: 20.0,
        },
        DefenderStats {
            armor: 1.0,
            shield_deflection: 0.5,
            dodge: 0.25,
        },
        5.0,
    );
    assert_eq!(actual.armor, 0.0);
    assert_eq!(actual.shield_deflection, 65.0);
    assert_eq!(actual.dodge, 125.0);
}

#[test]
fn morale_research_repairs_max_shield_fraction_once_at_round_start() {
    let mut owner = attacker();
    owner.armor = 0.0;
    owner.shield_deflection = 0.0;
    owner.dodge = 0.0;
    owner.hostile_mitigation_params = None;
    owner.shield_health = 1_000.0;
    owner.shield_mitigation = 1.0;
    let mut crew = research(3_288_570_685, 1);
    assert_eq!(crew.seats.len(), 1);
    assert!(
        matches!(crew.seats[0].ability.effect, AbilityEffect::ShieldRegenMaxFraction(v) if v == 0.01)
    );
    assert_eq!(crew.seats[0].ability.timing, TimingWindow::RoundStart);
    let inactive = run(&owner, &crew, 2);
    // Two 100-damage shots each round: 1,000 - 400.
    close(inactive.attacker_shield_remaining, 600.0);
    crew.seats.push(state(
        AbilityEffect::Morale(1.0),
        TimingWindow::RoundStart,
        2,
    ));
    let active = run(&owner, &crew, 2);
    // R1 repair is capped at full SHP; R2 restores 1% of MAX SHP = 10 before fire.
    close(active.attacker_shield_remaining, 610.0);
    assert_eq!(
        active.total_damage, inactive.total_damage,
        "repair cannot become crit damage"
    );
    // Double max SHP doubles the repair, independent of the identical damage taken.
    owner.shield_health = 2_000.0;
    let larger = run(&owner, &crew, 2);
    close(larger.attacker_shield_remaining, 1_620.0);
}

#[test]
fn native_burning_weapon_damage_joins_the_compiled_base_percentage_pool() {
    let mut crew = research(365_419_690, 8);
    assert_eq!(crew.seats.len(), 1);
    assert_eq!(
        crew.seats[0].ability.effect,
        AbilityEffect::BaseWeaponDamageBonus(6.75)
    );
    // Set Burning before RoundStart so this regression establishes arithmetic,
    // independently of the unresolved ordering of simultaneous state sources.
    crew.seats.push(state(
        AbilityEffect::Burning {
            chance: 1.0,
            duration_rounds: 2,
        },
        TimingWindow::CombatBegin,
        2,
    ));
    let owner = Combatant {
        attack: 44.0 * 2.0 * (1.0 + 44.7),
        hull_health: 1_000_000.0,
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Events,
        profile_weapon_damage_fraction: 44.7,
        ..Default::default()
    };
    let result = simulate_combat(&owner, &defender(), &config, &crew);
    let roll = result
        .events
        .iter()
        .find(|e| e.event_type == "attack_roll" && e.phase == "attack")
        .unwrap();
    close(value(roll, "effective_attack"), 44.0 * 2.0 * 52.45);

    // An unverified legacy multiplier remains a separate multiplicative
    // channel; assigning raw native units must not reinterpret other effects.
    crew.seats[0].ability.effect = AbilityEffect::AttackMultiplier(6.75);
    let legacy = simulate_combat(&owner, &defender(), &config, &crew);
    let roll = legacy
        .events
        .iter()
        .find(|e| e.event_type == "attack_roll" && e.phase == "attack")
        .unwrap();
    close(value(roll, "effective_attack"), owner.attack * 7.75);
}

#[test]
fn native_burning_isolytic_research_runs_at_round_start_with_its_state_gate() {
    let mut crew = research(2_580_836_593, 8);
    assert_eq!(crew.seats.len(), 1);
    assert_eq!(crew.seats[0].ability.timing, TimingWindow::RoundStart);
    assert_eq!(
        crew.seats[0].ability.condition,
        Some(AbilityCondition::DefenderBurning)
    );
    assert_eq!(
        crew.seats[0].ability.effect,
        AbilityEffect::IsolyticDamageBonus(0.16)
    );
    let inactive = run(&attacker(), &crew, 1);
    assert_eq!(inactive.total_isolytic_damage, 0.0);
    crew.seats.push(state(
        AbilityEffect::Burning {
            chance: 1.0,
            duration_rounds: 2,
        },
        TimingWindow::CombatBegin,
        2,
    ));
    let active = run(&attacker(), &crew, 1);
    assert!(active.total_isolytic_damage > 0.0);
    assert!(active
        .events
        .iter()
        .any(|e| e.event_type == "ability_activation"
            && e.phase == "round_start"
            && e.source
                .ship_ability_id
                .as_deref()
                .is_some_and(|id| id.contains("research_2580836593_iso_burning"))));
}

#[test]
fn native_hull_breach_isolytic_research_uses_its_declared_round_start_phase() {
    let mut crew = research(1_995_496_344, 4);
    assert_eq!(crew.seats.len(), 1);
    assert_eq!(crew.seats[0].ability.timing, TimingWindow::RoundStart);
    assert_eq!(
        crew.seats[0].ability.effect,
        AbilityEffect::IsolyticDamageBonus(0.085)
    );
    assert_eq!(
        crew.seats[0].ability.condition,
        Some(AbilityCondition::DefenderHullBreach)
    );
    assert_eq!(run(&attacker(), &crew, 1).total_isolytic_damage, 0.0);
    crew.seats.push(state(
        AbilityEffect::HullBreach {
            chance: 1.0,
            duration_rounds: 2,
            requires_critical: false,
        },
        TimingWindow::RoundStart,
        2,
    ));
    let active = run(&attacker(), &crew, 1);
    assert!(active.total_isolytic_damage > 0.0);
    assert!(active
        .events
        .iter()
        .any(|e| e.event_type == "ability_activation"
            && e.phase == "round_start"
            && e.source
                .ship_ability_id
                .as_deref()
                .is_some_and(|id| id.contains("research_1995496344_iso_hb"))));
}
