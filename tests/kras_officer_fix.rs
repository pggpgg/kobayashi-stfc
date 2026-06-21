//! Kras production wiring — captain maneuver debuff + bridge officer-stat debuff.

use kobayashi::combat::abilities::{AbilityClass, AbilityEffect, TimingWindow};
use kobayashi::data::combat_effect_spec::AbilityConditionSpec;
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, lcars_effect_coverage,
    resolve_crew_to_buff_set, MechanicCoverageTier, OfficerStatOpponentScope, ResolveOptions,
};

fn bundled() -> Option<(
    std::collections::HashMap<String, kobayashi::lcars::LcarsOfficer>,
    ResolveOptions,
)> {
    let file = build_officer_model_file_default().ok()?;
    let officers = index_lcars_officers_by_id(file.officers);
    Some((
        officers,
        ResolveOptions {
            tier: Some(1),
            ..ResolveOptions::default()
        },
    ))
}

#[test]
fn production_kras_captain_opponent_maneuver_debuff_seat() {
    let (officers, opts) = bundled().expect("bundled officers");
    let officer = officers.get("kras-a47042").expect("kras");
    let cap_eff = officer
        .captain_ability
        .as_ref()
        .and_then(|a| a.effects.first())
        .expect("captain effect");
    assert_eq!(cap_eff.effect_type, "stat_modify");
    assert_eq!(cap_eff.stat.as_deref(), Some("opponent_captain_maneuver"));
    assert_eq!(cap_eff.operator.as_deref(), Some("sub"));

    let buff = resolve_crew_to_buff_set("kras-a47042", &[], &[], &officers, &opts);
    let cap = buff
        .crew
        .seats
        .iter()
        .find(|s| s.ability.class == AbilityClass::CaptainManeuver)
        .expect("Kras captain maneuver seat");
    assert_eq!(cap.ability.timing, TimingWindow::CombatBegin);
    assert!(
        matches!(
            cap.ability.effect,
            AbilityEffect::OpponentCaptainManeuverMultiplier(m) if (m - 0.8).abs() < 1e-9
        ),
        "rank-1 Art of War should be 20% reduction (mult=0.8); got {:?}",
        cap.ability.effect
    );
    assert!(
        cap.ability.condition.is_some(),
        "captain debuff must carry defender_is_player_ship condition"
    );
}

#[test]
fn production_kras_bridge_coverage_and_pending() {
    let (officers, opts) = bundled().expect("bundled officers");
    let officer = officers.get("kras-a47042").expect("kras");
    let bridge_eff = officer
        .bridge_ability
        .as_ref()
        .and_then(|a| a.effects.first())
        .expect("bridge effect");
    assert_eq!(bridge_eff.operator.as_deref(), Some("sub"));
    let cov = lcars_effect_coverage(bridge_eff, "kras-a47042", &opts);
    assert_eq!(
        cov.tier,
        MechanicCoverageTier::Implemented,
        "bridge Know your Enemy should be Implemented (pending path); got {:?}",
        cov.pathway
    );

    let buff = resolve_crew_to_buff_set("kras-a47042", &[], &[], &officers, &opts);
    let pending = buff
        .pending_officer_stat_contributions
        .iter()
        .find(|p| p.stat_key == "officer_stat_all" && !p.target_attacker)
        .expect("enemy_bridge pending row");
    assert!((pending.value - 0.20).abs() < 1e-9);
    assert_eq!(
        pending.opponent_scope,
        OfficerStatOpponentScope::BridgeOfficers
    );
    assert!(matches!(
        pending.conditions.as_slice(),
        [AbilityConditionSpec::DefenderIsPlayerShip]
    ));
}
