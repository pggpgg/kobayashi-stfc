//! Phase 4d: Kirk captain `officerstatall` gated on `morale_active` — full 3-axis breakpoint path.
//!
//! Production sole case: `kirk-1323b6` captain "Leader" → dynamic officer-stat contribution
//! evaluated per round via [`OfficerStatRoundContext`].

use kobayashi::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CombatContext, TimingWindow,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::combat::{
    build_combat_setup_with_officer_stat, simulate_combat_from_setup, Combatant, CrewConfiguration,
    CrewOfficerStatTotals, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::officer_stat_round::OfficerStatRoundContext;
use kobayashi::data::profile::OfficerStatConditionContext;
use kobayashi::data::profile::PlayerProfile;
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

fn bundled_officers() -> Option<(
    std::collections::HashMap<String, kobayashi::lcars::LcarsOfficer>,
    ResolveOptions,
)> {
    let file = build_officer_model_file_default().ok()?;
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(1),
        officer_tiers: None,
        officer_levels: None,
    };
    Some((officers, opts))
}

fn resolve_kirk_buff_set() -> Option<kobayashi::lcars::BuffSet> {
    let (officers, opts) = bundled_officers()?;
    Some(resolve_crew_to_buff_set(
        "kirk-1323b6",
        &[],
        &[],
        &officers,
        &opts,
    ))
}

fn test_battleship() -> ShipRecord {
    ShipRecord {
        id: "test-battleship".into(),
        ship_class: "battleship".into(),
        armor: 1000.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        attack: 800.0,
        hull_health: 50_000.0,
        shield_health: 0.0,
        officer_bonus: OfficerBonusTable {
            attack: vec![
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
            ],
            defense: vec![
                OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                },
                OfficerBonusBreakpoint {
                    value: 100.0,
                    bonus: 0.1,
                },
                OfficerBonusBreakpoint {
                    value: 280.0,
                    bonus: 0.3,
                },
            ],
            health: vec![
                OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                },
                OfficerBonusBreakpoint {
                    value: 100.0,
                    bonus: 0.15,
                },
                OfficerBonusBreakpoint {
                    value: 280.0,
                    bonus: 0.45,
                },
            ],
        },
        ..ShipRecord::default()
    }
}

fn kirk_officer_stat_round(buff: &kobayashi::lcars::BuffSet) -> Option<OfficerStatRoundContext> {
    let ship = test_battleship();
    // Fixed totals so all three breakpoint tables cross together in synthetic tests.
    let totals = CrewOfficerStatTotals {
        attack: 200.0,
        defense: 200.0,
        health: 200.0,
    };
    let bridge = totals;
    OfficerStatRoundContext::try_from_ship_and_buffs(
        &ship,
        &PlayerProfile::default(),
        &buff.static_buffs,
        totals,
        bridge,
        &buff.pending_officer_stat_contributions,
        &[],
        &buff.dynamic_officer_stat_contributions,
        OfficerStatConditionContext::default(),
    )
}

fn minimal_attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
        mitigation: 0.0,
        armor: 1000.0,
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
        hull_health: 50_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 2_500.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn minimal_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        hull_health: 500_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        mitigation: 0.0,
        ..minimal_attacker()
    }
}

fn sim_config(rounds: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed: 42,
        trace_mode: TraceMode::Off,
        defender_level: Some(50),
        ..Default::default()
    }
}

fn morale_injector_seat() -> CrewSeatContext {
    CrewSeatContext {
        seat: CrewSeat::BelowDeck,
        ability: Ability {
            name: "morale_src".into(),
            class: AbilityClass::BelowDeck,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::Morale(1.0),
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    }
}

fn crew_with_optional_morale(
    buff: &kobayashi::lcars::BuffSet,
    inject_morale: bool,
) -> CrewConfiguration {
    let mut seats = buff.crew.seats.clone();
    if inject_morale {
        seats.push(morale_injector_seat());
    }
    CrewConfiguration { seats }
}

#[test]
fn kirk_production_phase4d_emits_dynamic_officer_stat_contribution() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };

    assert_eq!(
        buff.dynamic_officer_stat_contributions.len(),
        1,
        "expected one dynamic Leader contribution; got {:?}",
        buff.dynamic_officer_stat_contributions
    );
    let row = &buff.dynamic_officer_stat_contributions[0];
    assert_eq!(row.stat_key, "officer_stat_all");
    assert!((row.value - 0.4).abs() < 1e-9);
    assert_eq!(row.timing, TimingWindow::RoundStart);
    assert!(row.target_attacker);
    assert!(
        row.runtime_condition.as_ref().is_some_and(|c| matches!(
            c,
            AbilityCondition::MoraleActive | AbilityCondition::And(_)
        )),
        "Leader row should gate on MoraleActive (+ RoundRange); got {:?}",
        row.runtime_condition
    );

    assert!(
        buff.pending_officer_stat_contributions.is_empty(),
        "dynamic Kirk Leader must not duplicate via pending_officer_stat_contributions"
    );

    let attack_multiplier_seats = buff
        .crew
        .seats
        .iter()
        .filter(|s| matches!(s.ability.effect, AbilityEffect::AttackMultiplier(_)))
        .count();
    assert_eq!(
        attack_multiplier_seats, 0,
        "Phase 4d full path must not emit synthetic AttackMultiplier seats"
    );

    let morale_bridge_count = buff
        .crew
        .seats
        .iter()
        .filter(|s| matches!(s.ability.effect, AbilityEffect::Morale(_)))
        .count();
    assert_eq!(
        morale_bridge_count, 1,
        "Kirk bridge Inspirational should compile to one Morale seat"
    );
}

