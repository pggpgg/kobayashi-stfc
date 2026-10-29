//! Source-bound crew-health coefficient tests; numeric materialization is tested
//! separately at scenario setup where profile and crew health are available.
use kobayashi::combat::{AbilityEffect, TimingWindow};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};
use std::collections::HashMap;

#[test]
fn source_bound_officers_defer_raw_crew_health_and_preserve_gates() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let options = ResolveOptions {
        tier: None,
        officer_tiers: Some(HashMap::from([
            ("charvanek-0f1b5c".to_string(), 4),
            ("snw-ortegas-7c79fe".to_string(), 5),
        ])),
        officer_levels: None,
    };
    let buff = resolve_crew_to_buff_set(
        "charvanek-0f1b5c",
        &["snw-ortegas-7c79fe".into()],
        &[],
        &officers,
        &options,
    );
    let charvanek = buff
        .crew
        .seats
        .iter()
        .find(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::BaseArmorPiercingPerCrewHealth(_)
            )
        })
        .expect("Charvanek deferred piercing");
    assert_eq!(
        charvanek.ability.effect,
        AbilityEffect::BaseArmorPiercingPerCrewHealth(8.0)
    );
    assert_eq!(charvanek.ability.timing, TimingWindow::CombatBegin);
    let ortegas = buff
        .crew
        .seats
        .iter()
        .find(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::BaseMitigationStatsPerCrewHealth(_)
            )
        })
        .expect("SNW Ortegas deferred all-defense");
    assert_eq!(
        ortegas.ability.effect,
        AbilityEffect::BaseMitigationStatsPerCrewHealth(18.0)
    );
    assert_eq!(ortegas.ability.timing, TimingWindow::CombatBegin);
    assert!(
        ortegas.ability.condition.is_some(),
        "ship, enemy and hostile gates must survive conversion"
    );
}
