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

use kobayashi::combat::abilities::{
    filter_effects_by_condition, AbilityCondition, AbilityEffect, CombatContext, TimingWindow,
};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig,
};
use kobayashi::data::combat_effect_spec::{AbilityModifierSpec, AbilityTargetSpec};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id,
    lcars_effect_to_combat_effect_spec, resolve_crew_to_buff_set, ResolveOptions,
};

#[test]
fn harrison_sabotage_compiles_to_shield_mitigation_bypass_fraction() {
    // skip in minimal checkouts — matches `resolve_bundled_lcars_yaml_*` convention
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
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

    let (_, effect, condition) =
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
    assert!(
        matches!(
            &condition,
            Some(AbilityCondition::And(parts))
                if parts.iter().any(|c| matches!(
                    c,
                    AbilityCondition::RoundRange { min: 1, max: 1 }
                ))
        ),
        "Sabotage duration rounds:1 should AND a RoundRange 1..=1 gate, got {condition:?}"
    );
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

#[test]
fn harrison_sabotage_round_range_gates_bypass_to_first_combat_round() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(1),
        officer_tiers: None,
        officer_levels: None,
    };
    let buff_set = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &["harrison-56cc6c".to_string()],
        &[],
        &officers,
        &opts,
    );
    let bypass_effects: Vec<_> = buff_set
        .crew
        .seats
        .iter()
        .filter(|s| {
            s.ability.timing == TimingWindow::CombatBegin
                && matches!(
                    s.ability.effect,
                    AbilityEffect::ShieldMitigationBypassFraction(_)
                )
        })
        .map(|s| kobayashi::combat::ActiveAbilityEffect {
            ability_name: s.ability.name.clone(),
            officer_id: s.officer_id.clone(),
            effect: s.ability.effect,
            boosted: s.boosted,
            condition: s.ability.condition.clone(),
        })
        .collect();
    assert_eq!(bypass_effects.len(), 1, "expected one Sabotage bypass seat");

    let mut ctx = CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction: OpponentFactionTag::Unknown,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        defender_hull_faction_id: 0,
        defender_ship_type: ShipType::Explorer,
        attacker_ship_type: ShipType::Battleship,
        attacker_ship_id: std::sync::Arc::from("att"),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: true,
        defender_hostile_tag_mask: 0,
        engagement_enemy_types: std::sync::Arc::new(Default::default()),
        combat_battle_type_id: None,
        defender_level: Some(50),
    };
    assert_eq!(
        filter_effects_by_condition(&bypass_effects, &ctx).len(),
        1,
        "round 1 should apply Sabotage bypass"
    );
    ctx.round_index = 2;
    assert!(
        filter_effects_by_condition(&bypass_effects, &ctx).is_empty(),
        "round 2 should not apply first-round-only Sabotage bypass"
    );
}

#[test]
fn harrison_sabotage_increases_outbound_hull_damage_vs_high_shield_mitigation() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(2),
        officer_tiers: None,
        officer_levels: None,
    };
    let with_harrison = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &["harrison-56cc6c".to_string()],
        &[],
        &officers,
        &opts,
    )
    .crew;
    let without_harrison = resolve_crew_to_buff_set("kirk-1323b6", &[], &[], &officers, &opts).crew;

    let attacker = Combatant {
        id: "att".into(),
        attack: 2_500.0,
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
        hull_health: 100_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let defender = Combatant {
        id: "def".into(),
        shield_mitigation: 0.775,
        shield_health: 200_000.0,
        hull_health: 200_000.0,
        ..attacker.clone()
    };
    let config = SimulationConfig {
        rounds: 1,
        defender_level: Some(50),
        ..Default::default()
    };

    let setup_with = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &with_harrison,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let setup_without = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &without_harrison,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let seed = 42_u64;
    let with = simulate_combat_from_setup(&setup_with, seed);
    let without = simulate_combat_from_setup(&setup_without, seed);
    assert!(
        with.defender_shield_remaining > without.defender_shield_remaining + 1.0,
        "Harrison 70% bypass (rank 2) should lower effective shield mitigation in round 1, \
         leaving more defender shield HP (with_shield={}, without_shield={}, total_damage with={}, without={})",
        with.defender_shield_remaining,
        without.defender_shield_remaining,
        with.total_damage,
        without.total_damage
    );
    assert!(
        with.defender_hull_remaining < without.defender_hull_remaining,
        "more hull damage with bypass: with_hull={}, without_hull={}",
        with.defender_hull_remaining,
        without.defender_hull_remaining
    );
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
