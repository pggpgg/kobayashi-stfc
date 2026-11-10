//! Native deterministic RoundStart opponent subtraction, independent of saved profiles.

use kobayashi::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, TimingWindow,
};
use kobayashi::combat::types::{OpponentFactionTag, ShipType};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
    CrewSeat, CrewSeatContext, SimulationConfig, SimulationResult, TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, HostileAbilityCatalog,
};

fn compiled_hostile(
    timing: &str,
    chance: f64,
    value: f64,
    duration: Option<u32>,
) -> CrewConfiguration {
    let catalog: HostileAbilityCatalog = serde_json::from_value(serde_json::json!({
        "entries":{"1":{
            "timing":timing,
            "effect_type":"hostile_cumulative_opponent_shield_mitigation_debuff",
            "value_is_percentage":false,"duration_rounds":duration
        }}
    }))
    .unwrap();
    hostile_abilities_to_defender_crew(
        &[
            serde_json::json!({"id":1,"value_is_percentage":true,"values":[{"chance":chance,"value":value}]}),
        ],
        Some(&catalog),
        62,
    )
}

fn seat(
    effect: AbilityEffect,
    timing: TimingWindow,
    condition: Option<AbilityCondition>,
) -> CrewSeatContext {
    CrewSeatContext::legacy(
        CrewSeat::Ship,
        Ability {
            name: "synthetic source".into(),
            class: AbilityClass::ShipAbility,
            timing,
            boostable: false,
            effect,
            condition,
            weapon_scope: Default::default(),
        },
        false,
    )
}

fn combatant(id: &str, shield_mitigation: f64) -> Combatant {
    Combatant {
        id: id.into(),
        hull_health: 1e8,
        shield_health: 1e8,
        shield_mitigation,
        // Multiple weapons and shots catch a subtraction first applied after
        // the opening shot, or accidentally accumulated per hit.
        weapons: vec![
            WeaponStats {
                attack: 1000.0,
                shots: Some(2),
                ..Default::default()
            };
            2
        ],
        ..Combatant::default()
    }
}

fn run(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    attacker_crew: &CrewConfiguration,
    defender_crew: &CrewConfiguration,
) -> SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        config,
        attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        defender_crew,
    )
}

#[test]
fn hostile_cumulative_shield_subtraction_validates_timing_chance_duration_and_units() {
    for chance in [1.0, 100.0] {
        let crew = compiled_hostile("round_start", chance, 0.01, None);
        assert_eq!(crew.seats.len(), 1);
        assert_eq!(crew.seats[0].ability.timing, TimingWindow::RoundStart);
        assert!(!crew.seats[0].ability.boostable);
        assert!(matches!(crew.seats[0].ability.effect,
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff{per_round,..} if per_round==0.01));
    }
    for (timing, chance, value, duration) in [
        ("combat_begin", 1.0, 0.01, None),
        ("round_end", 1.0, 0.01, None),
        ("round_start", 0.5, 0.01, None),
        ("round_start", 0.99, 0.01, None),
        ("round_start", 0.0, 0.01, None),
        ("round_start", 1.0, 0.0, None),
        ("round_start", 1.0, -0.01, None),
        ("round_start", 1.0, 0.01, Some(2)),
    ] {
        assert!(
            compiled_hostile(timing, chance, value, duration)
                .seats
                .is_empty(),
            "unsupported source {timing}/{chance}/{value}/{duration:?}"
        );
    }
}

