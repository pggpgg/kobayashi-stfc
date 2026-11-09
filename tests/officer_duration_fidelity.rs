//! Source-bound duration checks for combat-start buffs and hit-triggered states.
use kobayashi::combat::{AbilityClass, AbilityCondition, AbilityEffect, TimingWindow};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, lcars_effect_coverage,
    resolve_crew_to_buff_set, MechanicCoverageTier, ResolveOptions,
};
use std::collections::HashMap;

fn has_range(condition: &AbilityCondition, expected_max: u32) -> bool {
    match condition {
        AbilityCondition::RoundRange { min: 1, max } => *max == expected_max,
        AbilityCondition::And(children) => children.iter().any(|c| has_range(c, expected_max)),
        _ => false,
    }
}

#[test]
fn start_relative_hull_gates_and_enemy_hull_drains_are_diagnostic_not_self_effects() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    // A pre-damaged ship can start a fight below 80% of max hull while still
    // being at 100% of its fight-start hull. Condition 44 cannot currently
    // distinguish those states. Charvanek's burn must therefore stay inert.
    for (id, class) in [
        ("charvanek-0f1b5c", AbilityClass::CaptainManeuver),
        ("tal-c3e4eb", AbilityClass::BridgeAbility),
        ("suder-d348a9", AbilityClass::BridgeAbility),
    ] {
        let buff = resolve_crew_to_buff_set(id, &[], &[], &officers, &ResolveOptions::default());
        let ability = buff.crew.seats.iter().find(|seat| {
            seat.ability.class == class
                && matches!(
                    seat.ability.effect,
                    AbilityEffect::UnsupportedOfficerEffect { .. }
                )
        });
        assert!(ability.is_some(), "{id} must retain an unsupported marker");
    }
}

#[test]
fn chang_and_marcus_hull_gate_tracks_opponent_despite_self_effect_target() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    for (id, threshold) in [("chang-ecc238", 0.6), ("marcus-073931", 0.8)] {
        assert!(officers.contains_key(id), "source officer {id} is missing");
        let buff = resolve_crew_to_buff_set(id, &[], &[], &officers, &ResolveOptions::default());
        assert!(
            buff.crew
                .seats
                .iter()
                .filter_map(|seat| seat.ability.condition.as_ref())
                .any(|condition| {
                    fn has_enemy_gate(c: &AbilityCondition, threshold: f64) -> bool {
                        match c {
                            AbilityCondition::StatBelow {
                                stat,
                                threshold_pct,
                            } => stat == "hull_hp" && (*threshold_pct - threshold).abs() < 1e-9,
                            AbilityCondition::And(parts) => {
                                parts.iter().any(|part| has_enemy_gate(part, threshold))
                            }
                            _ => false,
                        }
                    }
                    has_enemy_gate(condition, threshold)
                }),
            "{id} must gate on the enemy hull"
        );
    }
}

#[test]
fn nero_burn_duration_does_not_limit_future_weapon_procs() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "nero-1aeca9",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    );
    let ability = buff
        .crew
        .seats
        .iter()
        .find(|s| matches!(s.ability.effect, AbilityEffect::Burning { .. }))
        .expect("Nero's bridge ability must be represented");
    assert!(matches!(
        ability.ability.effect,
        AbilityEffect::Burning {
            duration_rounds: 2,
            ..
        }
    ));
    assert!(
        !ability
            .ability
            .condition
            .as_ref()
            .is_some_and(|c| has_range(c, 2)),
        "the two-round burning duration must not stop the ability triggering in later rounds"
    );
}

#[test]
fn suder_and_phlox_combat_start_critical_effects_expire() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "phlox-72c07b",
        &[],
        &["suder-d348a9".into()],
        &officers,
        &ResolveOptions::default(),
    );
    let phlox = buff
        .crew
        .seats
        .iter()
        .find(|s| s.officer_id.as_deref() == Some("phlox-72c07b"))
        .expect("Phlox bridge ability");
    assert!(
        matches!(phlox.ability.effect, AbilityEffect::HostileCritChanceReduction(v) if (v - 0.45).abs() < 1e-12),
        "Phlox must reduce the opponent's chance, not raise our chance"
    );
    assert!(phlox
        .ability
        .condition
        .as_ref()
        .is_some_and(|c| has_range(c, 8)));
    let suder = buff
        .crew
        .seats
        .iter()
        .find(|s| s.officer_id.as_deref() == Some("suder-d348a9"))
        .expect("Suder below-deck ability");
    assert!(suder
        .ability
        .condition
        .as_ref()
        .is_some_and(|c| has_range(c, 5)));
}

#[test]
fn ro_mudd_preserves_ranked_proc_and_coefficient_as_unresolved() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let options = ResolveOptions {
        tier: None,
        officer_tiers: Some(HashMap::from([("ro-mudd-21a89a".to_string(), 3)])),
        officer_levels: None,
    };
    let buff = resolve_crew_to_buff_set("ro-mudd-21a89a", &[], &[], &officers, &options);
    let proc = buff
        .crew
        .seats
        .iter()
        .find(|s| {
            s.ability.class == AbilityClass::BridgeAbility
                && matches!(
                    s.ability.effect,
                    AbilityEffect::UnsupportedOfficerEffect { .. }
                )
        })
        .expect("Ro Mudd's unverified crew damage must be explicitly retained");
    assert!(
        matches!(proc.ability.effect, AbilityEffect::UnsupportedOfficerEffect { coefficient, chance }
        if (coefficient - 3.0).abs() < 1e-12 && (chance - 0.4).abs() < 1e-12)
    );
}

