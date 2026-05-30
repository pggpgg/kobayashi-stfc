//! Phase 4d (attack-axis v1): Kirk captain `officerstatall` gated on `morale_active`.
//!
//! Production sole case: `kirk-1323b6` captain "Leader" → synthetic `AttackMultiplier`
//! seat via [`expand_dynamic_officer_stat_effects`]. Defense/Health axes are not modeled.

use std::path::Path;

use kobayashi::combat::abilities::{
    filter_effects_by_condition, Ability, AbilityClass, AbilityCondition, AbilityEffect,
    CombatContext, CrewSeat, CrewSeatContext, TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    index_lcars_officers_by_id, load_lcars_file, resolve_crew_to_buff_set, ResolveOptions,
};

fn bundled_officers() -> Option<(
    std::collections::HashMap<String, kobayashi::lcars::LcarsOfficer>,
    ResolveOptions,
)> {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return None;
    }
    let file = load_lcars_file(path).ok()?;
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

fn kirk_leader_attack_seats(
    buff: &kobayashi::lcars::BuffSet,
) -> Vec<&CrewSeatContext> {
    buff.crew
        .seats
        .iter()
        .filter(|s| {
            s.ability.timing == TimingWindow::RoundStart
                && matches!(s.ability.effect, AbilityEffect::AttackMultiplier(_))
        })
        .collect()
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

fn crew_leader_only(buff: &kobayashi::lcars::BuffSet) -> CrewConfiguration {
    let seats: Vec<_> = kirk_leader_attack_seats(buff)
        .into_iter()
        .cloned()
        .collect();
    assert!(
        !seats.is_empty(),
        "expected Kirk Leader AttackMultiplier seat in buff set"
    );
    CrewConfiguration { seats }
}

fn crew_leader_with_guaranteed_morale(buff: &kobayashi::lcars::BuffSet) -> CrewConfiguration {
    let mut seats: Vec<_> = kirk_leader_attack_seats(buff)
        .into_iter()
        .cloned()
        .collect();
    seats.push(morale_injector_seat());
    CrewConfiguration { seats }
}

fn active_leader_effects(buff: &kobayashi::lcars::BuffSet) -> Vec<kobayashi::combat::ActiveAbilityEffect> {
    kirk_leader_attack_seats(buff)
        .into_iter()
        .map(|s| kobayashi::combat::ActiveAbilityEffect {
            ability_name: s.ability.name.clone(),
            officer_id: s.officer_id.clone(),
            effect: s.ability.effect,
            boosted: s.boosted,
            condition: s.ability.condition.clone(),
        })
        .collect()
}

#[test]
fn kirk_production_phase4d_emits_single_attack_multiplier_seat() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };

    let attack_seats = kirk_leader_attack_seats(&buff);
    assert_eq!(
        attack_seats.len(),
        1,
        "expected one synthetic Leader AttackMultiplier seat; got {:?}",
        buff.crew
            .seats
            .iter()
            .map(|s| (s.ability.name.as_str(), format!("{:?}", s.ability.effect)))
            .collect::<Vec<_>>()
    );

    let seat = attack_seats[0];
    assert!(
        matches!(
            seat.ability.condition,
            Some(AbilityCondition::MoraleActive)
                | Some(AbilityCondition::And(_))
        ),
        "Leader seat should gate on MoraleActive (possibly AND RoundRange); got {:?}",
        seat.ability.condition
    );
    if let Some(AbilityEffect::AttackMultiplier(v)) = Some(seat.ability.effect) {
        assert!(
            (v - 0.4).abs() < 1e-9,
            "rank-1 Kirk Leader should be +40% attack mult, got {v}"
        );
    } else {
        panic!("expected AttackMultiplier effect");
    }

    assert!(
        buff.pending_officer_stat_contributions.is_empty(),
        "dynamic Kirk Leader must not duplicate via pending_officer_stat_contributions"
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

    let attacker = minimal_attacker();
    let defender = minimal_defender();
    let config = sim_config(4);

    let setup_no_morale = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew_leader_only(&buff),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let setup_with_morale = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew_leader_with_guaranteed_morale(&buff),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );

    let without = simulate_combat_from_setup(&setup_no_morale, config.seed);
    let with = simulate_combat_from_setup(&setup_with_morale, config.seed);

    assert!(
        with.total_damage > without.total_damage,
        "Leader +40% should apply only when MoraleActive (with={}, without={})",
        with.total_damage,
        without.total_damage
    );
}

#[test]
fn kirk_leader_duration_round_range_gates_bonus_to_first_round() {
    let Some(buff) = resolve_kirk_buff_set() else {
        return;
    };

    let effects = active_leader_effects(&buff);
    assert_eq!(effects.len(), 1);

    let mut ctx = CombatContext {
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
        attacker_ship_id: "att".into(),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: true,
        defender_hostile_tag_mask: 0,
        engagement_enemy_types: Default::default(),
        combat_battle_type_id: None,
        defender_level: Some(50),
    };

    assert_eq!(
        filter_effects_by_condition(&effects, &ctx).len(),
        1,
        "round 1 + morale should apply Leader bonus"
    );

    ctx.round_index = 2;
    assert!(
        filter_effects_by_condition(&effects, &ctx).is_empty(),
        "round 2 should not apply duration-1 Leader bonus even with morale"
    );
}
