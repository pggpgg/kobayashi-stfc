//! Unit + integration tests for [`kobayashi::lcars::LcarsDropReport`].
//!
//! Synthetic cases pin each drop category to its expected reason discriminator. The
//! `production_yaml_drop_baseline` test walks the bundled `officers.lcars.yaml` so the
//! coverage report doesn't silently regress when officers/triggers/tags change.

use std::path::Path;

use kobayashi::lcars::{
    collect_lcars_drops, lcars_effect_to_combat_effect_spec_with_report, load_lcars_file,
    LcarsCondition, LcarsDropReport, LcarsEffect,
};

fn lcars_effect_stat_modify(stat: &str, value: f64, trigger: &str) -> LcarsEffect {
    LcarsEffect {
        effect_type: "stat_modify".to_string(),
        stat: Some(stat.to_string()),
        target: None,
        operator: Some("add".to_string()),
        value: Some(value),
        trigger: Some(trigger.to_string()),
        duration: None,
        scaling: None,
        condition: None,
        chance: None,
        multiplier: None,
        tag: None,
        accumulate: None,
        decay: None,
    }
}

fn lcars_effect_tag(tag: &str, trigger: &str) -> LcarsEffect {
    LcarsEffect {
        effect_type: "tag".to_string(),
        stat: None,
        target: Some("self".to_string()),
        operator: None,
        value: Some(0.1),
        trigger: Some(trigger.to_string()),
        duration: None,
        scaling: None,
        condition: None,
        chance: None,
        multiplier: None,
        tag: Some(tag.to_string()),
        accumulate: None,
        decay: None,
    }
}

#[test]
fn reports_unmapped_tag_for_allreloadspeed() {
    let effect = lcars_effect_tag("allreloadspeed:unmapped", "on_attack");
    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-x",
        "Ability A",
        None,
        None,
        0,
        Some(&mut report),
    );
    assert!(out.is_none(), "unmapped tag should produce no spec");
    assert_eq!(report.drops.len(), 1, "expected one drop record");
    let drop = &report.drops[0];
    assert_eq!(drop.officer_id, "officer-x");
    assert_eq!(drop.ability_name, "Ability A");
    assert_eq!(drop.effect_index, 0);
    assert_eq!(drop.reason, "unmapped_tag:allreloadspeed");
}

#[test]
fn reports_unknown_trigger() {
    let effect = lcars_effect_stat_modify("weapon_damage", 0.1, "onmyaunt");
    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-y",
        "Cap",
        None,
        None,
        2,
        Some(&mut report),
    );
    assert!(out.is_none(), "unknown trigger should produce no spec");
    assert_eq!(report.drops.len(), 1);
    let drop = &report.drops[0];
    assert_eq!(drop.officer_id, "officer-y");
    assert_eq!(drop.ability_name, "Cap");
    assert_eq!(drop.effect_index, 2);
    assert_eq!(drop.reason, "unknown_trigger:onmyaunt");
}

#[test]
fn reports_unmapped_stat() {
    // `weapon_damage` is mapped; `mining_speed` is not. Use a known-good trigger so we land
    // on the stat-mapping early-return.
    let effect = lcars_effect_stat_modify("mining_speed", 0.5, "on_attack");
    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-z",
        "Bd",
        None,
        None,
        0,
        Some(&mut report),
    );
    assert!(out.is_none());
    assert_eq!(report.drops.len(), 1);
    assert_eq!(report.drops[0].reason, "unmapped_stat:mining_speed");
}

#[test]
fn reports_unmapped_condition() {
    // Parse LcarsCondition from YAML so we don't need to track every optional field by hand.
    let cond_yaml = "type: definitely_not_a_real_condition\n";
    let condition: LcarsCondition = serde_yaml::from_str(cond_yaml).expect("parse condition");

    let mut effect = lcars_effect_stat_modify("weapon_damage", 0.1, "on_attack");
    effect.condition = Some(condition);

    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-w",
        "Cap",
        None,
        None,
        0,
        Some(&mut report),
    );
    assert!(out.is_none());
    assert_eq!(report.drops.len(), 1);
    assert_eq!(
        report.drops[0].reason,
        "unmapped_condition:definitely_not_a_real_condition"
    );
}

#[test]
fn reports_zero_for_clean_effect() {
    let effect = lcars_effect_stat_modify("weapon_damage", 0.1, "on_attack");
    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-clean",
        "Cap",
        None,
        None,
        0,
        Some(&mut report),
    );
    assert!(out.is_some(), "clean effect should produce a spec");
    assert!(
        report.drops.is_empty(),
        "no drop should be recorded for clean effect, got {:?}",
        report.drops
    );
}

#[test]
fn skips_recording_for_non_combat_tags() {
    let effect = lcars_effect_tag("miningrate:non_combat", "on_attack");
    let mut report = LcarsDropReport::default();
    let out = lcars_effect_to_combat_effect_spec_with_report(
        &effect,
        "test:id",
        "officer-nc",
        "Bd",
        None,
        None,
        0,
        Some(&mut report),
    );
    assert!(out.is_none());
    assert!(
        report.drops.is_empty(),
        ":non_combat tags are explicit skips, not silent drops"
    );
}

#[test]
fn aggregation_methods_match_drop_categories() {
    let mut report = LcarsDropReport::default();
    for (tag, officer) in [
        ("foo:unmapped", "alpha"),
        ("foo:unmapped", "beta"),
        ("bar:unmapped", "alpha"),
    ] {
        let effect = lcars_effect_tag(tag, "on_attack");
        let _ = lcars_effect_to_combat_effect_spec_with_report(
            &effect,
            "id",
            officer,
            "Cap",
            None,
            None,
            0,
            Some(&mut report),
        );
    }
    let cats = report.category_counts();
    assert_eq!(cats.len(), 1);
    assert_eq!(cats[0].0, "unmapped_tag");
    assert_eq!(cats[0].1, 3); // total
    assert_eq!(cats[0].2, 2); // distinct officers

    let reasons = report.reasons_by_count();
    assert_eq!(reasons[0], ("unmapped_tag:foo".to_string(), 2));
    assert_eq!(reasons[1], ("unmapped_tag:bar".to_string(), 1));

    let officers = report.officers_by_count();
    assert_eq!(officers[0].0, "alpha");
    assert_eq!(officers[0].1, 2);
}

/// Drift detector for the bundled LCARS YAML. The baseline reflects the catalog at the time
/// this test landed; if it drifts ±5% the test fails and the engineer must explicitly bless
/// the new number (e.g. after Step 3 lands more `combat_tag_to_stat` mappings, this baseline
/// should drop).
#[test]
fn production_yaml_drop_baseline() {
    let path = Path::new("data/officers/officers.lcars.yaml");
    if !path.exists() {
        return; // minimal checkouts skip — matches resolver_bundled_lcars_yaml_* convention
    }
    let file = load_lcars_file(path).unwrap();
    let drops = collect_lcars_drops(&file.officers);

    // Baseline recorded 2026-05-16 after Step 1 landed. Update with intent: a *drop* in count
    // is good news (new mappings), but a rise should be investigated before bumping.
    const BASELINE: usize = 107;
    let lo = BASELINE * 95 / 100;
    let hi = BASELINE * 105 / 100;
    let total = drops.len();
    assert!(
        (lo..=hi).contains(&total),
        "drop count {total} outside ±5% of baseline {BASELINE} (range {lo}..={hi}); \
         categories: {:?}",
        drops
            .category_counts()
            .into_iter()
            .map(|(c, n, _)| (c, n))
            .collect::<Vec<_>>()
    );
}
