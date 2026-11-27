//! Regression test for `target=AttackerSelf` shield-mitigation officer effects.
//!
//! Before this fix the YAML→IR adapter routed all `shield_mitigation` officer effects into
//! the same `ShieldMitigationBonus` channel, which the engine reads against the **defender's**
//! mitigation on the outbound damage path. For `target=enemy` rows that direction was
//! arguable (debuff via negative value); for `target=self` rows ("Officer X grants +Y%
//! shield mitigation when attacked") it was strictly wrong — the bonus buffed the **defender**
//! and silently hurt the attacker.
//!
//! Fix: compile `ShieldMitigation` + `target=AttackerSelf` to
//! [`AbilityEffect::AttackerShieldMitigationBonus`], a separate channel that the engine
//! consumes in `effective_incoming_shield_mitigation` (the counter-fire path).
//!
//! 6 officers in the bundled catalog use this pattern as of writing: SNW Pike, Harry Mudd,
//! Kathryn Janeway, One of Eleven, WOK Carol, Trelane.

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::data::combat_effect_spec::{AbilityModifierSpec, AbilityTargetSpec};
use kobayashi::lcars::{build_officer_model_file_default, lcars_effect_to_combat_effect_spec};
use std::collections::HashSet;

#[test]
fn snw_pike_captain_self_shield_mitigation_compiles_to_attacker_bonus() {
    // skip in minimal checkouts — matches `resolve_bundled_lcars_yaml_*` convention
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let pike = file
        .officers
        .iter()
        .find(|o| o.id == "snw-pike-c94ac4")
        .expect("snw-pike-c94ac4 in bundled LCARS");

    let captain = pike
        .captain_ability
        .as_ref()
        .expect("SNW Pike has a captain ability");

    let (idx, target_effect) = captain
        .effects
        .iter()
        .enumerate()
        .find(|(_, e)| {
            e.effect_type == "stat_modify"
                && e.stat.as_deref() == Some("shield_mitigation")
                && e.target.as_deref() == Some("self")
        })
        .expect("SNW Pike captain has a target=self shield_mitigation stat_modify effect");

    let spec = lcars_effect_to_combat_effect_spec(
        target_effect,
        &format!("snw-pike:captain:{idx}"),
        &pike.id,
        &captain.name,
        Some(1),
        None,
    )
    .expect("SNW Pike captain effect should produce a CombatEffectSpec");

    assert_eq!(spec.modifier, AbilityModifierSpec::ShieldMitigation);
    assert_eq!(spec.target, AbilityTargetSpec::AttackerSelf);

    let (_, effect, _) =
        compile_officer_combat_spec(&spec).expect("compile SNW Pike shield mitigation");
    match effect {
        AbilityEffect::AttackerShieldMitigationBonus(v) => {
            // YAML value at rank 1 is 0.04. Engine adds this to attacker.shield_mitigation
            // in the counter-fire path; final value is clamped to [0, 1] at the apply site.
            assert!(
                (v - 0.04).abs() < 1e-9,
                "expected +0.04 attacker mitigation bonus, got {v}"
            );
        }
        AbilityEffect::ShieldMitigationBonus(v) => {
            panic!(
                "regression: AttackerSelf shieldmitigation still emits the outbound \
                 ShieldMitigationBonus channel ({v}); engine would buff the defender."
            );
        }
        other => panic!("Expected AttackerShieldMitigationBonus, got {other:?}"),
    }
}

/// All target=self `shieldmitigation` / `shield_mitigation` rows route to the attacker
/// self-mitigation channel (either static/CombatBegin `AttackerShieldMitigationBonus` or reactive
/// `HullDamageTaken` `ReactiveDamageTaken { kind: AttackerShieldMitigationBonus(..), .. }`),
/// never leaking into the defender-side `ShieldMitigationBonus`.
#[test]
fn all_target_self_shield_mitigation_officers_route_to_attacker_bonus() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };

    let mut checked = HashSet::new();
    let mut reactive_hull_damage = HashSet::new();
    let mut leaked: Vec<String> = Vec::new();

    for officer in &file.officers {
        for ability in [
            officer.captain_ability.as_ref(),
            officer.bridge_ability.as_ref(),
            officer.below_decks_ability.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            for (idx, effect) in ability.effects.iter().enumerate() {
                let is_self_shield_mit_tag = effect.effect_type == "tag"
                    && effect.tag.as_deref().unwrap_or("") == "shieldmitigation:unmapped"
                    && effect.target.as_deref() == Some("self");
                let is_self_shield_mit_stat = effect.effect_type == "stat_modify"
                    && effect.stat.as_deref() == Some("shield_mitigation")
                    && effect.target.as_deref() == Some("self");
                if !is_self_shield_mit_tag && !is_self_shield_mit_stat {
                    continue;
                }
                let Some(spec) = lcars_effect_to_combat_effect_spec(
                    effect,
                    &format!("{}::{}::{}", officer.id, ability.name, idx),
                    &officer.id,
                    &ability.name,
                    Some(1),
                    None,
                ) else {
                    leaked.push(format!(
                        "{}::{} unexpectedly failed to compile ({:?})",
                        officer.id, ability.name, effect.trigger
                    ));
                    continue;
                };
                let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("compile");
                match compiled {
                    AbilityEffect::AttackerShieldMitigationBonus(_) => {
                        checked.insert(officer.id.clone());
                    }
                    AbilityEffect::ReactiveDamageTaken {
                        trigger: kobayashi::combat::DamageTakenTrigger::HullDamage,
                        kind:
                            kobayashi::combat::ReactiveDamageTakenKind::AttackerShieldMitigationBonus(_),
                        ..
                    } => {
                        reactive_hull_damage.insert(officer.id.clone());
                    }
                    AbilityEffect::ShieldMitigationBonus(v) => {
                        leaked.push(format!(
                            "{}::{} ShieldMitigationBonus({v})",
                            officer.id, ability.name
                        ));
                    }
                    other => {
                        leaked.push(format!(
                            "{}::{} unexpected {other:?}",
                            officer.id, ability.name
                        ));
                    }
                }
            }
        }
    }

    let expected_checked = HashSet::from([
        "one-of-eleven-ee0ee9".to_string(),
        "snw-pike-c94ac4".to_string(),
        "trelane-96b872".to_string(),
    ]);
    assert_eq!(
        checked, expected_checked,
        "CombatBegin self shield-mitigation rows"
    );
    let expected_reactive = HashSet::from([
        "kathryn-janeway-bd4a19".to_string(),
        "wok-carol-52a350".to_string(),
    ]);
    assert_eq!(
        reactive_hull_damage, expected_reactive,
        "HullDamageTaken reactive self shield-mitigation rows"
    );
    assert!(
        leaked.is_empty(),
        "target=self shieldmitigation effects leaked off the AttackerShieldMitigationBonus channel: {leaked:?}"
    );
}
