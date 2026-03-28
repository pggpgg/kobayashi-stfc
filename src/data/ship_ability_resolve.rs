//! Resolve [ShipAbility] (normalized ship hull abilities) into combat [CrewSeatContext].
//!
//! `effect_type` and `timing` strings come from `data/upstream/data-stfc-space/ship_ability_catalog.json`
//! (filled by contributors per ability id). Names align with LCARS `stat_modify` stats and triggers
//! where possible so the catalog stays consistent with officer DSL.
//!
//! Unknown `timing` or `effect_type` → skipped (same as legacy behavior). Complex LCARS effects
//! that need extra parameters (decay, accumulate, shots duration) are not representable with the
//! single scalar `ShipAbility::value` and are omitted here until the schema grows.
//!
//! **Morale (U.S.S. Enterprise-D / Galaxy Class):** Cumulative weapon damage uses
//! [`AbilityCondition::MoraleActive`], satisfied when the primary round-start [AbilityEffect::Morale]
//! roll succeeds that round.
//!
//! **Accuracy:** `accuracy` / `accuracy_bonus` at **combat begin** are summed by
//! [`sum_combat_begin_accuracy_from_ship_abilities`] and folded into pre-mitigation [`AttackerStats`]
//! (not a crew seat). Other timings are not modeled yet.

use crate::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::types::{OpponentFactionTag, EPSILON, MAX_COMBAT_ROUNDS};
use crate::data::ship::ShipAbility;

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase().replace('-', "_")
}

fn opponent_faction_from_catalog_slug(s: &str) -> Option<OpponentFactionTag> {
    match normalize_key(s).as_str() {
        "unknown" => Some(OpponentFactionTag::Unknown),
        "federation" => Some(OpponentFactionTag::Federation),
        "klingon" => Some(OpponentFactionTag::Klingon),
        "romulan" => Some(OpponentFactionTag::Romulan),
        "borg" => Some(OpponentFactionTag::Borg),
        "cardassian" => Some(OpponentFactionTag::Cardassian),
        "augment" => Some(OpponentFactionTag::Augment),
        "dominion" => Some(OpponentFactionTag::Dominion),
        "mirror_universe" => Some(OpponentFactionTag::MirrorUniverse),
        "assimilated" => Some(OpponentFactionTag::Assimilated),
        "ex_borg" | "exborg" => Some(OpponentFactionTag::ExBorg),
        "swarm" => Some(OpponentFactionTag::Swarm),
        "actian" => Some(OpponentFactionTag::Actian),
        "gorn_hunting_pack" | "gorn" => Some(OpponentFactionTag::GornHuntingPack),
        "xindi" => Some(OpponentFactionTag::Xindi),
        "breen" => Some(OpponentFactionTag::Breen),
        _ => None,
    }
}

fn conditions_for_ship_ability(ability: &ShipAbility) -> Option<AbilityCondition> {
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
        if let Some(tag) = opponent_faction_from_catalog_slug(slug) {
            parts.push(AbilityCondition::DefenderFactionIs(tag));
        }
    }
    match parts.len() {
        0 => None,
        1 => Some(parts[0].clone()),
        _ => Some(AbilityCondition::And(parts)),
    }
}

/// Map catalog timing string to engine window. Accepts Kobayashi canonical names and LCARS-style triggers.
pub fn parse_ship_ability_timing(s: &str) -> Option<TimingWindow> {
    match normalize_key(s).as_str() {
        "combat_begin" | "combatstart" | "on_combat_start" | "passive" => {
            Some(TimingWindow::CombatBegin)
        }
        "round_start" | "roundstart" | "on_round_start" => Some(TimingWindow::RoundStart),
        "attack_phase" | "on_attack" | "on_hit" | "on_critical" | "criticalshotfired"
        | "enemytakeshit" => Some(TimingWindow::AttackPhase),
        "defense_phase" | "on_defense" | "hittaken" => Some(TimingWindow::DefensePhase),
        "round_end" | "roundend" | "on_round_end" => Some(TimingWindow::RoundEnd),
        "shield_break" | "on_shield_break" | "shieldsdepleted" | "targetshieldsdepleted" => {
            Some(TimingWindow::ShieldBreak)
        }
        "kill" | "on_kill" | "battlewon" => Some(TimingWindow::Kill),
        "hull_breach" | "on_hull_breach" | "hulldamagetaken" => Some(TimingWindow::HullBreach),
        "receive_damage" | "on_receive_damage" | "shielddamagetaken" => {
            Some(TimingWindow::ReceiveDamage)
        }
        "combat_end" | "on_combat_end" => Some(TimingWindow::CombatEnd),
        _ => None,
    }
}

