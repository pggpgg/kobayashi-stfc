//! Source-bound critical effects must preserve their owner and trigger.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, ShipType, SimulationConfig,
    SimulationResult, TimingWindow, TraceMode, WeaponStats, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::lcars::{
    build_officer_model_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

fn ship(crit_chance: f64) -> Combatant {
    Combatant {
        hull_health: 10_000.0,
        crit_chance,
        crit_multiplier: 2.0,
        weapons: vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(2),
                ..Default::default()
            };
            2
        ],
        ..Default::default()
    }
}

fn simulate(
    attacker: &Combatant,
    attacker_crew: &CrewConfiguration,
    defender_crew: &CrewConfiguration,
    pvp: bool,
    trace_mode: TraceMode,
) -> SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        &ship(1.0),
        &SimulationConfig {
            rounds: 2,
            seed: 3,
            defender_level: Some(50),
            trace_mode,
            ..Default::default()
        },
        attacker_crew,
        Default::default(),
        ShipType::Interceptor,
        ShipType::Interceptor,
        !pvp,
        pvp,
        defender_crew,
    )
}

fn with_breach(mut crew: CrewConfiguration) -> CrewConfiguration {
    crew.seats.push(CrewSeatContext {
        seat: CrewSeat::Captain,
        ability: Ability {
            weapon_scope: Default::default(),
            name: "Guaranteed Hull Breach".into(),
            class: AbilityClass::CaptainManeuver,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::HullBreach {
                chance: 1.0,
                duration_rounds: 2,
                requires_critical: false,
            },
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
    crew
}

fn crit_values(result: &SimulationResult, phase: &str, value: &str) -> Vec<f64> {
    result
        .events
        .iter()
        .filter(|event| event.event_type == "crit_resolution" && event.phase == phase)
        .map(|event| event.values[value].as_f64().unwrap())
        .collect()
}

#[test]
fn damar_does_not_change_the_opening_critical_roll() {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    let options = ResolveOptions {
        tier: Some(5),
        ..Default::default()
    };
    let damar = resolve_crew_to_buff_set("damar-796eb0", &[], &[], &officers, &options).crew;
    let empty = CrewConfiguration::default();
    let breach_damar = with_breach(damar.clone());

    let no_base_crit = ship(0.0);
    let result = simulate(
        &no_base_crit,
        &breach_damar,
        &empty,
        true,
        TraceMode::Events,
    );
    let odds = crit_values(&result, "attack", "effective_crit_chance");
    assert_eq!(
        odds,
        vec![0.0; 8],
        "Damar cannot grant crit chance before a crit"
    );

    assert_diagnostic(&damar, AbilityClass::CaptainManeuver, 0.02);
}

#[test]
fn damar_does_not_increase_friendly_critical_damage_in_pvp() {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    let damar = resolve_crew_to_buff_set(
        "",
        &["damar-796eb0".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let empty = CrewConfiguration::default();
    let breach_control = with_breach(CrewConfiguration::default());
    let breach_damar = with_breach(damar.clone());

    // Damar's bridge row reduces the breached opponent's crit damage after a
    // qualifying weapon. An unresolved proc must never buff the owner's shots.
    let guaranteed_crit = ship(1.0);
    let control = simulate(
        &guaranteed_crit,
        &breach_control,
        &empty,
        true,
        TraceMode::Events,
    );
    let result = simulate(
        &guaranteed_crit,
        &breach_damar,
        &empty,
        true,
        TraceMode::Events,
    );
    for phase in ["attack", "counter"] {
        let control_mult = crit_values(&control, phase, "multiplier");
        assert_eq!(control_mult.len(), 8);
        assert_eq!(
            crit_values(&result, phase, "multiplier"),
            control_mult,
            "{phase}"
        );
    }
    let defending = simulate(
        &guaranteed_crit,
        &breach_control,
        &breach_damar,
        true,
        TraceMode::Events,
    );
    let defending_control = simulate(
        &guaranteed_crit,
        &breach_control,
        &breach_control,
        true,
        TraceMode::Events,
    );
    assert_eq!(
        crit_values(&defending, "counter", "multiplier"),
        crit_values(&defending_control, "counter", "multiplier")
    );
    let mut untraced = simulate(
        &guaranteed_crit,
        &breach_damar,
        &empty,
        true,
        TraceMode::Off,
    );
    untraced.events.clone_from(&result.events);
    assert_eq!(untraced, result);
    assert_diagnostic(&damar, AbilityClass::BridgeAbility, 0.5);
}

fn assert_diagnostic(crew: &CrewConfiguration, class: AbilityClass, coefficient: f64) {
    assert!(crew.seats.iter().any(|seat| seat.ability.class == class && matches!(
        seat.ability.effect, AbilityEffect::UnsupportedOfficerEffect { coefficient: c, chance: 1.0 }
        if (c - coefficient).abs() < 1e-12
    )));
}

#[test]
fn gaila_does_not_increase_friendly_critical_damage_against_hostiles() {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    let gaila = resolve_crew_to_buff_set(
        "",
        &["gaila-3d387a".into()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    let attacker = ship(1.0);
    let empty = CrewConfiguration::default();
    let result = simulate(&attacker, &gaila.crew, &empty, false, TraceMode::Events);
    assert_eq!(crit_values(&result, "attack", "multiplier"), vec![2.0; 8]);
    assert_eq!(crit_values(&result, "counter", "multiplier"), vec![2.0; 8]);
    assert_diagnostic(&gaila.crew, AbilityClass::BridgeAbility, 0.5);
}

#[test]
fn unresolved_opponent_critical_damage_reductions_preserve_rank_and_seat() {
    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    let options = ResolveOptions {
        tier: Some(3),
        ..Default::default()
    };
    for (bridge, below, class, coefficient) in [
        (
            vec!["gossa-dafefb".into()],
            vec![],
            AbilityClass::BridgeAbility,
            0.1,
        ),
        (
            vec!["tos-spock-86f176".into()],
            vec![],
            AbilityClass::BridgeAbility,
            1.0,
        ),
        (
            vec![],
            vec!["vil-winn-adami-6c42f3".into()],
            AbilityClass::BelowDeck,
            2.0,
        ),
    ] {
        let crew = resolve_crew_to_buff_set("", &bridge, &below, &officers, &options).crew;
        assert_diagnostic(&crew, class, coefficient);
    }
}
