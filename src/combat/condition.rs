//! Central **condition evaluation** and shared construction for combat gating.
//!
//! Runtime checks for [`AbilityCondition`] use [`evaluate_ability_condition`] only. Data layers
//! ([`ShipAbility`](crate::data::ship::ShipAbility), [`ResearchBonusConditionKey`]) build the same
//! enum via [`ability_condition_from_ship_ability`] and [`ability_condition_from_research_bonus_key`]
//! so morale, burning, hull breach, faction, class, and round caps do not diverge across importers.

use crate::combat::abilities::{AbilityCondition, CombatContext};
use crate::combat::types::{OpponentFactionTag, ShipType, MAX_COMBAT_ROUNDS};
use crate::data::research::ResearchBonusConditionKey;
use crate::data::ship::ShipAbility;

/// Collapse a list of conditions into `None`, a single arm, or [`AbilityCondition::And`].
/// Preserves caller push order inside the `And` vector.
pub fn combine_optional_and(parts: Vec<AbilityCondition>) -> Option<AbilityCondition> {
    match parts.len() {
        0 => None,
        1 => Some(parts[0].clone()),
        _ => Some(AbilityCondition::And(parts)),
    }
}

/// Map LCARS / effect stat keys to current fractional HP (0..=1) for condition checks.
/// Unknown keys return [`None`] (treated as false by stat threshold arms in [`evaluate_ability_condition`]).
pub fn stat_pct_for_condition(stat: &str, ctx: &CombatContext) -> Option<f64> {
    match stat {
        "shield_hp" | "shield" => Some(ctx.defender_shield_pct),
        "hull_hp" | "hull" => Some(ctx.defender_hull_pct),
        "attacker_shield_hp" => Some(ctx.attacker_shield_pct),
        "attacker_hull_hp" => Some(ctx.attacker_hull_pct),
        _ => None,
    }
}

/// Inclusive round index bounds (1-based combat rounds use `min: 1`).
#[inline]
pub fn round_index_in_inclusive_range(round_index: u32, min: u32, max: u32) -> bool {
    round_index >= min && round_index <= max
}

/// Inclusive “first N combat rounds” where rounds are 1-based: `round_index` in `1..=n`.
/// Matches [`AbilityCondition::RoundRange`] with `min: 1`, `max: n` when `n > 0`.
pub fn round_in_inclusive_first_n(round_index: u32, n: u32) -> bool {
    n > 0 && round_index_in_inclusive_range(round_index, 1, n)
}

/// Single entry point for [`AbilityCondition`] during combat resolution.
pub fn evaluate_ability_condition(cond: &AbilityCondition, ctx: &CombatContext) -> bool {
    match cond {
        AbilityCondition::StatBelow {
            stat,
            threshold_pct,
        } => stat_pct_for_condition(stat.as_str(), ctx).is_some_and(|pct| pct < *threshold_pct),
        AbilityCondition::StatAbove {
            stat,
            threshold_pct,
        } => stat_pct_for_condition(stat.as_str(), ctx).is_some_and(|pct| pct > *threshold_pct),
        AbilityCondition::RoundRange { min, max } => {
            round_index_in_inclusive_range(ctx.round_index, *min, *max)
        }
        AbilityCondition::MoraleActive => ctx.attacker_morale_active,
        AbilityCondition::DefenderBurning => ctx.defender_burning_active,
        AbilityCondition::DefenderHullBreach => ctx.defender_hull_breach_active,
        AbilityCondition::DefenderFactionIs(expected) => ctx.defender_faction == *expected,
        AbilityCondition::DefenderShipTypeIs(expected) => ctx.defender_ship_type == *expected,
        AbilityCondition::AttackerShipTypeIs(expected) => ctx.attacker_ship_type == *expected,
        AbilityCondition::DefenderIsNpcHostile => ctx.defender_is_npc_hostile,
        AbilityCondition::DefenderIsPlayerShip => ctx.defender_is_player_ship,
        AbilityCondition::Not(inner) => !evaluate_ability_condition(inner, ctx),
        AbilityCondition::And(conds) => conds.iter().all(|c| evaluate_ability_condition(c, ctx)),
        AbilityCondition::Or(conds) => conds.iter().any(|c| evaluate_ability_condition(c, ctx)),
    }
}

/// Hull ability catalog / normalized [`ShipAbility`] → runtime [`AbilityCondition`] (AND of all set gates).
pub fn ability_condition_from_ship_ability(ability: &ShipAbility) -> Option<AbilityCondition> {
    let mut parts: Vec<AbilityCondition> = Vec::new();
    if ability.condition_morale {
        parts.push(AbilityCondition::MoraleActive);
    }
    if ability.condition_defender_burning {
        parts.push(AbilityCondition::DefenderBurning);
    }
    if ability.condition_defender_hull_breach {
        parts.push(AbilityCondition::DefenderHullBreach);
    }
    if let Some(ref slug) = ability.condition_opponent_faction {
        if let Some(tag) = OpponentFactionTag::from_data_slug(slug) {
            parts.push(AbilityCondition::DefenderFactionIs(tag));
        }
    }
    if let Some(ref slug) = ability.condition_opponent_ship_class {
        if let Some(st) = ShipType::from_data_slug(slug) {
            parts.push(AbilityCondition::DefenderShipTypeIs(st));
        }
    }
    if let Some(n) = ability.round_cap.filter(|&n| n > 0) {
        let max_r = n.min(MAX_COMBAT_ROUNDS);
        parts.push(AbilityCondition::RoundRange { min: 1, max: max_r });
    }
    combine_optional_and(parts)
}

