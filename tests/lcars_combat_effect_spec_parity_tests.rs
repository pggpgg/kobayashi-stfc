//! Golden parity: LCARS resolver (`resolve_officer_ability` / `resolve_lcars_condition`) vs
//! [`CombatEffectSpec`] compile path (`lcars_*_to_spec` + [`kobayashi::combat::effect_spec_compile::compile_*`]).
//!
//! `compile_condition` flattens singleton [`AbilityConditionSpec::And`] to a single child; the resolver
//! keeps `And(vec![one])`. We only assert `and` parity when there are **two or more** children.

use kobayashi::combat::abilities::{AbilityClass, AbilityEffect, CrewSeat};
use kobayashi::combat::effect_spec_compile::{compile_condition, compile_trigger};
use kobayashi::combat::TimingWindow;
use kobayashi::lcars::effect_spec_adapter::{
    lcars_condition_to_spec, lcars_effect_to_combat_effect_spec,
};
use kobayashi::lcars::{
    resolve_lcars_condition, resolve_officer_ability, LcarsAbility, LcarsCondition, LcarsEffect,
    LcarsOfficer, ResolveOptions,
};

fn empty_lcars_condition() -> LcarsCondition {
    LcarsCondition {
        condition_type: String::new(),
        stat: None,
        threshold_pct: None,
        min: None,
        max: None,
        faction: None,
        group: None,
        min_members: None,
        tag: None,
        ship_type: None,
        faction_id: None,
        conditions: None,
    }
}

fn test_officer() -> LcarsOfficer {
    LcarsOfficer {
        id: "parity_lcars".into(),
        name: "Parity".into(),
        faction: None,
        rarity: None,
        group: None,
        captain_ability: None,
        bridge_ability: None,
        below_decks_ability: None,
    }
}

fn stat_modify_effect(
    stat: &str,
    value: f64,
    trigger: &str,
    op: Option<&str>,
    target: Option<&str>,
    condition: Option<LcarsCondition>,
) -> LcarsEffect {
    LcarsEffect {
        effect_type: "stat_modify".into(),
        stat: Some(stat.into()),
        target: target.map(|s| s.into()),
        operator: Some(op.unwrap_or("add").into()),
        value: Some(value),
        trigger: Some(trigger.into()),
        duration: None,
        scaling: None,
        condition,
        chance: None,
        multiplier: None,
        tag: None,
        accumulate: None,
        decay: None,
    }
}

fn lcars_condition_matrix() -> Vec<LcarsCondition> {
    vec![
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "stat_below".into();
            c.stat = Some("shield_hp".into());
            c.threshold_pct = Some(0.3);
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "stat_above".into();
            c.threshold_pct = Some(0.9);
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "round_range".into();
            c.min = Some(2);
            c.max = Some(5);
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "morale_active".into();
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "defender_burning".into();
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "defender_faction_is".into();
            c.faction = Some("klingon".into());
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "defender_ship_type_is".into();
            c.ship_type = Some("explorer".into());
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "defender_hull_faction_id".into();
            c.faction_id = Some(42);
            c
        },
        {
            let mut inner = empty_lcars_condition();
            inner.condition_type = "defender_burning".into();
            let mut c = empty_lcars_condition();
            c.condition_type = "not".into();
            c.conditions = Some(vec![inner]);
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "and".into();
            c.conditions = Some(vec![
                {
                    let mut x = empty_lcars_condition();
                    x.condition_type = "morale_active".into();
                    x
                },
                {
                    let mut x = empty_lcars_condition();
                    x.condition_type = "defender_burning".into();
                    x
                },
            ]);
            c
        },
        {
            let mut c = empty_lcars_condition();
            c.condition_type = "or".into();
            c.conditions = Some(vec![
                {
                    let mut x = empty_lcars_condition();
                    x.condition_type = "defender_is_npc_hostile".into();
                    x
                },
                {
                    let mut x = empty_lcars_condition();
                    x.condition_type = "defender_is_player_ship".into();
                    x
                },
            ]);
            c
        },
    ]
}

#[test]
fn lcars_condition_compile_matches_resolve_lcars_condition() {
    for c in lcars_condition_matrix() {
        let compiled =
            compile_condition(&lcars_condition_to_spec(&c).expect("to spec")).expect("compile");
        let resolved = resolve_lcars_condition(&c).expect("resolve");
        assert_eq!(compiled, resolved, "condition_type={}", c.condition_type);
    }
}

