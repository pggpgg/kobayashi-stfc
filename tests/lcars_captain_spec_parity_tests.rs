//! Task 9: captain maneuver blocks must round-trip LCARS → [`CombatEffectSpec`] → compile identically
//! to [`kobayashi::lcars::resolve_officer_ability`] (which delegates to the same adapter + compiler).

use std::collections::{BTreeSet, HashMap};

use kobayashi::combat::abilities::{Ability, AbilityClass, CrewSeat, CrewSeatContext};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::lcars::effect_spec_adapter::lcars_effect_to_combat_effect_spec;
use kobayashi::lcars::{
    load_lcars_file, resolve_officer_ability, LcarsAbility, LcarsOfficer, ResolveOptions,
};

fn contexts_from_officer_combat_effect_spec(
    officer: &LcarsOfficer,
    ability: &LcarsAbility,
    seat: CrewSeat,
    class: AbilityClass,
    options: &ResolveOptions,
    contribution_batch: u32,
) -> Vec<CrewSeatContext> {
    let mut contexts = Vec::new();
    for (idx, effect) in ability.effects.iter().enumerate() {
        let tier = options.tier_for(&officer.id);
        let stable_id = format!("lcars:{}:{}:{idx}", officer.id, ability.name);
        let Some(spec) = lcars_effect_to_combat_effect_spec(
            effect,
            &stable_id,
            &officer.id,
            &ability.name,
            tier,
        ) else {
            continue;
        };
        let Ok((timing, effect_effect, condition)) = compile_officer_combat_spec(&spec) else {
            continue;
        };
        contexts.push(CrewSeatContext {
            seat,
            ability: Ability {
                name: ability.name.clone(),
                class,
                timing,
                boostable: true,
                effect: effect_effect,
                condition,
            },
            boosted: false,
            officer_id: Some(officer.id.clone()),
            contribution_batch,
        });
    }
    contexts
}

#[test]
fn captain_maneuver_effect_type_histogram_is_allowlisted() {
    let path = format!(
        "{}/data/officers/officers.lcars.yaml",
        env!("CARGO_MANIFEST_DIR")
    );
    let file = load_lcars_file(&path).expect("load officers.lcars.yaml");
    let mut types: BTreeSet<String> = BTreeSet::new();
    for o in &file.officers {
        let Some(a) = &o.captain_ability else {
            continue;
        };
        for e in &a.effects {
            types.insert(e.effect_type.clone());
        }
    }
    let allowed = [
        "assimilated",
        "burning",
        "extra_attack",
        "hull_breach",
        "morale",
        "stat_modify",
        "tag",
    ];
    for t in &types {
        assert!(
            allowed.contains(&t.as_str()),
            "unexpected captain_ability effect type {t:?}; extend OFFICER_SPEC / adapter if intentional"
        );
    }
}

#[test]
fn captain_maneuver_spec_path_matches_resolver_all_tiers() {
    let path = format!(
        "{}/data/officers/officers.lcars.yaml",
        env!("CARGO_MANIFEST_DIR")
    );
    let officers = load_lcars_file(&path)
        .expect("load officers.lcars.yaml")
        .officers;
    for tier in 1u8..=5u8 {
        for o in &officers {
            let Some(cap) = &o.captain_ability else {
                continue;
            };
            let mut tiers = HashMap::new();
            tiers.insert(o.id.clone(), tier);
            let opts = ResolveOptions {
                tier: None,
                officer_tiers: Some(tiers),
            };
            let batch = 0u32;
            let from_resolver = resolve_officer_ability(
                o,
                cap,
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                &opts,
                batch,
            );
            let from_spec = contexts_from_officer_combat_effect_spec(
                o,
                cap,
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                &opts,
                batch,
            );
            assert_eq!(
                from_resolver, from_spec,
                "captain maneuver mismatch officer={} tier={} ability={}",
                o.id, tier, cap.name
            );
        }
    }
}

#[test]
fn bridge_and_below_decks_spec_path_matches_resolver_sample_tiers() {
    let path = format!(
        "{}/data/officers/officers.lcars.yaml",
        env!("CARGO_MANIFEST_DIR")
    );
    let officers = load_lcars_file(&path)
        .expect("load officers.lcars.yaml")
        .officers;
    for tier in [1u8, 3u8, 5u8] {
        for o in &officers {
            let mut tiers = HashMap::new();
            tiers.insert(o.id.clone(), tier);
            let opts = ResolveOptions {
                tier: None,
                officer_tiers: Some(tiers),
            };
            if let Some(ref bridge) = o.bridge_ability {
                let batch = 1u32;
                let r = resolve_officer_ability(
                    o,
                    bridge,
                    CrewSeat::Bridge,
                    AbilityClass::BridgeAbility,
                    &opts,
                    batch,
                );
                let s = contexts_from_officer_combat_effect_spec(
                    o,
                    bridge,
                    CrewSeat::Bridge,
                    AbilityClass::BridgeAbility,
                    &opts,
                    batch,
                );
                assert_eq!(r, s, "bridge officer={} tier={}", o.id, tier);
            }
            if let Some(ref bd) = o.below_decks_ability {
                let batch = 2u32;
                let r = resolve_officer_ability(
                    o,
                    bd,
                    CrewSeat::BelowDeck,
                    AbilityClass::BelowDeck,
                    &opts,
                    batch,
                );
                let s = contexts_from_officer_combat_effect_spec(
                    o,
                    bd,
                    CrewSeat::BelowDeck,
                    AbilityClass::BelowDeck,
                    &opts,
                    batch,
                );
                assert_eq!(r, s, "below_decks officer={} tier={}", o.id, tier);
            }
        }
    }
}
