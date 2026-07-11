//! The player's own burning tick (1% of max hull per round) is self-inflicted DOT. Before the
//! 2026-07-10 fix it was multiplied by the OUTBOUND apex factor (attacker shred vs defender
//! barrier), so a hostile with a large apex barrier wrongly suppressed the player's burn damage.

use kobayashi::combat::abilities::{
    Ability, AbilityClass, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::combat::types::MAX_COMBAT_ROUNDS;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};

/// Hostile seat that sets the player burning for the whole fight at combat begin
/// (Immolator-style; chance 1.0 keeps the roll deterministic).
fn burn_the_player_crew() -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: "immolator_style_burn".to_string(),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::Burning {
                    chance: 1.0,
                    duration_rounds: MAX_COMBAT_ROUNDS,
                },
                condition: None,
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
            attack: 100.0,
            shots: None,
            ..Default::default()
        }],
        ..Combatant::default()
    }
}

/// Toothless hostile (no counter damage) with an enormous apex barrier.
fn defender(apex_barrier: f64) -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 0.0,
        hull_health: 5_000_000.0,
        apex_barrier,
        weapons: vec![],
        ..attacker()
    }
}

fn attacker_hull_after(rounds: u32, apex_barrier: f64) -> f64 {
    let attacker = attacker();
    let defender = defender(apex_barrier);
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
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &burn_the_player_crew(),
    );
    simulate_combat_from_setup(&setup, config.seed).attacker_hull_remaining
}

#[test]
fn burning_player_takes_full_tick_regardless_of_hostile_apex_barrier() {
    // 1% of 50k max hull per round for 3 rounds = 1500 hull, whatever the hostile's barrier.
    let expected = 50_000.0 - 3.0 * 0.01 * 50_000.0;
    let no_barrier = attacker_hull_after(3, 0.0);
    let huge_barrier = attacker_hull_after(3, 1e9);
    assert!(
        (no_barrier - expected).abs() < 1e-6,
        "burn tick without barrier: expected {expected}, got {no_barrier}"
    );
    assert!(
        (huge_barrier - expected).abs() < 1e-6,
        "hostile apex barrier must not suppress the player's own burn tick: expected {expected}, got {huge_barrier}"
    );
}
