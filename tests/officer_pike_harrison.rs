//! Pike captain `OffAbilityEffect` scales bridge officer ability magnitudes:
//! `effective = min(1.0, base × (1 + pike_bonus))` (e.g. Harrison Sabotage 60% × 1.4 = 84%).

use std::path::Path;

use kobayashi::combat::abilities::{
    scale_bridge_officer_ability_effect, scale_crew_bridge_ability_effects,
    sum_bridge_ability_effectiveness_add, Ability, AbilityClass, AbilityEffect,
    ActiveAbilityEffect, CrewConfiguration, CrewSeat, CrewSeatContext, TimingWindow,
};
use kobayashi::combat::{
    build_combat_setup, Combatant, OpponentFactionTag, ShipType, SimulationConfig,
};
use kobayashi::lcars::{
    index_lcars_officers_by_id, load_lcars_file, resolve_crew_to_buff_set, ResolveOptions,
};

#[test]
fn scale_bridge_officer_effect_harrison_sabotage_with_pike_rank1() {
    let mut effect = AbilityEffect::ShieldMitigationBypassFraction(0.6);
    scale_bridge_officer_ability_effect(&mut effect, 0.4);
    match effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => {
            assert!(
                (v - 0.84).abs() < 1e-9,
                "60% bypass × (1 + 40%) should be 84%, got {v}"
            );
        }
        other => panic!("expected bypass fraction, got {other:?}"),
    }
}

#[test]
fn scale_bridge_officer_effect_caps_at_one() {
    let mut effect = AbilityEffect::ShieldMitigationBypassFraction(0.8);
    scale_bridge_officer_ability_effect(&mut effect, 0.8);
    match effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => {
            assert!(
                (v - 1.0).abs() < 1e-9,
                "0.8 × 1.8 should cap at 1.0, got {v}"
            );
        }
        other => panic!("expected bypass fraction, got {other:?}"),
    }
}

#[test]
fn pike_captain_harrison_bridge_resolves_sabotage_at_eighty_four_percent() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return;
    }
    let file = load_lcars_file(path).unwrap();
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(1),
        officer_tiers: None,
        officer_levels: None,
    };
    let buff_set = resolve_crew_to_buff_set(
        "pike-1e7d0d",
        &["harrison-56cc6c".to_string()],
        &[],
        &officers,
        &opts,
    );
    let crew = buff_set.crew.clone();
    let attacker = dummy_combatant();
    let defender = attacker.clone();
    let config = SimulationConfig {
        defender_level: Some(50),
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let bypass = setup
        .attacker_crew
        .seats
        .iter()
        .find(|s| {
            s.seat == CrewSeat::Bridge
                && s.ability.class == AbilityClass::BridgeAbility
                && matches!(
                    s.ability.effect,
                    AbilityEffect::ShieldMitigationBypassFraction(_)
                )
        })
        .map(|s| match s.ability.effect {
            AbilityEffect::ShieldMitigationBypassFraction(v) => v,
            _ => unreachable!(),
        })
        .expect("Harrison bridge bypass seat");
    assert!(
        (bypass - 0.84).abs() < 1e-9,
        "Pike tier-1 + Harrison tier-1 bridge bypass should be 0.84, got {bypass}"
    );
}

fn dummy_combatant() -> Combatant {
    Combatant {
        id: "test_ship".to_string(),
        attack: 1000.0,
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
        hull_health: 1000.0,
        shield_health: 1000.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

#[test]
fn pike_boost_inactive_when_hostile_level_above_seventy() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return;
    }
    let file = load_lcars_file(path).unwrap();
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(1),
        officer_tiers: None,
        officer_levels: None,
    };
    let buff_set = resolve_crew_to_buff_set(
        "pike-1e7d0d",
        &["harrison-56cc6c".to_string()],
        &[],
        &officers,
        &opts,
    );
    let crew = buff_set.crew.clone();
    let attacker = dummy_combatant();
    let defender = attacker.clone();
    let config = SimulationConfig {
        defender_level: Some(71),
        ..Default::default()
    };
    let setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let bypass = setup
        .attacker_crew
        .seats
        .iter()
        .find(|s| {
            s.seat == CrewSeat::Bridge
                && matches!(
                    s.ability.effect,
                    AbilityEffect::ShieldMitigationBypassFraction(_)
                )
        })
        .map(|s| match s.ability.effect {
            AbilityEffect::ShieldMitigationBypassFraction(v) => v,
            _ => unreachable!(),
        })
        .expect("Harrison bypass");
    assert!(
        (bypass - 0.6).abs() < 1e-9,
        "Pike boost should not apply vs level 71 hostile, expected 0.6 got {bypass}"
    );
}

#[test]
fn sum_bridge_effectiveness_from_combat_begin_rows() {
    let effects = vec![ActiveAbilityEffect {
        ability_name: "Teaching Moments".into(),
        officer_id: Some("pike-1e7d0d".into()),
        effect: AbilityEffect::BridgeAbilityEffectivenessBonus(0.4),
        boosted: false,
        condition: None,
    }];
    assert!((sum_bridge_ability_effectiveness_add(&effects) - 0.4).abs() < 1e-9);
}

#[test]
fn scale_crew_skips_captain_seat_bridge_ability() {
    let mut crew = CrewConfiguration {
        seats: vec![
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "meta".into(),
                    class: AbilityClass::CaptainManeuver,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::BridgeAbilityEffectivenessBonus(0.4),
                    condition: None,
                },
                boosted: false,
                officer_id: Some("pike-1e7d0d".into()),
                contribution_batch: 0,
            },
            CrewSeatContext {
                seat: CrewSeat::Captain,
                ability: Ability {
                    name: "own_bridge".into(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::ShieldMitigationBypassFraction(0.5),
                    condition: None,
                },
                boosted: false,
                officer_id: Some("pike-1e7d0d".into()),
                contribution_batch: 0,
            },
            CrewSeatContext {
                seat: CrewSeat::Bridge,
                ability: Ability {
                    name: "harrison".into(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::ShieldMitigationBypassFraction(0.6),
                    condition: None,
                },
                boosted: false,
                officer_id: Some("harrison-56cc6c".into()),
                contribution_batch: 1,
            },
        ],
    };
    scale_crew_bridge_ability_effects(&mut crew, 0.4);
    let captain_bridge = match crew.seats[1].ability.effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => v,
        _ => panic!(),
    };
    let harrison = match crew.seats[2].ability.effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => v,
        _ => panic!(),
    };
    assert!(
        (captain_bridge - 0.5).abs() < 1e-9,
        "captain-seat bridge ability should not be Pike-scaled"
    );
    assert!((harrison - 0.84).abs() < 1e-9);
}