/// If the game stores probabilities as whole percents (e.g. 25 = 25%), fold to [0, 1].
fn normalize_probability(value: f64) -> f64 {
    if (1.0..=100.0).contains(&value) {
        value / 100.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// Map catalog `effect_type` + timing to [AbilityEffect]. `value` is already scaled by the
/// normalizer (`value_is_percentage` → decimal). `duration_rounds` is used by some hull abilities
/// (e.g. hostile crit reduction duration).
pub fn ship_ability_effect_from_catalog(
    effect_type: &str,
    timing: TimingWindow,
    value: f64,
    duration_rounds: Option<u32>,
) -> Option<AbilityEffect> {
    match normalize_key(effect_type).as_str() {
        // Catalogued for every hull; no combat seat until modeled explicitly.
        "combat_noop" | "unmodeled" | "not_applicable" => None,
        "hostile_crit_damage_reduction" | "reduce_hostile_crit_damage" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileCritDamageReduction {
                reduction: value.clamp(0.0, 0.95),
                duration_rounds: duration_rounds.unwrap_or(5).max(1),
            })
        }
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => Some(AbilityEffect::PierceBonus(value)),

        "attack_multiplier" | "weapon_damage" | "attack" => Some(AbilityEffect::AttackMultiplier(value)),

        // Per-round cumulative weapon damage (round n → additive modifier n * value on pre-attack multiplier).
        "accumulating_attack_multiplier" | "cumulative_weapon_damage" => {
            if timing != TimingWindow::RoundStart {
                return None;
            }
            let growth = value;
            let ceiling = 1.0 + growth * MAX_COMBAT_ROUNDS as f64 + 1.0;
            Some(AbilityEffect::AccumulatingAttackMultiplier {
                initial: 1.0,
                growth_per_round: growth,
                ceiling,
            })
        },

        "crit_chance" => Some(AbilityEffect::CritChanceBonus(normalize_probability(value))),
        "crit_damage" => Some(AbilityEffect::CritDamageMultiplier((1.0 + value).max(EPSILON))),

        "apex_shred" => Some(AbilityEffect::ApexShredBonus(value)),
        "apex_barrier" => Some(AbilityEffect::ApexBarrierBonus(value)),

        "shield_regen" | "shield_hp_repair" => Some(AbilityEffect::ShieldRegen(value)),

        "hull_regen" | "hull_hp_repair" | "hull_repair" => {
            if timing == TimingWindow::Kill {
                Some(AbilityEffect::OnKillHullRegen(value))
            } else {
                Some(AbilityEffect::HullRegen(value))
            }
        }

        "isolytic_damage" => Some(AbilityEffect::IsolyticDamageBonus(value)),
        "isolytic_defense" => Some(AbilityEffect::IsolyticDefenseBonus(value)),
        "isolytic_cascade" | "isolytic_cascade_damage" => {
            Some(AbilityEffect::IsolyticCascadeDamageBonus(value))
        }

        "shield_mitigation" => Some(AbilityEffect::ShieldMitigationBonus(value)),

        "morale" => Some(AbilityEffect::Morale(normalize_probability(value))),

        "assimilated" => Some(AbilityEffect::Assimilated {
            chance: normalize_probability(value),
            duration_rounds: 1,
        }),

        "hull_breach" => Some(AbilityEffect::HullBreach {
            chance: normalize_probability(value),
            duration_rounds: 1,
            requires_critical: false,
        }),

        "burning" => Some(AbilityEffect::Burning {
            chance: normalize_probability(value),
            duration_rounds: 1,
        }),

        "shots" | "weapon_shots" | "shots_per_weapon" | "shots_per_attack" | "shots_bonus" => {
            if matches!(timing, TimingWindow::RoundStart | TimingWindow::CombatBegin) {
                Some(AbilityEffect::ShotsBonus {
                    chance: 1.0,
                    bonus_pct: value,
                    duration_rounds: 1,
                })
            } else {
                None
            }
        }

        // Handled only via [`sum_combat_begin_accuracy_from_ship_abilities`] so dodge mitigation
        // and pierce-through see stacked accuracy before combat. Non-combat-begin timing is not
        // modeled in the engine yet.
        "accuracy" | "accuracy_bonus" => None,

        _ => None,
    }
}