#[test]
fn harry_mudd_and_eurydice_shield_effects_are_explicitly_unsupported() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    for (id, tier, class, expected_coefficient, expected_chance) in [
        (
            "harry-mudd-374d5f",
            3,
            AbilityClass::CaptainManeuver,
            0.0,
            0.2,
        ),
        (
            "eurydice-18b643",
            4,
            AbilityClass::BridgeAbility,
            0.05,
            0.65,
        ),
    ] {
        let options = ResolveOptions {
            tier: None,
            officer_tiers: Some(HashMap::from([(id.to_string(), tier)])),
            officer_levels: None,
        };
        let buff = resolve_crew_to_buff_set(id, &[], &[], &officers, &options);
        let ability = buff
            .crew
            .seats
            .iter()
            .find(|s| {
                s.ability.class == class
                    && matches!(
                        s.ability.effect,
                        AbilityEffect::UnsupportedOfficerEffect { .. }
                    )
            })
            .expect("uncertain shield mechanics must remain visible and inert");
        assert!(matches!(ability.ability.effect,
            AbilityEffect::UnsupportedOfficerEffect { coefficient, chance }
            if (coefficient - expected_coefficient).abs() < 1e-12
                && (chance - expected_chance).abs() < 1e-12));
    }
}

#[test]
fn annorax_unknown_hit_timing_is_not_mapped_to_an_unconditional_barrier() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "annorax-830d35",
        &[],
        &[],
        &officers,
        &ResolveOptions::default(),
    );
    let a = buff
        .crew
        .seats
        .iter()
        .find(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::UnsupportedOfficerEffect { .. }
            )
        })
        .expect("Annorax's unresolved within-volley timing must be explicit");
    assert_eq!(a.ability.timing, TimingWindow::DefensePhase);
    assert!(
        matches!(a.ability.effect, AbilityEffect::UnsupportedOfficerEffect { coefficient, chance } if (coefficient - 2000.0).abs() < 1e-12 && (chance - 1.0).abs() < 1e-12)
    );
    assert!(
        a.ability.condition.is_some(),
        "hostile, armada, and Tal conditions must remain"
    );
}

#[test]
fn known_unimplemented_officer_abilities_are_not_silent_proxies() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    for id in [
        "gowron-27ac30",
        "marcus-073931",
        "vemet-00a218",
        "b-elanna-torres-75cf02",
    ] {
        assert!(officers.contains_key(id), "missing catalog officer {id}");
        let buff = resolve_crew_to_buff_set(id, &[], &[], &officers, &ResolveOptions::default());
        assert!(
            buff.crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::UnsupportedOfficerEffect { .. }
            )),
            "missing unsupported marker for {id}"
        );
        if id == "vemet-00a218" {
            assert!(
                buff.crew
                    .seats
                    .iter()
                    .any(|s| s.ability.timing == TimingWindow::SelfShieldBreak
                        && matches!(
                            s.ability.effect,
                            AbilityEffect::UnsupportedOfficerEffect { .. }
                        )),
                "Vemet captain must not become a shield-break heal"
            );
        }
    }
}

#[test]
fn unsupported_source_families_remain_visible_for_passive_and_below_deck_seats() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    for (captain, below) in [
        ("origins-burnham-e854d6", ""),
        ("tyler-1dcc4d", ""),
        ("", "brad-boimler-ee5262"),
        ("", "tom-paris-3640cc"),
    ] {
        let below = if below.is_empty() {
            vec![]
        } else {
            vec![below.to_string()]
        };
        let buff =
            resolve_crew_to_buff_set(captain, &[], &below, &officers, &ResolveOptions::default());
        assert!(
            buff.crew.seats.iter().any(|s| matches!(
                s.ability.effect,
                AbilityEffect::UnsupportedOfficerEffect { .. }
            )),
            "missing diagnostic marker for captain={captain}, below={below:?}"
        );
        assert!(
            !buff.static_buffs.contains_key("unsupported_officer_effect"),
            "marker must not become a static buff"
        );
    }
}

#[test]
fn unsupported_officer_is_not_reported_as_implemented_by_coverage() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let mudd = officers.get("ro-mudd-21a89a").unwrap();
    let effect = &mudd.bridge_ability.as_ref().unwrap().effects[0];
    let coverage = lcars_effect_coverage(effect, &mudd.id, &ResolveOptions::default());
    assert_eq!(coverage.tier, MechanicCoverageTier::Ignored);
    assert_eq!(coverage.pathway, "unsupported_officer_effect");
}

#[test]
fn enemy_all_ships_critical_debuff_is_not_mapped_to_our_critical_chance() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let buff = resolve_crew_to_buff_set(
        "",
        &[],
        &["vil-gul-dukat-c46cb8".into()],
        &officers,
        &ResolveOptions::default(),
    );
    assert!(buff.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect { .. }
    )));
    assert!(!buff
        .crew
        .seats
        .iter()
        .any(|s| matches!(s.ability.effect, AbilityEffect::CritChanceBonus(_))));
}