#[test]
fn hostile_round_start_shield_subtraction_composes_before_first_counter_and_keeps_opposite_direction(
) {
    let attacker = combatant("player", 0.8);
    let defender = combatant("hostile", 0.8);
    let hostile = compiled_hostile("round_start", 1.0, 0.01, None);
    let player = CrewConfiguration {
        seats: vec![
            seat(
                AbilityEffect::ShieldMitigationBonus(-0.025),
                TimingWindow::CombatBegin,
                Some(AbilityCondition::RoundRange { min: 1, max: 2 }),
            ),
            seat(
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff {
                    per_round: 0.17,
                    cap: 1.0,
                },
                TimingWindow::RoundStart,
                None,
            ),
        ],
    };
    for seed in [1, 7, 42] {
        let mut prior_hull = attacker.hull_health;
        let mut prior_shield = attacker.shield_health;
        for (round, expected_incoming, expected_outgoing) in
            [(1, 0.815, 0.605), (2, 0.805, 0.435), (3, 0.77, 0.29)]
        {
            let config = SimulationConfig {
                rounds: round,
                seed,
                trace_mode: TraceMode::Events,
                incoming_shield_mitigation_bonus: 0.025,
                incoming_shield_mitigation_bonus_rounds: 2,
                ..Default::default()
            };
            let result = run(&attacker, &defender, &config, &player, &hostile);
            let hull = prior_hull - result.attacker_hull_remaining;
            let shield = prior_shield - result.attacker_shield_remaining;
            assert!(
                (shield / (hull + shield) - expected_incoming).abs() < 1e-9,
                "round{round}: hostile subtraction must precede every counter shot"
            );
            let outgoing: Vec<_> = result
                .events
                .iter()
                .filter(|e| e.round_index == round && e.event_type == "damage_application")
                .collect();
            assert_eq!(outgoing.len(), 4);
            assert!(
                outgoing
                    .iter()
                    .all(
                        |e| (e.values["shield_mitigation"].as_f64().unwrap() - expected_outgoing)
                            .abs()
                            < 1e-12
                    ),
                "hostile subtraction must not leak into the opposite Quantum/Immortal pool"
            );
            let off = run(
                &attacker,
                &defender,
                &SimulationConfig {
                    trace_mode: TraceMode::Off,
                    ..config
                },
                &player,
                &hostile,
            );
            assert_eq!(result.attacker_hull_remaining, off.attacker_hull_remaining);
            assert_eq!(
                result.attacker_shield_remaining,
                off.attacker_shield_remaining
            );
            assert_eq!(result.total_damage, off.total_damage);
            prior_hull = result.attacker_hull_remaining;
            prior_shield = result.attacker_shield_remaining;
        }
    }
}

#[test]
fn hostile_round_start_subtraction_clamps_after_own_bonus_and_respects_owner_orientation() {
    let attacker = combatant("first", 0.99);
    let defender = combatant("second", 0.8);
    let source = compiled_hostile("round_start", 1.0, 0.01, None);
    let config = SimulationConfig {
        rounds: 1,
        seed: 91,
        trace_mode: TraceMode::Events,
        incoming_shield_mitigation_bonus: 0.025,
        incoming_shield_mitigation_bonus_rounds: 2,
        ..Default::default()
    };
    let defender_owned = run(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        &source,
    );
    assert_eq!(
        defender_owned.attacker_hull_remaining, attacker.hull_health,
        ".99+.025-.01=1.005 clamps to1; premature clamp would incorrectly produce.99"
    );
    assert!(defender_owned.attacker_shield_remaining < attacker.shield_health);
    let attacker_owned = run(
        &attacker,
        &defender,
        &config,
        &source,
        &CrewConfiguration::default(),
    );
    let outgoing: Vec<_> = attacker_owned
        .events
        .iter()
        .filter(|e| e.event_type == "damage_application")
        .collect();
    assert_eq!(outgoing.len(), 4);
    assert!(outgoing
        .iter()
        .all(|e| (e.values["shield_mitigation"].as_f64().unwrap() - 0.79).abs() < 1e-12));
    assert_eq!(
        attacker_owned.attacker_hull_remaining, attacker.hull_health,
        "attacker-owned subtraction affects only the other side, not own KSG shield pool"
    );
}

#[test]
fn inactive_hostile_round_start_subtraction_preserves_existing_results() {
    let attacker = combatant("player", 0.8);
    let defender = combatant("hostile", 0.8);
    let mut gated = compiled_hostile("round_start", 1.0, 0.01, None);
    gated.seats[0].ability.condition =
        Some(AbilityCondition::AttackerShipTypeIs(ShipType::Interceptor));
    for seed in [1, 7, 42] {
        let config = SimulationConfig {
            rounds: 3,
            seed,
            ..Default::default()
        };
        let baseline = run(
            &attacker,
            &defender,
            &config,
            &CrewConfiguration::default(),
            &CrewConfiguration::default(),
        );
        assert_eq!(
            baseline,
            run(
                &attacker,
                &defender,
                &config,
                &CrewConfiguration::default(),
                &gated
            )
        );
        let unsupported = compiled_hostile("round_start", 0.5, 0.01, None);
        assert_eq!(
            baseline,
            run(
                &attacker,
                &defender,
                &config,
                &CrewConfiguration::default(),
                &unsupported
            )
        );
    }
}