/// Flat accuracy from hull abilities at combat begin, folded into [`crate::combat::AttackerStats`]
/// before hostile mitigation / pierce-through (see [`crate::optimizer::monte_carlo::scenario::effective_attacker_stats_for_mitigation`]).
pub fn sum_combat_begin_accuracy_from_ship_abilities(abilities: &[ShipAbility]) -> f64 {
    let mut sum = 0.0;
    for a in abilities {
        if parse_ship_ability_timing(&a.timing) != Some(TimingWindow::CombatBegin) {
            continue;
        }
        match normalize_key(&a.effect_type).as_str() {
            "accuracy" | "accuracy_bonus" => sum += a.value,
            _ => {}
        }
    }
    sum
}

/// One ship hull ability → one seat context, or None if unsupported.
pub fn ship_ability_to_crew_seat_context(ability: &ShipAbility) -> Option<CrewSeatContext> {
    if let Some(ref slug) = ability.condition_opponent_faction {
        if opponent_faction_from_catalog_slug(slug).is_none() {
            return None;
        }
    }
    let timing = parse_ship_ability_timing(&ability.timing)?;
    let effect = ship_ability_effect_from_catalog(
        &ability.effect_type,
        timing,
        ability.value,
        ability.duration_rounds,
    )?;
    let condition = conditions_for_ship_ability(ability);
    Some(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: ability.id.clone(),
            class: AbilityClass::ShipAbility,
            timing,
            boostable: false,
            effect,
            condition,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    })
}

