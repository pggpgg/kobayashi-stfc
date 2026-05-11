//! Regression test for the 2026 Leslie nerf: his captain's maneuver "Minor Damage Control"
//! is now capped at Hostile Level 51 via the canonical `TargetMaxLevel` token.
//!
//! The devs also referenced a 10-round limit in game data but explicitly stated it is
//! **not enforced** in live combat, so this test does not assert any round cap.

use kobayashi::combat::abilities::{
    AbilityClass, AbilityCondition, CombatContext, CrewSeat, CrewSeatContext,
};
use kobayashi::combat::condition::evaluate_ability_condition;
use kobayashi::combat::types::{EnemyTypes, OpponentFactionTag, ShipType};
use kobayashi::lcars::{load_lcars_file, resolve_officer_ability, ResolveOptions};

const LESLIE_ID: &str = "leslie-975ce0";

fn load_leslie_captain_contexts() -> Vec<CrewSeatContext> {
    let path = format!(
        "{}/data/officers/officers.lcars.yaml",
        env!("CARGO_MANIFEST_DIR")
    );
    let officers = load_lcars_file(&path)
        .expect("load officers.lcars.yaml")
        .officers;
    let leslie = officers
        .into_iter()
        .find(|o| o.id == LESLIE_ID)
        .expect("Leslie must be present in officers.lcars.yaml");
    let cap = leslie
        .captain_ability
        .clone()
        .expect("Leslie must have a captain ability");
    let opts = ResolveOptions::default();
    resolve_officer_ability(
        &leslie,
        &cap,
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        &opts,
        0,
    )
}

fn ctx_with(hull_pct: f64, defender_level: Option<u32>) -> CombatContext {
    CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: hull_pct,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction: OpponentFactionTag::Unknown,
        defender_hull_faction_id: 0,
        defender_ship_type: ShipType::Battleship,
        attacker_ship_type: ShipType::Explorer,
        attacker_ship_id: String::new(),
        defender_is_npc_hostile: true,
        defender_is_player_ship: false,
        attacker_tal_assigned_captain_or_bridge: false,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: EnemyTypes::default(),
        combat_battle_type_id: None,
        defender_level,
    }
}

#[test]
fn leslie_captain_ability_condition_is_and_of_hull_below_and_level_cap_51() {
    let contexts = load_leslie_captain_contexts();
    assert!(
        !contexts.is_empty(),
        "Leslie captain ability must resolve into at least one crew seat context"
    );
    let condition = contexts[0]
        .ability
        .condition
        .clone()
        .expect("Leslie CM must carry a gating condition after 2026 nerf");
    let children = match &condition {
        AbilityCondition::And(children) => children.clone(),
        other => panic!("expected And(_) gating condition, got {other:?}"),
    };

    let has_level_cap = children
        .iter()
        .any(|c| matches!(c, AbilityCondition::DefenderLevelAtMost(51)));
    assert!(
        has_level_cap,
        "Leslie CM must include DefenderLevelAtMost(51); got children: {children:?}"
    );

    let has_hull_below_35 = children.iter().any(|c| {
        matches!(
            c,
            AbilityCondition::StatBelow { stat, threshold_pct }
                if stat == "attacker_hull_hp" && (*threshold_pct - 0.35).abs() < 1e-12
        )
    });
    assert!(
        has_hull_below_35,
        "Leslie CM must keep the hull<35% gate; got children: {children:?}"
    );
}

#[test]
fn leslie_captain_ability_activates_against_level_51_and_below_when_hull_low() {
    let contexts = load_leslie_captain_contexts();
    let condition = contexts[0]
        .ability
        .condition
        .clone()
        .expect("Leslie CM gating condition");

    // Hull below 35% and hostile level <= 51 → ability activates.
    let ctx_eligible = ctx_with(0.10, Some(51));
    assert!(
        evaluate_ability_condition(&condition, &ctx_eligible),
        "Leslie CM must activate vs level-51 hostile with hull < 35%"
    );

    let ctx_eligible_low = ctx_with(0.10, Some(30));
    assert!(
        evaluate_ability_condition(&condition, &ctx_eligible_low),
        "Leslie CM must activate vs level-30 hostile with hull < 35%"
    );
}

#[test]
fn leslie_captain_ability_blocked_against_level_52_and_above() {
    let contexts = load_leslie_captain_contexts();
    let condition = contexts[0]
        .ability
        .condition
        .clone()
        .expect("Leslie CM gating condition");

    // Hull criteria met but level > 51 → ability must not activate (post-nerf cap).
    let ctx_capped = ctx_with(0.10, Some(52));
    assert!(
        !evaluate_ability_condition(&condition, &ctx_capped),
        "Leslie CM must be blocked vs level-52 hostile"
    );

    let ctx_capped_high = ctx_with(0.10, Some(70));
    assert!(
        !evaluate_ability_condition(&condition, &ctx_capped_high),
        "Leslie CM must be blocked vs level-70 hostile"
    );
}

#[test]
fn leslie_captain_ability_still_respects_hull_gate_below_level_cap() {
    let contexts = load_leslie_captain_contexts();
    let condition = contexts[0]
        .ability
        .condition
        .clone()
        .expect("Leslie CM gating condition");

    // Level <= 51 but hull healthy → hull gate still blocks activation.
    let ctx_healthy = ctx_with(0.90, Some(40));
    assert!(
        !evaluate_ability_condition(&condition, &ctx_healthy),
        "Leslie CM must not activate when hull is above 35%, even vs a low-level hostile"
    );
}
