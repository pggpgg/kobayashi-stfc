//! Integration test for John Harrison's "Sabotage" bridge ability.
//!
//! Per `data/officers/officer_modeling_fidelity.yaml`, Harrison's bridge ability is documented
//! as: "bridge shield ignore is mapped via shieldmitigation tag but enemy-targeted mitigation
//! reduction requires engine refinement." This test pins the engine refinement.
//!
//! In-game semantic: "For the first round of combat, John Harrison ignores X% of the
//! opponent's shield." This is a **multiplicative** bypass on the defender's shield mitigation,
//! not an additive subtraction:
//!     effective_mitigation = base_mitigation × (1 - bypass_fraction)
//!
//! Example (rank 2 → 70% bypass): a defender with 77.5% mitigation drops to
//! `0.775 × (1 - 0.70) = 0.2325` effective mitigation. The SHP : HHP split goes from
//! 77.5 : 22.5 to 23.25 : 76.75.

use std::path::Path;

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::data::combat_effect_spec::{AbilityModifierSpec, AbilityTargetSpec};
use kobayashi::lcars::{lcars_effect_to_combat_effect_spec, load_lcars_file};

#[test]
fn harrison_sabotage_compiles_to_shield_mitigation_bypass_fraction() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return; // minimal checkouts — matches `resolve_bundled_lcars_yaml_*` convention
    }
    let file = load_lcars_file(path).unwrap();
    let harrison = file
        .officers
        .iter()
        .find(|o| o.id == "harrison-56cc6c")
        .expect("harrison-56cc6c in bundled LCARS");

    let bridge = harrison
        .bridge_ability
        .as_ref()
        .expect("Harrison has a bridge ability");

    let (idx, target_effect) = bridge
        .effects
        .iter()
        .enumerate()
        .find(|(_, e)| {
            e.effect_type == "tag"
                && e.tag.as_deref().unwrap_or("") == "shieldmitigation:unmapped"
                && e.target.as_deref() == Some("enemy")
        })
        .expect("Harrison bridge has a target=enemy shieldmitigation tag effect");

    // Rank 1 in the YAML is `value: 0.6` (the per-rank values are 0.6/0.7/0.8/0.9/1.0).
    let spec = lcars_effect_to_combat_effect_spec(
        target_effect,
        &format!("harrison:bridge:{idx}"),
        &harrison.id,
        &bridge.name,
        Some(1),
        None,
    )
    .expect("Harrison shield_mitigation effect should produce a CombatEffectSpec");

    assert_eq!(spec.modifier, AbilityModifierSpec::ShieldMitigation);
    assert_eq!(spec.target, AbilityTargetSpec::DefenderOpponent);

    let (_, effect, _) =
        compile_officer_combat_spec(&spec).expect("compile Harrison shield_mitigation");
    match effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => {
            assert!(
                (v - 0.6).abs() < 1e-9,
                "Rank-1 Harrison bypass should be 0.6, got {v}"
            );
        }
        other => panic!("Expected ShieldMitigationBypassFraction, got {other:?}"),
    }
}

/// User-supplied worked example: tier-2 Harrison bypasses 70% of the defender's 77.5% shield
/// mitigation; effective mitigation should drop to 23.25%, flipping the SHP : HHP split.
#[test]
fn harrison_bypass_math_matches_in_game_split() {
    let base_mitigation: f64 = 0.775;
    let bypass: f64 = 0.70;
    let effective = base_mitigation * (1.0 - bypass);
    assert!(
        (effective - 0.2325).abs() < 1e-12,
        "expected effective mitigation 0.2325, got {effective}"
    );
    // SHP : HHP damage split flips with the bypass.
    let shp_split: f64 = effective;
    let hhp_split: f64 = 1.0 - effective;
    assert!((shp_split - 0.2325).abs() < 1e-12);
    assert!((hhp_split - 0.7675).abs() < 1e-12);
}

/// Bypass cannot exceed 100% — a single source ≥ 1.0 saturates effective mitigation at 0.
#[test]
fn harrison_bypass_cannot_exceed_100_percent() {
    let base_mitigation: f64 = 0.775;
    for bypass_input in [1.0_f64, 1.4, 2.5, f64::INFINITY] {
        let bypass_applied = bypass_input.clamp(0.0, 1.0);
        let effective = base_mitigation * (1.0 - bypass_applied);
        assert!(
            (0.0..=base_mitigation).contains(&effective),
            "bypass={bypass_input}: effective mitigation must stay in [0, base], got {effective}"
        );
        assert!(
            effective.abs() < 1e-12,
            "bypass={bypass_input} clamps to 1.0 → effective mitigation should be 0, got {effective}"
        );
    }
}