/// All supported abilities on a ship (unknown combinations dropped).
pub fn ship_abilities_to_crew_seat_contexts(abilities: &[ShipAbility]) -> Vec<CrewSeatContext> {
    abilities
        .iter()
        .filter_map(ship_ability_to_crew_seat_context)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::{AbilityCondition, AbilityEffect};
    use crate::combat::types::OpponentFactionTag;

    #[test]
    fn timing_accepts_lcars_style_aliases() {
        assert_eq!(
            parse_ship_ability_timing("on_round_start"),
            Some(TimingWindow::RoundStart)
        );
        assert_eq!(
            parse_ship_ability_timing("on_shield_break"),
            Some(TimingWindow::ShieldBreak)
        );
    }

    #[test]
    fn hull_repair_on_kill_maps_to_on_kill_hull_regen() {
        let e = ship_ability_effect_from_catalog(
            "hull_repair",
            TimingWindow::Kill,
            500.0,
            None,
        )
        .unwrap();
        assert!(matches!(e, AbilityEffect::OnKillHullRegen(500.0)));
    }

    #[test]
    fn hull_repair_not_kill_maps_to_hull_regen() {
        let e = ship_ability_effect_from_catalog(
            "hull_repair",
            TimingWindow::RoundEnd,
            100.0,
            None,
        )
        .unwrap();
        assert!(matches!(e, AbilityEffect::HullRegen(100.0)));
    }

    #[test]
    fn crit_stats_map_to_typed_effects_not_attack_multiplier() {
        let cc = ship_ability_effect_from_catalog("crit_chance", TimingWindow::RoundStart, 0.15, None)
            .expect("crit_chance");
        assert!(matches!(cc, AbilityEffect::CritChanceBonus(v) if (v - 0.15).abs() < 1e-12));
        let cd = ship_ability_effect_from_catalog("crit_damage", TimingWindow::RoundStart, 0.2, None)
            .expect("crit_damage");
        assert!(matches!(cd, AbilityEffect::CritDamageMultiplier(m) if (m - 1.2).abs() < 1e-12));
    }

    #[test]
    fn sum_combat_begin_accuracy_ignores_non_combat_begin_rows() {
        let abilities = vec![
            ShipAbility {
                id: "cb".into(),
                timing: "combat_begin".into(),
                effect_type: "accuracy".into(),
                value: 15.0,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
            },
            ShipAbility {
                id: "rs".into(),
                timing: "round_start".into(),
                effect_type: "accuracy".into(),
                value: 999.0,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
            },
        ];
        assert_eq!(
            sum_combat_begin_accuracy_from_ship_abilities(&abilities),
            15.0
        );
    }

    #[test]
    fn shots_bonus_requires_round_start_or_combat_begin() {
        assert!(ship_ability_effect_from_catalog("shots_bonus", TimingWindow::RoundStart, 0.2, None).is_some());
        assert!(ship_ability_effect_from_catalog("shots_bonus", TimingWindow::AttackPhase, 0.2, None).is_none());
    }

    #[test]
    fn fixture_coverage_file_deserializes_and_resolves() {
        let json = include_str!("../../tests/fixtures/ship_abilities/catalog_effect_coverage.json");
        let abilities: Vec<ShipAbility> = serde_json::from_str(json).expect("fixture JSON");
        let seats = ship_abilities_to_crew_seat_contexts(&abilities);
        assert_eq!(
            seats.len(),
            abilities.len(),
            "each fixture row should resolve; missing mappings?"
        );
    }

    #[test]
    fn accumulating_attack_multiplier_requires_round_start() {
        let g = 0.0085;
        let e = ship_ability_effect_from_catalog(
            "accumulating_attack_multiplier",
            TimingWindow::RoundStart,
            g,
            None,
        )
        .expect("round_start maps");
        let ceiling = 1.0 + g * MAX_COMBAT_ROUNDS as f64 + 1.0;
        assert!(
            matches!(
                e,
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial: 1.0,
                    growth_per_round,
                    ceiling: c
                } if (growth_per_round - g).abs() < 1e-12 && (c - ceiling).abs() < 1e-9
            ),
            "{e:?}"
        );
        assert!(
            ship_ability_effect_from_catalog(
                "accumulating_attack_multiplier",
                TimingWindow::CombatBegin,
                g,
                None,
            )
            .is_none(),
            "must be round_start only"
        );
    }

    #[test]
    fn opponent_faction_slug_merges_into_and_condition() {
        let seat = ship_ability_to_crew_seat_context(&ShipAbility {
            id: "test_vs_klingon".to_string(),
            timing: "combat_begin".to_string(),
            effect_type: "attack_multiplier".to_string(),
            value: 0.1,
            duration_rounds: None,
            condition_morale: true,
            condition_defender_burning: false,
            condition_defender_hull_breach: false,
            condition_opponent_faction: Some("klingon".to_string()),
        })
        .expect("faction + morale");
        assert_eq!(
            seat.ability.condition,
            Some(AbilityCondition::And(vec![
                AbilityCondition::MoraleActive,
                AbilityCondition::DefenderFactionIs(OpponentFactionTag::Klingon),
            ]))
        );
    }

    #[test]
    fn enterprise_d_hull_ability_resolves_to_accumulating_multiplier() {
        let seat = ship_ability_to_crew_seat_context(&ShipAbility {
            id: "448699234".to_string(),
            timing: "round_start".to_string(),
            effect_type: "accumulating_attack_multiplier".to_string(),
            value: 0.0085,
            duration_rounds: None,
            condition_morale: true,
            condition_defender_burning: false,
            condition_defender_hull_breach: false,
            condition_opponent_faction: None,
        })
        .expect("Galaxy Class seat");
        assert_eq!(seat.ability.timing, TimingWindow::RoundStart);
        assert!(matches!(
            seat.ability.effect,
            AbilityEffect::AccumulatingAttackMultiplier { .. }
        ));
        assert_eq!(seat.ability.condition, Some(AbilityCondition::MoraleActive));
    }

    #[test]
    fn hostile_crit_damage_reduction_maps_from_combat_begin() {
        let e = ship_ability_effect_from_catalog(
            "hostile_crit_damage_reduction",
            TimingWindow::CombatBegin,
            0.02,
            Some(5),
        )
        .expect("maps");
        assert!(
            matches!(
                e,
                AbilityEffect::HostileCritDamageReduction {
                    reduction: r,
                    duration_rounds: 5
                } if (r - 0.02).abs() < 1e-12
            ),
            "{e:?}"
        );
        assert!(
            ship_ability_effect_from_catalog(
                "hostile_crit_damage_reduction",
                TimingWindow::RoundStart,
                0.02,
                Some(5),
            )
            .is_none(),
            "must be combat_begin only"
        );
    }
}