#[test]
fn kirk_leader_morale_gated_damage_requires_morale() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };
    let Some(osr_ctx) = kirk_officer_stat_round(&buff) else {
        panic!("expected officer stat round context for Kirk + test ship");
    };

    let attacker = minimal_attacker();
    let defender = minimal_defender();
    let config = sim_config(4);

    let setup_no_morale = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &crew_with_optional_morale(&buff, false),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        Some(osr_ctx.clone()),
    );
    let setup_with_morale = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &crew_with_optional_morale(&buff, true),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        Some(osr_ctx),
    );

    let without = simulate_combat_from_setup(&setup_no_morale, config.seed);
    let with = simulate_combat_from_setup(&setup_with_morale, config.seed);

    assert!(
        with.total_damage > without.total_damage,
        "Leader +40% breakpoint path should apply only when MoraleActive (with={}, without={})",
        with.total_damage,
        without.total_damage
    );
}

#[test]
fn kirk_leader_duration_round_range_gates_bonus_to_first_round() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };
    let Some(ctx) = kirk_officer_stat_round(&buff) else {
        panic!("expected officer stat round context");
    };

    let mut combat_ctx = CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_shield_pct: 1.0,
        attacker_morale_active: true,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction: OpponentFactionTag::Unknown,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        defender_hull_faction_id: 0,
        defender_ship_type: ShipType::Explorer,
        attacker_ship_type: ShipType::Battleship,
        attacker_ship_id: std::sync::Arc::from("att"),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: true,
        defender_hostile_tag_mask: 0,
        engagement_enemy_types: std::sync::Arc::new(Default::default()),
        combat_battle_type_id: None,
        defender_level: Some(50),
    };

    let active_r1 = ctx.delta_for_timing(&combat_ctx, TimingWindow::RoundStart);
    assert!(
        !active_r1.is_effectively_zero(),
        "round 1 + morale should apply Leader 3-axis bonus"
    );
    assert!(
        active_r1.attack_pre_mult_add.abs() > f64::EPSILON,
        "attack axis should cross a breakpoint when morale gates +40% officer stat all"
    );
    assert!(
        (active_r1.health_max_mult - 1.0).abs() > f64::EPSILON,
        "health axis should cross a breakpoint when morale gates +40% officer stat all"
    );
    assert!(
        active_r1.defense_armor_add.abs() > f64::EPSILON,
        "defense axis should cross a breakpoint on battleship when morale gates +40% officer stat all"
    );

    combat_ctx.round_index = 2;
    let active_r2 = ctx.delta_for_timing(&combat_ctx, TimingWindow::RoundStart);
    assert!(
        active_r2.is_effectively_zero(),
        "round 2 should not apply duration-1 Leader bonus even with morale"
    );
}

#[test]
fn kirk_leader_morale_gated_survivability_requires_morale_on_counter_fire() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };
    let Some(osr_ctx) = kirk_officer_stat_round(&buff) else {
        panic!("expected officer stat round context for Kirk + test ship");
    };

    let mut attacker = minimal_attacker();
    attacker.hull_health = 80_000.0;
    attacker.armor = 0.0;
    let mut defender = minimal_defender();
    defender.hull_health = 5_000_000.0;
    defender.mitigation = 0.0;
    let config = sim_config(1);

    let setup_no_morale = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &crew_with_optional_morale(&buff, false),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        Some(osr_ctx.clone()),
    );
    let setup_with_morale = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &crew_with_optional_morale(&buff, true),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        Some(osr_ctx),
    );

    let without = simulate_combat_from_setup(&setup_no_morale, config.seed);
    let with = simulate_combat_from_setup(&setup_with_morale, config.seed);

    assert!(
        with.attacker_hull_remaining > without.attacker_hull_remaining,
        "round-1 Leader health/defense axes should improve survivability under counter-fire (with={}, without={})",
        with.attacker_hull_remaining,
        without.attacker_hull_remaining
    );
}
