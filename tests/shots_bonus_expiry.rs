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
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
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
