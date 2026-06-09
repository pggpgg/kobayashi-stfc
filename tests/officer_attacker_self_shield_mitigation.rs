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
//! 5 officers in the bundled YAML use this pattern as of writing: SNW Pike, Harry Mudd,
//! Kathryn Janeway, One of Eleven, WOK Carol.

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::data::combat_effect_spec::{AbilityModifierSpec, AbilityTargetSpec};
use kobayashi::lcars::{build_officer_model_file_default, lcars_effect_to_combat_effect_spec};

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
            e.effect_type == "tag"
                && e.tag.as_deref().unwrap_or("") == "shieldmitigation:unmapped"
                && e.target.as_deref() == Some("self")
        })
        .expect("SNW Pike captain has a target=self shieldmitigation tag effect");

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

/// All 5 target=self `shieldmitigation` officers in the bundled YAML compile to the new
/// attacker-side channel — none leak into `ShieldMitigationBonus`. Pins the production
/// roster shape so a future officer that gets the wrong target setting will fail loudly
/// instead of regressing silently.
#[test]
fn all_target_self_shield_mitigation_officers_route_to_attacker_bonus() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };

    let mut checked = 0usize;
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
                if effect.effect_type != "tag"
                    || effect.tag.as_deref().unwrap_or("") != "shieldmitigation:unmapped"
                    || effect.target.as_deref() != Some("self")
                {
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
                    continue;
                };
                let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("compile");
                checked += 1;
                match compiled {
                    AbilityEffect::AttackerShieldMitigationBonus(_) => {}
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

    assert!(
        checked >= 5,
        "expected at least 5 target=self shieldmitigation officers in the bundled YAML, scanned {checked}"
    );
    assert!(
        leaked.is_empty(),
        "target=self shieldmitigation effects leaked off the AttackerShieldMitigationBonus channel: {leaked:?}"
    );
}
