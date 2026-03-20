//! Resolve [ShipAbility] (normalized ship hull abilities) into combat [CrewSeatContext].
//!
//! `effect_type` and `timing` strings come from `data/upstream/data-stfc-space/ship_ability_catalog.json`
//! (filled by contributors per ability id). Names align with LCARS `stat_modify` stats and triggers
//! where possible so the catalog stays consistent with officer DSL.
//!
//! Unknown `timing` or `effect_type` → skipped (same as legacy behavior). Complex LCARS effects
//! that need extra parameters (decay, accumulate, shots duration) are not representable with the
//! single scalar `ShipAbility::value` and are omitted here until the schema grows.

use crate::combat::abilities::{
    Ability, AbilityClass, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::ship::ShipAbility;

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase().replace('-', "_")
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
/// normalizer (`value_is_percentage` → decimal).
pub fn ship_ability_effect_from_catalog(
    effect_type: &str,
    timing: TimingWindow,
    value: f64,
) -> Option<AbilityEffect> {
    match normalize_key(effect_type).as_str() {
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => Some(AbilityEffect::PierceBonus(value)),

        "attack_multiplier" | "weapon_damage" | "attack" => Some(AbilityEffect::AttackMultiplier(value)),

        // LCARS uses this rough mapping for crit stats when they appear as timed crew effects.
        "crit_chance" | "crit_damage" => Some(AbilityEffect::AttackMultiplier(1.0 + value * 0.5)),

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

        _ => None,
    }
}

/// One ship hull ability → one seat context, or None if unsupported.
pub fn ship_ability_to_crew_seat_context(ability: &ShipAbility) -> Option<CrewSeatContext> {
    let timing = parse_ship_ability_timing(&ability.timing)?;
    let effect = ship_ability_effect_from_catalog(&ability.effect_type, timing, ability.value)?;
    Some(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: ability.id.clone(),
            class: AbilityClass::ShipAbility,
            timing,
            boostable: false,
            effect,
            condition: None,
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
    use crate::combat::abilities::AbilityEffect;

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
        )
        .unwrap();
        assert!(matches!(e, AbilityEffect::HullRegen(100.0)));
    }

    #[test]
    fn shots_bonus_requires_round_start_or_combat_begin() {
        assert!(ship_ability_effect_from_catalog("shots_bonus", TimingWindow::RoundStart, 0.2).is_some());
        assert!(ship_ability_effect_from_catalog("shots_bonus", TimingWindow::AttackPhase, 0.2).is_none());
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
}