#[test]
fn hostile_cumulative_source_has_no_arbitrary_cap_before_overfilled_pool_composition() {
    let attacker = combatant("overfilled player", 0.99);
    let defender = combatant("hostile", 0.8);
    // Synthetic strength makes the distinction visible in three rounds:
    // raw own pool1.59 minus .6/1.2/1.8 => .99/.39/0 after final clamping.
    let source = compiled_hostile("round_start", 1.0, 0.6, None);
    let mut prior_hull = attacker.hull_health;
    let mut prior_shield = attacker.shield_health;
    for (round, expected) in [(1, 0.99), (2, 0.39), (3, 0.0)] {
        let config = SimulationConfig {
            rounds: round,
            seed: 9,
            incoming_shield_mitigation_bonus: 0.6,
            incoming_shield_mitigation_bonus_rounds: 3,
            ..Default::default()
        };
        let result = run(
            &attacker,
            &defender,
            &config,
            &CrewConfiguration::default(),
            &source,
        );
        let hull = prior_hull - result.attacker_hull_remaining;
        let shield = prior_shield - result.attacker_shield_remaining;
        assert!(
            (shield / (hull + shield) - expected).abs() < 1e-9,
            "round{round}: only final domain clamping may saturate the source"
        );
        prior_hull = result.attacker_hull_remaining;
        prior_shield = result.attacker_shield_remaining;
    }
}

#[test]
fn hostile_native_families_reuse_isolytic_bonus_and_physical_immunity_without_affecting_counter_fire(
) {
    let catalog: HostileAbilityCatalog = serde_json::from_value(serde_json::json!({"entries":{
        "2":{"timing":"combat_begin","effect_type":"hostile_isolytic_vulnerability"},
        "3":{"timing":"combat_begin","effect_type":"isolytic_damage","value_is_percentage":false}
    }}))
    .unwrap();
    let raw = vec![
        serde_json::json!({"id":2,"values":[{"chance":1,"value":1}]}),
        serde_json::json!({"id":3,"values":[{"chance":1,"value":14.91}]}),
    ];
    let hostile = hostile_abilities_to_defender_crew(&raw, Some(&catalog), 62);
    assert_eq!(hostile.seats.len(), 2);
    assert!(hostile
        .seats
        .iter()
        .any(|s| s.ability.effect == AbilityEffect::HostileIsolyticVulnerability));
    assert!(hostile
        .seats
        .iter()
        .any(|s| s.ability.effect == AbilityEffect::IsolyticDamageBonus(14.91)));
    let config = SimulationConfig {
        rounds: 1,
        seed: 1,
        ..Default::default()
    };
    let defender = combatant("hostile", 0.8);
    let physical = combatant("physical player", 0.8);
    let no_iso = run(
        &physical,
        &defender,
        &config,
        &CrewConfiguration::default(),
        &hostile,
    );
    assert_eq!(
        no_iso.total_damage, 0.0,
        "physical shots cannot deplete immune target HP"
    );
    let mut isolytic = physical.clone();
    isolytic.isolytic_damage = 0.5;
    let with_iso = run(
        &isolytic,
        &defender,
        &config,
        &CrewConfiguration::default(),
        &hostile,
    );
    assert!(with_iso.total_damage > 0.0);
    assert!((with_iso.total_damage - with_iso.total_isolytic_damage).abs() < 1e-9);
    assert_eq!(
        with_iso.attacker_hull_remaining, no_iso.attacker_hull_remaining,
        "target immunity does not suppress its own counter fire"
    );
    assert_eq!(
        with_iso.attacker_shield_remaining,
        no_iso.attacker_shield_remaining
    );
}
