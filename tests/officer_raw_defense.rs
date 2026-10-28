//! Raw all-defense semantics, separate from choosing a combat officer-stat source.
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, AttackerStats, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats, HostileMitigationParams, ShipType,
    SimulationConfig, TimingWindow, TraceMode,
};
use kobayashi::data::combat_effect_spec::CombatEffectSpec;
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    LcarsLevelStats, ResolveOptions,
};

#[test]
fn tom_paris_rank_four_uses_raw_defense_points_without_dividing_by_100() {
    let mut officers =
        index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    // A controlled stat row isolates the verified coefficient and units from the
    // still-unverified choice of effective own versus total officer Defense.
    let tom = officers.get_mut("tom-paris-3640cc").unwrap();
    tom.stats = vec![LcarsLevelStats {
        level: 20,
        attack: 10.0,
        defense: 420.6,
        health: 100.0,
    }];
    let crew = resolve_crew_to_buff_set(
        "unused-captain",
        &[],
        &["tom-paris-3640cc".into()],
        &officers,
        &ResolveOptions {
            tier: Some(4),
            officer_tiers: None,
            officer_levels: Some(std::collections::HashMap::from([(
                "tom-paris-3640cc".into(),
                20,
            )])),
        },
    )
    .crew;
    assert_eq!(crew.seats.len(), 1);
    assert_eq!(crew.seats[0].ability.timing, TimingWindow::CombatBegin);
    assert_eq!(
        crew.seats[0].ability.effect,
        AbilityEffect::BaseMitigationStatsAdd(4.0 * 420.6)
    );
}

#[test]
fn tom_paris_missing_officer_stat_does_not_treat_coefficient_as_raw_points() {
    let mut officers =
        index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    officers.get_mut("tom-paris-3640cc").unwrap().stats.clear();
    let crew = resolve_crew_to_buff_set(
        "unused-captain",
        &[],
        &["tom-paris-3640cc".into()],
        &officers,
        &ResolveOptions {
            tier: Some(4),
            officer_tiers: None,
            officer_levels: None,
        },
    )
    .crew;
    assert!(crew.seats.is_empty());
}

#[test]
fn raw_defense_compiles_without_percent_normalization_or_probability_clamp() {
    let spec: CombatEffectSpec = serde_json::from_value(serde_json::json!({
        "id":"raw_defense", "source":"manual", "trigger":"combat_begin",
        "target":"attacker_self", "modifier":"base_mitigation_stats_add",
        "operation":"add", "value":{"scalar":1682.4,"unit":"flat"}
    }))
    .unwrap();
    let (timing, effect, _) = compile_officer_combat_spec(&spec).unwrap();
    assert_eq!(timing, TimingWindow::CombatBegin);
    assert_eq!(effect, AbilityEffect::BaseMitigationStatsAdd(1682.4));
}

fn mitigation_event(raw_context: bool, bonus: f64) -> serde_json::Map<String, serde_json::Value> {
    let attacker = Combatant {
        id: "self".into(),
        hull_health: 1e9,
        armor: 0.5,
        shield_deflection: 0.25,
        dodge: 1.0,
        hostile_mitigation_params: raw_context.then_some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 86.0,
                shield_deflection: 677.0,
                dodge: 29.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 750.0,
                shield_piercing: 750.0,
                accuracy: 4500.0,
            },
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        attack: 2000.0,
        hull_health: 1e9,
        ..Default::default()
    };
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::BelowDeck,
            Ability {
                name: "raw_defense".into(),
                class: AbilityClass::BelowDeck,
                timing: TimingWindow::CombatBegin,
                boostable: true,
                effect: AbilityEffect::BaseMitigationStatsAdd(bonus),
                condition: None,
                weapon_scope: Default::default(),
            },
            false,
        )],
    };
    let result = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &crew,
    );
    result
        .events
        .into_iter()
        .find(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
        .unwrap()
        .values
}

#[test]
fn raw_defense_adds_after_base_multipliers_and_retains_nonlinear_curve() {
    let baseline = mitigation_event(true, 0.0);
    let boosted = mitigation_event(true, 1682.4);
    for (key, value) in [
        ("effective_armor", 86.0 * 1.5 + 1682.4),
        ("effective_shield_deflection", 677.0 * 1.25 + 1682.4),
        ("effective_dodge", 29.0 * 2.0 + 1682.4),
    ] {
        assert!(
            (boosted[key].as_f64().unwrap() - value).abs() < 1e-8,
            "{key}"
        );
    }
    assert_eq!(boosted["mitigation_additive_bonus"], 0.0);
    let mitigation = boosted["mitigation"].as_f64().unwrap();
    assert!(mitigation > baseline["mitigation"].as_f64().unwrap());
    assert!(
        mitigation < 0.72,
        "1682.4 raw points must not clamp directly to 100% immunity"
    );
}

#[test]
fn missing_raw_context_never_turns_raw_defense_into_final_mitigation_chance() {
    let baseline = mitigation_event(false, 0.0);
    let boosted = mitigation_event(false, 1682.4);
    assert_eq!(baseline["mitigation"], boosted["mitigation"]);
    assert_eq!(boosted["raw_mitigation_stats_applied"], false);
}
