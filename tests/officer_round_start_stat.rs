//! Phase 4d: `on_round_start` officer-stat rows (Kumak, Strike Team Una captain) and pending
//! `on_combat_start` gates (Dezoc bridge).

use kobayashi::combat::abilities::TimingWindow;
use kobayashi::combat::{
    build_combat_setup_with_officer_stat, simulate_combat_from_setup, Combatant, CrewConfiguration,
    CrewOfficerStatTotals, OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::data::officer_stat_round::OfficerStatRoundContext;
use kobayashi::data::profile::{OfficerStatConditionContext, PlayerProfile};
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

fn officer_stat_round_ctx(
    buff: &kobayashi::lcars::BuffSet,
    totals: CrewOfficerStatTotals,
    cond_ctx: OfficerStatConditionContext,
) -> Option<OfficerStatRoundContext> {
    let ship = test_battleship();
    OfficerStatRoundContext::try_from_ship_and_buffs(
        &ship,
        &PlayerProfile::default(),
        &buff.static_buffs,
        totals,
        totals,
        &buff.pending_officer_stat_contributions,
        &[],
        &buff.dynamic_officer_stat_contributions,
        cond_ctx,
    )
}

fn minimal_attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
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

#[test]
fn kumak_production_emits_round_start_officer_stat_dynamic_row() {
    let Some((officers, opts)) = bundled_officers() else {
        return;
    };
    let buff = resolve_crew_to_buff_set("kumak-c5b0db", &[], &[], &officers, &opts);
    assert!(
        !buff.static_buffs.contains_key("officer_stat_all"),
        "Kumak captain on_round_start must not use static_buffs"
    );
    let rows: Vec<_> = buff
        .dynamic_officer_stat_contributions
        .iter()
        .filter(|r| r.stat_key == "officer_stat_all")
        .collect();
    assert_eq!(rows.len(), 1, "expected one Kumak Discipline row");
    assert_eq!(rows[0].timing, TimingWindow::RoundStart);
    assert!(rows[0].runtime_condition.is_none());
}

#[test]
fn kumak_round_start_officer_stat_increases_damage() {
    let Some((officers, opts)) = bundled_officers() else {
        return;
    };
    let buff = resolve_crew_to_buff_set("kumak-c5b0db", &[], &[], &officers, &opts);
    let totals = CrewOfficerStatTotals {
        attack: 270.0,
        defense: 270.0,
        health: 270.0,
    };
    let Some(ctx) = officer_stat_round_ctx(&buff, totals, OfficerStatConditionContext::default())
    else {
        panic!("expected officer stat round context for Kumak");
    };

    let attacker = minimal_attacker();
    let defender = minimal_defender();
    let config = sim_config(4);

    let setup = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &buff.crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        Some(ctx),
    );
    let baseline = build_combat_setup_with_officer_stat(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
        None,
    );

    let with = simulate_combat_from_setup(&setup, config.seed);
    let without = simulate_combat_from_setup(&baseline, config.seed);
    assert!(
        with.total_damage > without.total_damage,
        "Kumak round-start officer stat should boost outbound damage (with={}, without={})",
        with.total_damage,
        without.total_damage
    );
}

#[test]
fn strike_team_una_captain_round_start_gated_on_pvp_explorer() {
    let Some((officers, opts)) = bundled_officers() else {
        return;
    };
    let buff = resolve_crew_to_buff_set("strike-team-una-5ec6f6", &[], &[], &officers, &opts);
    assert!(
        !buff
            .pending_officer_stat_contributions
            .iter()
            .any(|c| c.stat_key == "officer_stat_all"),
        "Una captain on_round_start must not use fight-setup pending"
    );
    let rows: Vec<_> = buff
        .dynamic_officer_stat_contributions
        .iter()
        .filter(|r| r.stat_key == "officer_stat_all")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].timing, TimingWindow::RoundStart);
    assert!(rows[0].runtime_condition.is_some());
}

#[test]
fn dezoc_bridge_armada_gate_stays_pending_not_round_start() {
    let Some((officers, opts)) = bundled_officers() else {
        return;
    };
    let buff = resolve_crew_to_buff_set("dezoc-381416", &[], &[], &officers, &opts);
    assert!(
        buff.pending_officer_stat_contributions
            .iter()
            .any(|c| c.stat_key == "officer_stat_all"),
        "Dezoc bridge on_combat_start gate should stay pending"
    );
    assert!(
        buff.dynamic_officer_stat_contributions
            .iter()
            .all(|r| r.stat_key != "officer_stat_all" || r.timing != TimingWindow::CombatBegin),
        "Dezoc bridge officer stat must not duplicate into dynamic round-start path"
    );
}
