//! Round-start `ShotsBonus` expiry: an entry earned in round N with `duration_rounds: d` is
//! active for exactly `d` rounds counting the earn round (`expires_round = N + d − 1`, the same
//! ring convention as Seska / `DefenderOnHitStack`). Before the 2026-07-10 fix the two
//! round-start push sites used `N + d`, so every round-start extra-shots proc fired one round
//! too long (attacker officers and hostile counter-fire alike).

use kobayashi::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext,
    TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, SimulationResult, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    build_officer_model_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

/// One round-start +1-shot proc (100% chance) that can only trigger in round 1.
fn round_one_shots_bonus_crew(duration_rounds: u32) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "round1_extra_shot".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::ShotsBonus {
                    chance: 1.0,
                    bonus_pct: 1.0,
                    duration_rounds,
                },
                condition: Some(AbilityCondition::RoundRange { min: 1, max: 1 }),
                weapon_scope: Default::default(),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    }
}

fn attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
        hull_health: 50_000.0,
        crit_multiplier: 1.0,
        proc_multiplier: 1.0,
        weapons: vec![WeaponStats {
            attack: 2_500.0,
            shots: None,
            ..Default::default()
        }],
        ..Combatant::default()
    }
}

fn defender() -> Combatant {
    Combatant {
        id: "def".into(),
        hull_health: 5_000_000.0,
        ..attacker()
    }
}

/// Cumulative outbound damage after `rounds` rounds with the given crew.
fn total_damage(crew: &CrewConfiguration, rounds: u32) -> f64 {
    let attacker = attacker();
    let defender = defender();
    let config = SimulationConfig {
        rounds,
        seed: 42,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    simulate_combat_from_setup(&setup, config.seed).total_damage
}

/// Outbound damage dealt in round N alone.
fn round_damage(crew: &CrewConfiguration, n: u32) -> f64 {
    total_damage(crew, n)
        - if n > 1 {
            total_damage(crew, n - 1)
        } else {
            0.0
        }
}

#[test]
fn one_round_shots_bonus_expires_after_its_earn_round() {
    let crew = round_one_shots_bonus_crew(1);
    let baseline = round_damage(&CrewConfiguration::default(), 1);
    let r1 = round_damage(&crew, 1);
    let r2 = round_damage(&crew, 2);
    assert!(
        (r1 - 2.0 * baseline).abs() < 1e-6,
        "round 1 must fire the extra shot (r1={r1}, baseline={baseline})"
    );
    assert!(
        (r2 - baseline).abs() < 1e-6,
        "a duration-1 bonus earned in round 1 must NOT fire in round 2 (r2={r2}, baseline={baseline})"
    );
}

#[test]
fn two_round_shots_bonus_covers_earn_round_plus_one() {
    let crew = round_one_shots_bonus_crew(2);
    let baseline = round_damage(&CrewConfiguration::default(), 1);
    let r2 = round_damage(&crew, 2);
    let r3 = round_damage(&crew, 3);
    assert!(
        (r2 - 2.0 * baseline).abs() < 1e-6,
        "a duration-2 bonus earned in round 1 must still fire in round 2 (r2={r2})"
    );
    assert!(
        (r3 - baseline).abs() < 1e-6,
        "a duration-2 bonus earned in round 1 must NOT fire in round 3 (r3={r3})"
    );
}

fn officer_crew(captain: &str, bridge: &[String], tier: u8) -> CrewConfiguration {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    resolve_crew_to_buff_set(
        captain,
        bridge,
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(tier),
            ..Default::default()
        },
    )
    .crew
}

fn shot_trace(
    attacker_crew: &CrewConfiguration,
    defender_crew: &CrewConfiguration,
    opponent: ShipType,
    is_npc: bool,
    trace_mode: TraceMode,
) -> SimulationResult {
    let ship = Combatant {
        hull_health: 100_000.0,
        weapons: vec![WeaponStats {
            attack: 100.0,
            shots: Some(2),
            ..Default::default()
        }],
        ..Default::default()
    };
    let config = SimulationConfig {
        rounds: 6,
        seed: 3,
        trace_mode,
        ..Default::default()
    };
    let setup = build_combat_setup(
        &ship,
        &ship,
        &config,
        attacker_crew,
        OpponentFactionTag::Unknown,
        opponent,
        ShipType::Explorer,
        is_npc,
        !is_npc,
        defender_crew,
    );
    simulate_combat_from_setup(&setup, config.seed)
}

