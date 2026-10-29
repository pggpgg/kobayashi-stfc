//! Fixed, profile-free engine workloads for paired release measurements.
//!
//! `--list`; `--case NAME --iterations N`; `--parity [--case NAME]`.
//! Timing uses prepared inputs and TraceMode::Off. Every timed workload first
//! validates all 64 seeds with tracing on and off, outside the measured interval.
//! JSONL parity output hashes complete results and exposes exact floating-point bits;
//! it deliberately contains no revision, clock, machine, or filesystem fields.

use std::collections::HashMap;
use std::hint::black_box;
use std::io::{self, BufWriter, Write};
use std::time::Instant;

use kobayashi::combat::{
    build_combat_setup_with_officer_stat, mitigation_for_hostile, pierce_damage_through_bonus,
    simulate_combat_from_setup, Ability, AbilityClass, AbilityCondition, AbilityEffect,
    AttackerStats, Combatant, CrewConfiguration, CrewOfficerStatTotals, CrewSeat, CrewSeatContext,
    DefenderOnHitGate, DefenderOnHitStat, DefenderStats, HostileMitigationParams,
    OpponentFactionTag, PreCombatSetup, ShipType, SimulationConfig, SimulationResult, TimingWindow,
    TraceMode, WeaponStats, WeaponType,
};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::officer_stat_round::OfficerStatRoundContext;
use kobayashi::data::profile::{OfficerStatConditionContext, PlayerProfile};
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;
use kobayashi::lcars::{
    index_lcars_officers_by_id, resolve_crew_to_buff_set, DynamicOfficerStatContribution,
    OfficerStatOpponentScope, ResolveOptions,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[path = "hot_path_bench/allocations.rs"]
mod allocations;

const CASE_NAMES: [&str; 7] = [
    "legacy_round_one",
    "scalar_100_rounds",
    "weapons_40_rounds",
    "status_40_rounds",
    "catalog_lcars_profile_free",
    "scalar_100_rounds_wrapper",
    "dynamic_officer_stats_40",
];
const PANEL_LEN: usize = 64;
const SEEDS: [u64; PANEL_LEN] = seed_panel();

const fn seed_panel() -> [u64; PANEL_LEN] {
    let mut seeds = [0_u64; PANEL_LEN];
    let mut index = 0;
    while index < PANEL_LEN {
        let mut z = 7_u64.wrapping_add((index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        seeds[index] = z ^ (z >> 31);
        index += 1;
    }
    seeds[0] = 7;
    seeds[1] = 42;
    seeds[2] = 1337;
    seeds[3] = 9_007_199_254_740_993;
    seeds
}

struct Case {
    name: &'static str,
    description: &'static str,
    attacker: Combatant,
    defender: Combatant,
    crew: CrewConfiguration,
    defender_crew: CrewConfiguration,
    config: SimulationConfig,
    attacker_type: ShipType,
    defender_type: ShipType,
    defender_faction: OpponentFactionTag,
    expected_rounds: Option<u32>,
    minimum_rounds: u32,
    metadata: Value,
    include_setup: bool,
    officer_stat_round: Option<OfficerStatRoundContext>,
}

impl Case {
    fn prepared(&self, mode: TraceMode) -> PreCombatSetup {
        let mut config = self.config.clone();
        config.trace_mode = mode;
        build_combat_setup_with_officer_stat(
            &self.attacker,
            &self.defender,
            &config,
            &self.crew,
            self.defender_faction,
            self.defender_type,
            self.attacker_type,
            true,
            false,
            &self.defender_crew,
            self.officer_stat_round.clone(),
        )
    }

    fn workload(&self) -> Value {
        // Combatant's wire representation omits raw mitigation context. Include
        // it explicitly so changing raw stats cannot preserve the workload hash.
        json!({
            "name": self.name,
            "description": self.description,
            "attacker": self.attacker,
            "defender": self.defender,
            "attacker_raw_mitigation": format!("{:?}", self.attacker.hostile_mitigation_params),
            "defender_raw_mitigation": format!("{:?}", self.defender.hostile_mitigation_params),
            "attacker_crew": format!("{:?}", self.crew),
            "defender_crew": format!("{:?}", self.defender_crew),
            "config": self.config,
            "attacker_type": format!("{:?}", self.attacker_type),
            "defender_type": format!("{:?}", self.defender_type),
            "defender_faction": format!("{:?}", self.defender_faction),
            "expected_rounds": self.expected_rounds,
            "minimum_rounds": self.minimum_rounds,
            "seeds": SEEDS.iter().map(|seed| seed.to_string()).collect::<Vec<_>>(),
            "metadata": self.metadata,
            "include_setup_in_timing": self.include_setup,
            "officer_stat_round": self.officer_stat_round.as_ref().map(|context| json!({"officer_bonus": context.officer_bonus, "ship_class": context.ship_class, "base_armor": context.base_armor, "base_shield_deflection": context.base_shield_deflection, "base_dodge": context.base_dodge, "totals": format!("{:?}", context.totals), "bridge_totals": format!("{:?}", context.bridge_totals), "profile": context.profile, "static_buffs": context.static_buffs, "dynamic_contributions": format!("{:?}", context.dynamic_contributions), "static_pending": format!("{:?}", context.static_pending), "opponent_enemy_pending": format!("{:?}", context.opponent_enemy_pending), "baseline_osr": format!("{:?}", context.baseline_osr)})),
        })
    }
}

fn combatant(id: &str) -> Combatant {
    Combatant {
        id: id.into(),
        attack: 900.0,
        mitigation: 0.4,
        pierce: 0.08,
        crit_chance: 0.2,
        crit_multiplier: 1.6,
        proc_chance: 0.15,
        proc_multiplier: 1.2,
        hull_health: 10_000_000.0,
        shield_health: 80_000.0,
        shield_mitigation: 0.8,
        ..Combatant::default()
    }
}

fn base_case(name: &'static str, rounds: u32, description: &'static str) -> Case {
    Case {
        name,
        description,
        attacker: combatant("bench-attacker"),
        defender: combatant("bench-defender"),
        crew: CrewConfiguration::default(),
        defender_crew: CrewConfiguration::default(),
        config: SimulationConfig {
            rounds,
            seed: 7,
            trace_mode: TraceMode::Off,
            emit_state_snapshots: false,
            ..SimulationConfig::default()
        },
        attacker_type: ShipType::Explorer,
        defender_type: ShipType::Battleship,
        defender_faction: OpponentFactionTag::Romulan,
        expected_rounds: Some(rounds),
        minimum_rounds: rounds,
        metadata: json!({"profile": "none", "synthetic": true}),
        include_setup: false,
        officer_stat_round: None,
    }
}

fn seat(name: &str, timing: TimingWindow, effect: AbilityEffect) -> CrewSeatContext {
    CrewSeatContext::legacy(
        CrewSeat::Ship,
        Ability {
            name: name.into(),
            class: AbilityClass::ShipAbility,
            timing,
            effect,
            condition: None,
            weapon_scope: Default::default(),
            boostable: false,
        },
        false,
    )
}

fn conditional(mut row: CrewSeatContext, condition: AbilityCondition) -> CrewSeatContext {
    row.ability.condition = Some(condition);
    row
}

fn raw_context(ship_type: ShipType, weapon_count: usize) -> HostileMitigationParams {
    HostileMitigationParams {
        round_native_defense_stats: true,
        defender_stats: DefenderStats {
            armor: 1_250.4,
            shield_deflection: 1_780.7,
            dodge: 920.1,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: AttackerStats {
            armor_piercing: 1_100.0,
            shield_piercing: 1_600.0,
            accuracy: 1_500.0,
        },
        base_attacker_weapon_stats: (0..weapon_count)
            .map(|index| AttackerStats {
                armor_piercing: 1_000.0 + index as f64 * 90.0,
                shield_piercing: 1_400.0 + index as f64 * 80.0,
                accuracy: 1_300.0 + index as f64 * 70.0,
            })
            .collect(),
        ship_type,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    }
}

fn weapons(count: usize) -> Vec<WeaponStats> {
    (0..count)
        .map(|index| WeaponStats {
            attack: 800.0 + index as f64 * 180.0,
            component_base_attack: None,
            shots: Some([3, 2, 4, 1, 3, 2][index % 6]),
            pierce: Some(0.04 + index as f64 * 0.01),
            crit_chance: Some(0.25 + index as f64 * 0.05),
            crit_multiplier: Some(1.5 + index as f64 * 0.15),
            proc_chance: Some(0.2),
            proc_multiplier: Some(1.3),
            weapon_type: if index % 2 == 0 {
                WeaponType::Energy
            } else {
                WeaponType::Kinetic
            },
        })
        .collect()
}

fn synthetic_case(name: &str) -> Option<Case> {
    match name {
        "legacy_round_one" => {
            let mut case = base_case(
                CASE_NAMES[0],
                100,
                "Historical microbenchmark: oversized fractional pierce causes a round-one kill.",
            );
            case.attacker.attack = 500.0;
            case.attacker.pierce = 200.0;
            case.attacker.hull_health = 1_000.0;
            case.attacker.shield_health = 0.0;
            case.defender.attack = 0.0;
            case.defender.mitigation = 300.0;
            case.defender.hull_health = 1_000.0;
            case.defender.shield_health = 0.0;
            case.expected_rounds = Some(1);
            case.minimum_rounds = 1;
            Some(case)
        }
        "scalar_100_rounds" | "scalar_100_rounds_wrapper" => {
            let mut case = base_case(
                CASE_NAMES[1],
                100,
                "Guaranteed 100-round scalar exchange, empty crews and ordinary fractional stats.",
            );
            case.attacker.attack = 40.0;
            case.defender.attack = 30.0;
            case.attacker.hull_health = 500_000.0;
            case.defender.hull_health = 400_000.0;
            if name == CASE_NAMES[5] {
                case.name = CASE_NAMES[5];
                case.include_setup = true;
                case.description = "Same 100-round scalar workload, including per-combat immutable setup construction.";
            }
            Some(case)
        }
        "dynamic_officer_stats_40" => {
            let mut case = base_case(CASE_NAMES[6], 40, "Synthetic morale-gated three-axis officer breakpoint recomputation each round; no filesystem profile.");
            let breakpoints = vec![
                OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                },
                OfficerBonusBreakpoint {
                    value: 100.0,
                    bonus: 0.2,
                },
                OfficerBonusBreakpoint {
                    value: 280.0,
                    bonus: 0.5,
                },
            ];
            let ship = ShipRecord {
                ship_class: "explorer".into(),
                armor: 1000.0,
                shield_deflection: 1200.0,
                dodge: 800.0,
                officer_bonus: OfficerBonusTable {
                    attack: breakpoints.clone(),
                    defense: breakpoints.clone(),
                    health: breakpoints,
                },
                ..ShipRecord::default()
            };
            let totals = CrewOfficerStatTotals {
                attack: 200.0,
                defense: 200.0,
                health: 200.0,
            };
            let contributions = vec![DynamicOfficerStatContribution {
                stat_key: "officer_stat_all".into(),
                value: 0.5,
                target_attacker: true,
                opponent_scope: OfficerStatOpponentScope::AllCrewed,
                runtime_condition: Some(AbilityCondition::MoraleActive),
                timing: TimingWindow::RoundStart,
            }];
            case.officer_stat_round = OfficerStatRoundContext::try_from_ship_and_buffs(
                &ship,
                &PlayerProfile::default(),
                &HashMap::new(),
                totals,
                totals,
                &[],
                &[],
                &contributions,
                OfficerStatConditionContext::default(),
            );
            case.officer_stat_round.as_ref()?;
            case.crew.seats = vec![seat(
                "dynamic-morale",
                TimingWindow::RoundStart,
                AbilityEffect::Morale(0.65),
            )];
            case.attacker.hostile_mitigation_params = Some(raw_context(case.attacker_type, 1));
            case.attacker.weapons = weapons(2);
            case.defender.weapons = weapons(2);
            Some(case)
        }
        "weapons_40_rounds" => {
            let mut case = base_case(
                CASE_NAMES[2],
                40,
                "Six weapons per side, mixed types/shots, raw mitigation, fractional profile-style bonuses, shields, apex and isolytic.",
            );
            case.attacker.weapons = weapons(6);
            case.defender.weapons = weapons(6);
            case.attacker.armor = 0.15;
            case.attacker.shield_deflection = 0.25;
            case.attacker.dodge = 0.1;
            case.attacker.hostile_mitigation_params = Some(raw_context(case.attacker_type, 6));
            case.defender.hostile_mitigation_params = Some(raw_context(case.defender_type, 6));
            case.attacker.isolytic_damage = 0.3;
            case.attacker.isolytic_defense = 0.2;
            case.attacker.apex_shred = 0.4;
            case.attacker.apex_barrier = 2_000.0;
            case.defender.isolytic_damage = 0.2;
            case.defender.isolytic_defense = 0.15;
            case.defender.apex_barrier = 3_000.0;
            Some(case)
        }
        "status_40_rounds" => {
            let mut case = base_case(
                CASE_NAMES[3],
                40,
                "Bounded status/condition/decay/accumulation/proc/on-hit/reactive workload on both sides.",
            );
            case.attacker.hull_health = 100_000_000.0;
            case.defender.hull_health = 100_000_000.0;
            case.attacker.weapons = weapons(3);
            case.defender.weapons = weapons(3);
            case.attacker.hostile_mitigation_params = Some(raw_context(case.attacker_type, 3));
            case.defender.hostile_mitigation_params = Some(raw_context(case.defender_type, 3));
            case.crew.seats = vec![
                seat(
                    "morale",
                    TimingWindow::RoundStart,
                    AbilityEffect::Morale(0.8),
                ),
                seat(
                    "burn",
                    TimingWindow::AttackPhase,
                    AbilityEffect::Burning {
                        chance: 0.55,
                        duration_rounds: 3,
                    },
                ),
                seat(
                    "breach",
                    TimingWindow::AttackPhase,
                    AbilityEffect::HullBreach {
                        chance: 0.45,
                        duration_rounds: 3,
                        requires_critical: false,
                    },
                ),
                seat(
                    "assimilate",
                    TimingWindow::AttackPhase,
                    AbilityEffect::Assimilated {
                        chance: 0.35,
                        duration_rounds: 2,
                    },
                ),
                seat(
                    "decay",
                    TimingWindow::RoundStart,
                    AbilityEffect::DecayingAttackMultiplier {
                        initial: 1.6,
                        decay_per_round: 0.02,
                        floor: 1.0,
                    },
                ),
                seat(
                    "accumulate",
                    TimingWindow::RoundStart,
                    AbilityEffect::AccumulatingAttackMultiplier {
                        initial: 1.0,
                        growth_per_round: 0.02,
                        ceiling: 1.8,
                    },
                ),
                seat(
                    "proc",
                    TimingWindow::AttackPhase,
                    AbilityEffect::ProcAttackMultiplier {
                        chance: 0.4,
                        multiplier: 1.2,
                    },
                ),
                seat(
                    "timed-hit",
                    TimingWindow::AttackPhase,
                    AbilityEffect::OnHitTimedBonus {
                        crit_chance: 0.01,
                        cascade_damage: 0.01,
                        duration_rounds: 2,
                    },
                ),
                seat(
                    "crit-stack",
                    TimingWindow::AttackPhase,
                    AbilityEffect::OnHitCritDamageStack {
                        bonus: 0.03,
                        duration_rounds: 2,
                    },
                ),
                seat(
                    "hull-repair",
                    TimingWindow::RoundStart,
                    AbilityEffect::HullRegenPrevRoundFraction(0.2),
                ),
                seat(
                    "reactive-shield",
                    TimingWindow::ReceiveDamage,
                    AbilityEffect::ShieldRegen(120.0),
                ),
                conditional(
                    seat(
                        "morale-defense",
                        TimingWindow::RoundStart,
                        AbilityEffect::BaseMitigationStatsBonus(0.15),
                    ),
                    AbilityCondition::MoraleActive,
                ),
                conditional(
                    seat(
                        "burning-attack",
                        TimingWindow::AttackPhase,
                        AbilityEffect::AttackMultiplier(0.1),
                    ),
                    AbilityCondition::DefenderBurning,
                ),
                conditional(
                    seat(
                        "breach-crit",
                        TimingWindow::AttackPhase,
                        AbilityEffect::CritDamageMultiplier(1.1),
                    ),
                    AbilityCondition::DefenderHullBreach,
                ),
                conditional(
                    seat(
                        "early-shots",
                        TimingWindow::RoundStart,
                        AbilityEffect::ShotsBonus {
                            chance: 0.7,
                            bonus_pct: 0.25,
                            duration_rounds: 2,
                        },
                    ),
                    AbilityCondition::RoundRange { min: 1, max: 20 },
                ),
                conditional(
                    seat(
                        "late-defense",
                        TimingWindow::RoundStart,
                        AbilityEffect::BaseMitigationStatsAdd(30.0),
                    ),
                    AbilityCondition::RoundRange { min: 21, max: 40 },
                ),
            ];
            case.defender_crew.seats = vec![
                seat(
                    "defender-burn",
                    TimingWindow::AttackPhase,
                    AbilityEffect::Burning {
                        chance: 0.4,
                        duration_rounds: 2,
                    },
                ),
                seat(
                    "defender-proc",
                    TimingWindow::AttackPhase,
                    AbilityEffect::ProcPierceBonus {
                        chance: 0.35,
                        bonus: 0.03,
                    },
                ),
                seat(
                    "defender-reactive",
                    TimingWindow::ReceiveDamage,
                    AbilityEffect::HullRegen(200.0),
                ),
                seat(
                    "defender-hit-stack",
                    TimingWindow::AttackPhase,
                    AbilityEffect::DefenderOnHitStack {
                        stat: DefenderOnHitStat::WeaponDamage,
                        per_hit: 0.002,
                        duration_rounds: 2,
                        requires: DefenderOnHitGate::Always,
                    },
                ),
            ];
            Some(case)
        }
        _ => None,
    }
}

fn catalog_case() -> Result<Case, String> {
    // Static registry/LCARS APIs intentionally exclude profiles and sync state.
    // The production scenario builder is crate-private, so this is explicitly a
    // profile-free engine workload, not an end-to-end optimizer measurement.
    let registry = DataRegistry::load().map_err(|_| "Static public catalog load failed")?;
    let ship = registry
        .resolve_ship_with_tier_level("uss_enterprise_d", Some(12), Some(60))
        .ok_or("Public Enterprise-D tier12/level60 did not resolve")?;
    let hostile = registry
        .resolve_hostile("1140710508")
        .ok_or("Public CORVUS did not resolve")?;
    let officers = registry
        .lcars_officers()
        .ok_or("Public LCARS catalog did not resolve")?;
    let by_id = index_lcars_officers_by_id(officers.to_vec());
    let captain = "kirk-1323b6";
    let bridge = vec!["spock-c04738".to_owned(), "gorkon-b8d2e7".to_owned()];
    for id in std::iter::once(captain).chain(bridge.iter().map(String::as_str)) {
        if !by_id.contains_key(id) {
            return Err("Selected public LCARS officer is absent".into());
        }
    }
    let buffs = resolve_crew_to_buff_set(
        captain,
        &bridge,
        &[],
        &by_id,
        &ResolveOptions {
            tier: Some(3),
            officer_tiers: None,
            officer_levels: Some(HashMap::from([
                (captain.into(), 15),
                (bridge[0].clone(), 15),
                (bridge[1].clone(), 15),
            ])),
        },
    );
    if buffs.crew.seats.is_empty() {
        return Err("Public LCARS crew resolved no dynamic seats".into());
    }
    let mut case = base_case(CASE_NAMES[4], 100, "Actual Enterprise-D tier12/level60, CORVUS52, rank3/level15 Kirk/Spock/Gorkon; profile-free public LCARS and hull abilities.");
    case.expected_rounds = None;
    case.minimum_rounds = 2;
    case.attacker_type = ship.ship_type();
    case.defender_type = hostile.ship_type_for_combat();
    case.defender_faction = hostile.opponent_faction_tag();
    let (hull, shield) = ship.health_with_launch_bonuses();
    case.attacker = buffs.apply_static_buffs_to_combatant(Combatant {
        id: ship.id.clone(),
        attack: ship.attack,
        mitigation: 0.0,
        pierce: pierce_damage_through_bonus(
            hostile.to_defender_stats(),
            ship.to_attacker_stats(),
            case.defender_type,
        ),
        crit_chance: ship.crit_chance,
        crit_multiplier: ship.combat_crit_multiplier(),
        proc_chance: buffs.proc_chance,
        proc_multiplier: buffs.proc_multiplier.max(1.0),
        hull_health: hull,
        shield_health: shield,
        shield_mitigation: ship.shield_mitigation.unwrap_or(0.8),
        isolytic_damage: ship.isolytic_damage,
        apex_shred: ship.apex_shred,
        weapons: ship.to_weapons(),
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: ship.to_defender_stats(),
            weapon_pierce_defender_stats: None,
            base_attacker_stats: hostile.to_attacker_stats(),
            base_attacker_weapon_stats: hostile.counter_weapon_attacker_stats(),
            ship_type: case.attacker_type,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Combatant::default()
    });
    case.defender = Combatant {
        id: hostile.id.clone(),
        attack: hostile.scalar_attack_fallback(),
        mitigation: mitigation_for_hostile(
            hostile.to_defender_stats(),
            ship.to_attacker_stats(),
            case.defender_type,
            hostile.mystery_mitigation_factor.unwrap_or(0.0),
            hostile.mitigation_floor.unwrap_or(0.16),
            hostile.mitigation_ceiling.unwrap_or(0.72),
        ),
        pierce: hostile
            .counter_pierce_damage_through_bonus(case.attacker_type, ship.to_defender_stats()),
        crit_chance: hostile.crit_chance,
        crit_multiplier: hostile.combat_crit_multiplier(),
        hull_health: hostile.hull_health,
        shield_health: hostile.shield_health,
        shield_mitigation: hostile.shield_mitigation.unwrap_or(0.8),
        apex_barrier: hostile.apex_barrier,
        isolytic_defense: hostile.isolytic_defense,
        weapons: hostile.weapons_for_counter_attack(case.attacker_type, ship.to_defender_stats()),
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: hostile.to_defender_stats(),
            weapon_pierce_defender_stats: None,
            base_attacker_stats: ship.to_attacker_stats(),
            base_attacker_weapon_stats: Vec::new(),
            ship_type: case.defender_type,
            mystery_mitigation_factor: hostile.mystery_mitigation_factor.unwrap_or(0.0),
            floor: hostile.mitigation_floor.unwrap_or(0.16),
            ceiling: hostile.mitigation_ceiling.unwrap_or(0.72),
        }),
        ..Combatant::default()
    };
    case.crew = buffs.crew;
    case.crew.seats.extend(ship_abilities_to_crew_seat_contexts(
        ship.abilities.as_deref().unwrap_or(&[]),
    ));
    case.defender_crew = hostile_abilities_to_defender_crew(
        &hostile.ability,
        hostile_ability_catalog_for_default_path(),
        hostile.level,
    );
    case.config.defender_level = Some(hostile.level);
    case.config.defender_hull_faction_id = hostile.faction.as_ref().map_or(0, |f| f.id);
    case.config.defender_hostile_tag_mask = hostile.hostile_tag_mask();
    case.config.engagement_enemy_types = hostile.engagement_enemy_types_for_combat();
    case.config.attacker_roster_officer_ids =
        vec![captain.into(), bridge[0].clone(), bridge[1].clone()];
    case.metadata = json!({"profile": "none", "synthetic": false, "ship_tier": 12, "ship_level": 60, "captain": captain, "bridge": bridge, "rank": 3, "officer_level": 15, "coverage": "static public catalogs plus LCARS/hull effects; excludes profile bonuses and full scenario assembly"});
    Ok(case)
}

fn make_case(name: &str) -> Result<Case, String> {
    if let Some(case) = synthetic_case(name) {
        return Ok(case);
    }
    if name == CASE_NAMES[4] {
        return catalog_case();
    }
    Err("Unknown case; use --list".into())
}

fn result_bits(result: &SimulationResult) -> [u64; 7] {
    [
        result.total_damage.to_bits(),
        result.total_isolytic_damage.to_bits(),
        result.attacker_hull_remaining.to_bits(),
        result.attacker_shield_remaining.to_bits(),
        result.defender_hull_remaining.to_bits(),
        result.defender_shield_remaining.to_bits(),
        u64::from(result.rounds_simulated),
    ]
}

fn mix_result(mut checksum: u64, result: &SimulationResult) -> u64 {
    for value in result_bits(result) {
        checksum = checksum.rotate_left(9) ^ value;
        checksum = checksum.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    checksum
        ^ u64::from(result.attacker_won)
        ^ (u64::from(result.winner_by_round_limit) << 1)
        ^ (u64::from(result.conqueror_borg_beam_suppression) << 2)
}

#[derive(Clone, Copy, Default)]
struct Probe {
    rounds: u32,
    outbound_hits: u64,
    inbound_hits: u64,
    won: bool,
    round_limit: bool,
}

fn validate(
    case: &Case,
    off: &PreCombatSetup,
    events: &PreCombatSetup,
) -> Result<[Probe; PANEL_LEN], String> {
    let mut probes = [Probe::default(); PANEL_LEN];
    for (index, seed) in SEEDS.iter().enumerate() {
        let plain = simulate_combat_from_setup(off, *seed);
        let traced = simulate_combat_from_setup(events, *seed);
        if result_bits(&plain)[..6]
            .iter()
            .any(|bits| !f64::from_bits(*bits).is_finite())
            || result_bits(&traced)[..6]
                .iter()
                .any(|bits| !f64::from_bits(*bits).is_finite())
        {
            return Err(format!(
                "{}: nonfinite combat result for seed {seed}",
                case.name
            ));
        }
        if result_bits(&plain) != result_bits(&traced)
            || plain.attacker_won != traced.attacker_won
            || plain.winner_by_round_limit != traced.winner_by_round_limit
            || plain.conqueror_borg_beam_suppression != traced.conqueror_borg_beam_suppression
        {
            return Err(format!(
                "{}: tracing changed outcome for seed {seed}",
                case.name
            ));
        }
        if plain.rounds_simulated < case.minimum_rounds
            || case
                .expected_rounds
                .is_some_and(|expected| expected != plain.rounds_simulated)
        {
            return Err(format!(
                "{}: expected {:?} rounds (minimum {}), got {} for seed {seed}",
                case.name, case.expected_rounds, case.minimum_rounds, plain.rounds_simulated
            ));
        }
        if case.expected_rounds == Some(case.config.rounds) && !plain.winner_by_round_limit {
            return Err(format!(
                "{}: long workload ended before its round-limit outcome",
                case.name
            ));
        }
        if case.name == CASE_NAMES[0] && !plain.attacker_won {
            return Err("Historical round-one case no longer wins in round one".into());
        }
        let outbound_hits = traced
            .events
            .iter()
            .filter(|event| event.event_type == "attack_roll")
            .count() as u64;
        let inbound_hits = traced
            .events
            .iter()
            .filter(|event| event.event_type == "crit_resolution" && event.phase == "counter")
            .count() as u64;
        if outbound_hits == 0 || (case.name != CASE_NAMES[0] && inbound_hits == 0) {
            return Err(format!(
                "{}: expected weapon activity was absent",
                case.name
            ));
        }
        probes[index] = Probe {
            rounds: plain.rounds_simulated,
            outbound_hits,
            inbound_hits,
            won: plain.attacker_won,
            round_limit: plain.winner_by_round_limit,
        };
    }
    Ok(probes)
}

fn write_json(output: &mut impl Write, row: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, row)?;
    writeln!(output)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--list" {
        println!(
            "{}",
            json!({"schema_version": 1, "cases": CASE_NAMES, "seed_panel_size": PANEL_LEN})
        );
        return Ok(());
    }
    let mut selected: Option<&str> = None;
    let mut parity = false;
    let mut count_allocations = false;
    let mut iterations: Option<u64> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--parity" if !parity => parity = true,
            "--allocations" if !count_allocations => count_allocations = true,
            "--case" if selected.is_none() => {
                index += 1;
                selected = Some(args.get(index).ok_or("--case needs a name")?);
            }
            "--iterations" if iterations.is_none() => {
                index += 1;
                iterations = Some(args.get(index).ok_or("--iterations needs a count")?.parse().map_err(|_| "Invalid iteration count")?);
            }
            _ => return Err("Usage: hot_path_bench --list | [--case NAME] [--iterations N] [--parity | --allocations]".into()),
        }
        index += 1;
    }
    if parity && iterations.is_some() {
        return Err("--iterations does not apply to --parity".into());
    }
    if parity && count_allocations {
        return Err("--allocations and --parity are separate modes".into());
    }
    if count_allocations && !allocations::AVAILABLE {
        return Err("--allocations requires --features bench-alloc-count".into());
    }
    if !count_allocations && !parity && allocations::AVAILABLE {
        return Err("Instrumented allocation builds cannot produce timing reports; use --allocations or --parity".into());
    }
    let iterations = iterations.unwrap_or(4096);
    if iterations == 0 || iterations > 1_000_000_000 {
        return Err("iterations must be 1..1000000000".into());
    }
    let names = selected.map_or_else(|| CASE_NAMES.to_vec(), |name| vec![name]);
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    for name in names {
        let case = make_case(name)?;
        let workload = case.workload();
        let workload_sha = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&workload).map_err(|_| "Cannot encode workload")?)
        );
        let off = case.prepared(TraceMode::Off);
        let events = case.prepared(TraceMode::Events);
        let probes = validate(&case, &off, &events)?;
        if parity {
            for (mode, setup) in [("off", &off), ("events", &events)] {
                let seeds = if mode == "events" {
                    &SEEDS[..4]
                } else {
                    &SEEDS[..]
                };
                for &seed in seeds {
                    let result = simulate_combat_from_setup(setup, seed);
                    if result_bits(&result)[..6]
                        .iter()
                        .any(|bits| !f64::from_bits(*bits).is_finite())
                    {
                        return Err("Nonfinite combat result cannot be used for parity".into());
                    }
                    let bytes = serde_json::to_vec(&result)
                        .map_err(|_| "Cannot serialize parity result")?;
                    write_json(&mut output, &json!({"schema_version": 1, "case": case.name, "workload_sha256": workload_sha, "seed": seed.to_string(), "trace_mode": mode, "floating_point_bits": result_bits(&result).map(|bits| format!("{bits:016x}")), "result_sha256": format!("{:x}", Sha256::digest(bytes)), "result_hash_scope": "entire serde_json serialized SimulationResult, including all fields and ordered events", "event_count": result.events.len(), "rounds_simulated": result.rounds_simulated})).map_err(|_| "Cannot write parity output")?;
                }
            }
        } else {
            let mut checksum = 0_u64;
            let mut rounds = 0_u64;
            let mut outbound_hits = 0_u64;
            let mut inbound_hits = 0_u64;
            let mut wins = 0_u64;
            let mut round_limits = 0_u64;
            let allocation_window = if count_allocations {
                Some(allocations::start()?)
            } else {
                None
            };
            let start = Instant::now();
            for iteration in 0..iterations {
                let seed_index = (iteration % PANEL_LEN as u64) as usize;
                let result = if case.include_setup {
                    let per_combat_setup = case.prepared(TraceMode::Off);
                    black_box(simulate_combat_from_setup(
                        black_box(&per_combat_setup),
                        black_box(SEEDS[seed_index]),
                    ))
                } else {
                    black_box(simulate_combat_from_setup(
                        black_box(&off),
                        black_box(SEEDS[seed_index]),
                    ))
                };
                checksum = mix_result(checksum, black_box(&result));
                rounds += u64::from(result.rounds_simulated);
                wins += u64::from(result.attacker_won);
                round_limits += u64::from(result.winner_by_round_limit);
                outbound_hits += probes[seed_index].outbound_hits;
                inbound_hits += probes[seed_index].inbound_hits;
            }
            let elapsed = start.elapsed();
            let allocation_counts = allocation_window.map(|window| window.finish());
            black_box(checksum);
            let mut row = json!({"schema_version": 1, "case": case.name, "workload_sha256": workload_sha, "workload": workload, "trace_mode": "off", "setup_timed": case.include_setup, "combats": iterations, "actual_rounds": rounds, "rounds_per_combat": rounds as f64 / iterations as f64, "outbound_hits": outbound_hits, "inbound_hits": inbound_hits, "hit_count_source": "untimed full Events probes for the identical seed panel; TraceOff outcome parity required", "wins": wins, "round_limit_outcomes": round_limits, "checksum": format!("{checksum:016x}"), "probe": {"combats": PANEL_LEN, "minimum_rounds": probes.iter().map(|p| p.rounds).min(), "maximum_rounds": probes.iter().map(|p| p.rounds).max(), "wins": probes.iter().filter(|p| p.won).count(), "round_limits": probes.iter().filter(|p| p.round_limit).count()}});
            if let Some(counts) = allocation_counts {
                row["mode"] = json!("instrumented_allocations_not_timing_data");
                row["allocations"] =
                    serde_json::to_value(counts).map_err(|_| "Cannot encode allocation counts")?;
            } else {
                row["mode"] = json!("release_timing_uninstrumented");
                row["elapsed_ns"] = json!(elapsed.as_nanos());
                row["elapsed_seconds"] = json!(elapsed.as_secs_f64());
                row["combats_per_second"] = json!(iterations as f64 / elapsed.as_secs_f64());
                row["actual_rounds_per_second"] = json!(rounds as f64 / elapsed.as_secs_f64());
            }
            write_json(&mut output, &row).map_err(|_| "Cannot write benchmark output")?;
        }
    }
    output
        .flush()
        .map_err(|_| "Cannot flush output".to_string())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