#[test]
fn lcars_spec_trigger_compile_matches_officer_ability_timing() {
    let officer = test_officer();
    let triggers = [
        ("on_attack", TimingWindow::AttackPhase),
        ("on_round_start", TimingWindow::RoundStart),
        ("on_defense", TimingWindow::DefensePhase),
        ("on_round_end", TimingWindow::RoundEnd),
        ("on_kill", TimingWindow::Kill),
        ("on_hull_breach", TimingWindow::HullBreach),
        ("on_receive_damage", TimingWindow::ReceiveDamage),
        ("on_combat_end", TimingWindow::CombatEnd),
        ("on_own_shield_break", TimingWindow::SelfShieldBreak),
        ("on_enemy_shield_break", TimingWindow::ShieldBreak),
        ("after_shot", TimingWindow::AfterSubround),
    ];
    for (trigger, expected_tw) in triggers {
        let e = stat_modify_effect("weapon_damage", 0.05, trigger, Some("add"), None, None);
        let spec = lcars_effect_to_combat_effect_spec(&e, "tid", "parity_lcars", "ab")
            .unwrap_or_else(|| panic!("spec None for trigger {trigger}"));
        let tw = compile_trigger(spec.trigger).expect("compile_trigger");
        assert_eq!(tw, expected_tw, "trigger {trigger}");
        let ability = LcarsAbility {
            name: "ab".into(),
            effects: vec![e],
        };
        let ctx = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(ctx.len(), 1, "trigger {trigger}");
        assert_eq!(ctx[0].ability.timing, expected_tw);
    }
}

#[test]
fn lcars_on_shield_break_target_disambiguates_timing() {
    let officer = test_officer();
    let cases = [
        ("on_shield_break", Some("enemy"), TimingWindow::ShieldBreak),
        ("on_shield_break", None, TimingWindow::SelfShieldBreak),
        (
            "on_shield_break",
            Some("self"),
            TimingWindow::SelfShieldBreak,
        ),
    ];
    for (trigger, target, expected_tw) in cases {
        let e = stat_modify_effect("weapon_damage", 0.01, trigger, Some("add"), target, None);
        let spec =
            lcars_effect_to_combat_effect_spec(&e, "tid", "parity_lcars", "ab").expect("spec");
        assert_eq!(compile_trigger(spec.trigger).unwrap(), expected_tw);
        let ability = LcarsAbility {
            name: "ab".into(),
            effects: vec![e],
        };
        let ctx = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].ability.timing, expected_tw);
    }
}

#[test]
fn lcars_effect_with_condition_spec_matches_resolve() {
    let mut c = empty_lcars_condition();
    c.condition_type = "defender_burning".into();
    let e = stat_modify_effect(
        "weapon_damage",
        0.2,
        "on_attack",
        Some("add"),
        None,
        Some(c.clone()),
    );
    let spec =
        lcars_effect_to_combat_effect_spec(&e, "id", "parity_lcars", "strike").expect("spec");
    assert_eq!(spec.conditions.len(), 1);
    let cc = compile_condition(&spec.conditions[0]).expect("cc");
    let rc = resolve_lcars_condition(&c).expect("rc");
    assert_eq!(cc, rc);
    let ability = LcarsAbility {
        name: "strike".into(),
        effects: vec![e],
    };
    let officer = test_officer();
    let ctx = resolve_officer_ability(
        &officer,
        &ability,
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        &ResolveOptions::default(),
        0,
    );
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].ability.condition, Some(rc));
}

#[test]
fn lcars_spec_scalar_matches_resolver_effect_for_weapon_crit_pierce() {
    let officer = test_officer();
    let cases = [
        ("weapon_damage", 0.12f64, "weapon_damage"),
        ("crit_chance", 0.05f64, "crit_chance"),
        ("crit_damage", 0.25f64, "crit_damage"),
        ("armor_pierce", 0.1f64, "armor_pierce"),
    ];
    for (stat, value, label) in cases {
        let e = stat_modify_effect(stat, value, "on_attack", Some("add"), None, None);
        let spec = lcars_effect_to_combat_effect_spec(&e, "tid", "parity_lcars", "ab")
            .unwrap_or_else(|| panic!("{label} spec"));
        let scalar = spec.value.as_ref().and_then(|v| v.scalar).expect("scalar");
        assert!((scalar - value).abs() < 1e-12, "{label} scalar mismatch");
        let ability = LcarsAbility {
            name: "ab".into(),
            effects: vec![e],
        };
        let ctx = resolve_officer_ability(
            &officer,
            &ability,
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            &ResolveOptions::default(),
            0,
        );
        assert_eq!(ctx.len(), 1, "{label}");
        match (label, &ctx[0].ability.effect) {
            ("weapon_damage", AbilityEffect::AttackMultiplier(m)) => {
                assert!((m - (1.0 + value)).abs() < 1e-12);
            }
            ("crit_chance", AbilityEffect::CritChanceBonus(x)) => {
                assert!((x - value).abs() < 1e-12);
            }
            ("crit_damage", AbilityEffect::CritDamageMultiplier(m)) => {
                assert!((m - (1.0 + value)).abs() < 1e-12);
            }
            ("armor_pierce", AbilityEffect::PierceBonus(x)) => {
                assert!((x - value).abs() < 1e-12);
            }
            (_, other) => panic!("unexpected effect for {label}: {other:?}"),
        }
    }
}