fn shots_by_round(result: &SimulationResult, phase: &str) -> [usize; 6] {
    [1, 2, 3, 4, 5, 6].map(|round| {
        result
            .events
            .iter()
            .filter(|event| {
                event.round_index == round
                    && event.event_type == "crit_resolution"
                    && event.phase == phase
            })
            .count()
    })
}

#[test]
fn dajash_can_earn_fresh_overlapping_grants_after_round_two() {
    let crew = officer_crew("", &["dajash-tolra-1e809f".into()], 5);
    let empty = CrewConfiguration::default();
    let armada = shot_trace(&crew, &empty, ShipType::Armada, true, TraceMode::Events);
    assert_eq!(shots_by_round(&armada, "attack"), [4, 6, 6, 6, 6, 6]);
    assert_eq!(shots_by_round(&armada, "counter"), [2; 6]);
    let hostile = shot_trace(
        &crew,
        &empty,
        ShipType::Interceptor,
        true,
        TraceMode::Events,
    );
    assert_eq!(shots_by_round(&hostile, "attack"), [2; 6]);
    assert!(hostile
        .events
        .iter()
        .all(|e| e.event_type != "shots_bonus_trigger"));
}

#[test]
fn dajash_rolls_ranked_chance_once_per_round_with_trace_parity() {
    let crew = officer_crew("", &["dajash-tolra-1e809f".into()], 1);
    let empty = CrewConfiguration::default();
    let traced = shot_trace(&crew, &empty, ShipType::Armada, true, TraceMode::Events);
    let grants: Vec<_> = traced
        .events
        .iter()
        .filter(|event| event.event_type == "shots_bonus_trigger")
        .collect();
    let shots = shots_by_round(&traced, "attack");
    assert_eq!(grants.len(), shots.len());
    let mut previous = false;
    let mut successes = 0;
    for (i, grant) in grants.iter().enumerate() {
        assert_eq!(grant.round_index, i as u32 + 1);
        assert_eq!(grant.values["chance"].as_f64(), Some(0.30));
        let triggered = grant.values["triggered"].as_bool().unwrap();
        assert_eq!(triggered, grant.values["roll"].as_f64().unwrap() < 0.30);
        assert_eq!(
            shots[i],
            2 + 2 * (usize::from(previous) + usize::from(triggered))
        );
        successes += usize::from(triggered);
        previous = triggered;
    }
    assert!(
        (1..6).contains(&successes),
        "seed should exercise hits and misses"
    );
    let untraced = shot_trace(&crew, &empty, ShipType::Armada, true, TraceMode::Off);
    assert_eq!(traced.total_damage, untraced.total_damage);
    assert_eq!(
        traced.attacker_hull_remaining,
        untraced.attacker_hull_remaining
    );
}

#[test]
fn hierarch_renews_only_its_own_counter_fire_against_players() {
    let crew = officer_crew("the-hierarch-e1f430", &[], 1);
    let empty = CrewConfiguration::default();
    let pvp = shot_trace(
        &empty,
        &crew,
        ShipType::Interceptor,
        false,
        TraceMode::Events,
    );
    assert_eq!(shots_by_round(&pvp, "counter"), [4, 7, 7, 7, 7, 7]);
    assert_eq!(shots_by_round(&pvp, "attack"), [2; 6]);
    let hostile = shot_trace(
        &crew,
        &empty,
        ShipType::Interceptor,
        true,
        TraceMode::Events,
    );
    assert_eq!(shots_by_round(&hostile, "attack"), [2; 6]);
}

#[test]
fn miles_keeps_the_three_round_roll_window_and_uses_rank_chance() {
    let crew = officer_crew("", &["miles-o-brien-f0c92f".into()], 5);
    let armada = shot_trace(
        &crew,
        &CrewConfiguration::default(),
        ShipType::Armada,
        true,
        TraceMode::Events,
    );
    let grants: Vec<_> = armada
        .events
        .iter()
        .filter(|event| event.event_type == "shots_bonus_trigger")
        .map(|event| (event.round_index, event.values["chance"].as_f64()))
        .collect();
    assert_eq!(grants, [(1, Some(0.5)), (2, Some(0.5)), (3, Some(0.5))]);
}
