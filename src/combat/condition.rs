//! Shared primitives for combat condition evaluation and construction.
//!
//! Effect runtime gates use [`crate::combat::abilities::AbilityCondition::evaluate`]. This module
//! holds helpers so importers and the engine do not duplicate equivalent logic.

use crate::combat::abilities::{AbilityCondition, CombatContext};

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
/// Unknown keys return [`None`] (treated as false by [`AbilityCondition::StatBelow`] / [`StatAbove`](AbilityCondition::StatAbove)).
pub fn stat_pct_for_condition(stat: &str, ctx: &CombatContext) -> Option<f64> {
    match stat {
        "shield_hp" | "shield" => Some(ctx.defender_shield_pct),
        "hull_hp" | "hull" => Some(ctx.defender_hull_pct),
        "attacker_shield_hp" => Some(ctx.attacker_shield_pct),
        "attacker_hull_hp" => Some(ctx.attacker_hull_pct),
        _ => None,
    }
}

/// Inclusive “first N combat rounds” where rounds are 1-based: `round_index` in `1..=n`.
/// Matches [`AbilityCondition::RoundRange`] with `min: 1`, `max: n` when `n > 0`.
pub fn round_in_inclusive_first_n(round_index: u32, n: u32) -> bool {
    n > 0 && round_index >= 1 && round_index <= n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::AbilityCondition;
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
        }
    }

    #[test]
    fn combine_optional_and_empty_none_singleton_and() {
        assert!(combine_optional_and(vec![]).is_none());
        let m = AbilityCondition::MoraleActive;
        assert_eq!(combine_optional_and(vec![m.clone()]), Some(m.clone()));
        let anded = combine_optional_and(vec![m.clone(), AbilityCondition::DefenderBurning]).unwrap();
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
                assert!(rr.evaluate(&CombatContext {
                    round_index: r,
                    ..sample_ctx()
                }));
            }
            assert!(!round_in_inclusive_first_n(n + 1, n));
            assert!(!round_in_inclusive_first_n(0, n));
        }
        assert!(!round_in_inclusive_first_n(1, 0));
    }
}