/// Research conditional crit rows → same [`AbilityCondition`] tree used by officers and ship seats.
///
/// Push order matches [`ability_condition_from_ship_ability`] (morale / burning / hull breach,
/// then faction / class) so combined `And` trees stay consistent across importers. Round caps are
/// ship-only and do not appear on research keys.
pub fn ability_condition_from_research_bonus_key(
    key: &ResearchBonusConditionKey,
) -> Option<AbilityCondition> {
    let mut parts: Vec<AbilityCondition> = Vec::new();
    if key.requires_morale {
        parts.push(AbilityCondition::MoraleActive);
    }
    if key.requires_defender_burning {
        parts.push(AbilityCondition::DefenderBurning);
    }
    if key.requires_defender_hull_breach {
        parts.push(AbilityCondition::DefenderHullBreach);
    }
    if let Some(ref slug) = key.defender_faction {
        let tag = OpponentFactionTag::from_data_slug(slug)?;
        parts.push(AbilityCondition::DefenderFactionIs(tag));
    }
    if let Some(ref slug) = key.defender_ship_class {
        let st = ShipType::from_data_slug(slug)?;
        parts.push(AbilityCondition::DefenderShipTypeIs(st));
    }
    combine_optional_and(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::{AbilityCondition, CombatContext};
    use crate::combat::types::{OpponentFactionTag, ShipType};

    fn sample_ctx() -> CombatContext {
        CombatContext {
            round_index: 3,
            defender_hull_pct: 0.4,
            defender_shield_pct: 0.8,
            attacker_hull_pct: 0.9,
            attacker_shield_pct: 0.1,
            attacker_morale_active: true,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
        }
    }

    #[test]
    fn combine_optional_and_empty_none_singleton_and() {
        assert!(combine_optional_and(vec![]).is_none());
        let m = AbilityCondition::MoraleActive;
        assert_eq!(combine_optional_and(vec![m.clone()]), Some(m.clone()));
        let anded =
            combine_optional_and(vec![m.clone(), AbilityCondition::DefenderBurning]).unwrap();
        assert_eq!(
            anded,
            AbilityCondition::And(vec![m, AbilityCondition::DefenderBurning])
        );
    }

    #[test]
    fn stat_pct_for_condition_known_and_unknown() {
        let ctx = sample_ctx();
        assert!((stat_pct_for_condition("hull_hp", &ctx).unwrap() - 0.4).abs() < 1e-12);
        assert!(stat_pct_for_condition("unknown_stat", &ctx).is_none());
    }

    #[test]
    fn round_in_inclusive_first_n_matches_round_range_min_1() {
        for n in [1u32, 5u32, 15u32] {
            for r in 1u32..=n {
                assert!(
                    round_in_inclusive_first_n(r, n),
                    "r={r} n={n} should be in range"
                );
                let rr = AbilityCondition::RoundRange { min: 1, max: n };
                assert!(evaluate_ability_condition(
                    &rr,
                    &CombatContext {
                        round_index: r,
                        ..sample_ctx()
                    }
                ));
            }
            assert!(!round_in_inclusive_first_n(n + 1, n));
            assert!(!round_in_inclusive_first_n(0, n));
        }
        assert!(!round_in_inclusive_first_n(1, 0));
    }

    #[test]
    fn evaluate_ability_condition_matches_impl_delegate() {
        let ctx = sample_ctx();
        let cond = AbilityCondition::And(vec![
            AbilityCondition::MoraleActive,
            AbilityCondition::DefenderShipTypeIs(ShipType::Battleship),
        ]);
        assert_eq!(cond.evaluate(&ctx), evaluate_ability_condition(&cond, &ctx));
    }

    #[test]
    fn not_defender_armada_true_when_defender_not_armada() {
        let ctx = sample_ctx();
        let cond = AbilityCondition::Not(Box::new(AbilityCondition::DefenderShipTypeIs(
            ShipType::Armada,
        )));
        assert!(evaluate_ability_condition(&cond, &ctx));
        let ctx_armada = CombatContext {
            defender_ship_type: ShipType::Armada,
            ..sample_ctx()
        };
        assert!(!evaluate_ability_condition(&cond, &ctx_armada));
    }

    #[test]
    fn ship_ability_and_research_key_build_same_and_tree_when_round_cap_absent() {
        let ship = ShipAbility {
            id: "t".into(),
            timing: "round_start".into(),
            effect_type: "attack_multiplier".into(),
            value: 1.0,
            duration_rounds: None,
            condition_morale: true,
            condition_defender_burning: true,
            condition_defender_hull_breach: false,
            condition_opponent_faction: Some("klingon".into()),
            condition_opponent_ship_class: Some("battleship".into()),
            round_cap: None,
            level_scaled_values: None,
        };
        let from_ship = ability_condition_from_ship_ability(&ship).unwrap();
        let key = ResearchBonusConditionKey {
            defender_ship_class: Some("battleship".into()),
            defender_faction: Some("klingon".into()),
            requires_morale: true,
            requires_defender_burning: true,
            requires_defender_hull_breach: false,
        };
        let from_research = ability_condition_from_research_bonus_key(&key).unwrap();
        assert_eq!(from_ship, from_research);
    }
}
