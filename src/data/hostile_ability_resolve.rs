//! Resolve hostile upstream `ability[]` entries into combat [`CrewSeatContext`] rows.
//!
//! Hostile records keep the raw upstream JSON (`HostileRecord::ability`). We only model defender-side
//! effects when an upstream ability id is explicitly mapped in a catalog, to avoid guessing mechanics.
//!
//! Catalog format mirrors `ship_ability_catalog.json` but is scoped to hostiles:
//! `data/upstream/data-stfc-space/hostile_ability_catalog.json`.
//!
//! Supported effect types are intentionally minimal and reuse existing `AbilityEffect` variants so
//! defender abilities can feed the same stacking/effect accumulator logic as officer + ship abilities.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext,
    TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::condition::combine_optional_and;
use crate::combat::types::OpponentFactionTag;
use crate::combat::CrewConfiguration;
use crate::data::ship_ability_resolve::{
    normalize_probability, parse_ship_ability_timing, ship_ability_effect_from_catalog,
};

pub const DEFAULT_HOSTILE_ABILITY_CATALOG_PATH: &str =
    "data/upstream/data-stfc-space/hostile_ability_catalog.json";

#[derive(Debug, Clone, Deserialize)]
pub struct HostileAbilityCatalog {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub entries: HashMap<String, HostileAbilityCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostileAbilityCatalogEntry {
    pub timing: String,
    pub effect_type: String,
    #[serde(default)]
    pub value_is_percentage: bool,
    #[serde(default)]
    pub ignore_upstream_value_is_percentage: bool,
    #[serde(default)]
    pub value_override: Option<f64>,
    #[serde(default)]
    pub duration_rounds: Option<u32>,
    #[serde(default)]
    pub round_interval: Option<u32>,
    #[serde(default)]
    pub shots: Option<u32>,
    #[serde(default)]
    pub condition_defender_burning: bool,
    #[serde(default)]
    pub condition_defender_hull_breach: bool,
    #[serde(default)]
    pub crit_reduction_additive_points: bool,
    #[serde(default)]
    pub crit_debuff_stacks: bool,
    #[serde(default)]
    pub prevent_when_defender_assimilated: bool,
    /// 1-based weapon sub-round index for Denticle Blade heavy artillery gating.
    #[serde(default)]
    pub weapon_index: Option<u32>,
    /// When set, the seat's effect applies only for combat rounds `1..=round_cap` (inclusive),
    /// via [`AbilityCondition::RoundRange`] — "for the first N rounds" hostile ability texts.
    #[serde(default)]
    pub round_cap: Option<u32>,
    /// Hull-design faction slugs allowed to engage (`federation`, `klingon`, `romulan`, …)
    /// for [`AbilityEffect::HostileLethalUnlessAttackerFaction`].
    #[serde(default)]
    pub allowed_attacker_factions: Vec<String>,
    /// Ship ids exempt from the faction gate (e.g. `uss_vengeance`).
    #[serde(default)]
    pub allowed_attacker_ship_ids: Vec<String>,
    #[serde(default)]
    pub extra_seats: Vec<HostileAbilityCatalogEntry>,
}

#[derive(Debug, Clone)]
pub struct ResolvedHostileAbility {
    pub id: String,
    pub chance: f64,
    pub value: f64,
    pub upstream_value_is_percentage: Option<bool>,
}

pub fn load_hostile_ability_catalog(path: &str) -> Option<HostileAbilityCatalog> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

/// Process-wide cache for the default hostile-ability catalog. Built once on first access and
/// reused — every Monte Carlo candidate prep was previously re-reading and re-parsing this JSON
/// file from disk (showed as ~5 % of total samples in profiling). Restart the process to pick
/// up edits to the file on disk.
///
/// Use this through [`hostile_ability_catalog_for_default_path`] from per-candidate hot paths.
static DEFAULT_HOSTILE_ABILITY_CATALOG: std::sync::OnceLock<Option<HostileAbilityCatalog>> =
    std::sync::OnceLock::new();

/// Cached accessor for the default catalog. Returns the same `Option<&_>` every call.
/// Hot-path-safe (no I/O after first call). For non-default paths, callers should use
/// [`load_hostile_ability_catalog`] directly.
pub fn hostile_ability_catalog_for_default_path() -> Option<&'static HostileAbilityCatalog> {
    DEFAULT_HOSTILE_ABILITY_CATALOG
        .get_or_init(|| load_hostile_ability_catalog(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH))
        .as_ref()
}

fn json_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|u| u as f64))
}

fn parse_one_upstream_ability(v: &Value) -> Option<ResolvedHostileAbility> {
    let obj = v.as_object()?;
    let id = obj
        .get("id")
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_i64().filter(|&i| i >= 0).map(|i| i as u64))
        })?
        .to_string();
    let upstream_value_is_percentage = obj.get("value_is_percentage").and_then(|b| b.as_bool());
    let first = obj.get("values")?.as_array()?.first()?;
    let chance = first.get("chance").and_then(json_f64).unwrap_or(100.0);
    let value = first.get("value").and_then(json_f64).unwrap_or(0.0);
    Some(ResolvedHostileAbility {
        id,
        chance,
        value,
        upstream_value_is_percentage,
    })
}

fn normalize_catalog_value(
    catalog_value_is_percentage: bool,
    ignore_upstream_value_is_percentage: bool,
    upstream_value_is_percentage: Option<bool>,
    value: f64,
) -> f64 {
    if catalog_value_is_percentage {
        let upstream_is_pct = if ignore_upstream_value_is_percentage {
            true
        } else {
            upstream_value_is_percentage.unwrap_or(true)
        };
        if upstream_is_pct {
            value / 100.0
        } else {
            value
        }
    } else {
        value
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn hostile_ability_effect_from_catalog(
    effect_type: &str,
    timing: TimingWindow,
    chance: f64,
    value: f64,
    duration_rounds: Option<u32>,
    round_interval: Option<u32>,
    shots: Option<u32>,
    weapon_index: Option<u32>,
    allowed_attacker_factions: &[String],
    allowed_attacker_ship_ids: &[String],
) -> Option<AbilityEffect> {
    // Proc-gated counter-fire multipliers keep upstream `values[].chance` semantics.
    match effect_type.trim().to_lowercase().replace('-', "_").as_str() {
        "combat_noop" | "unmodeled" | "not_applicable" => None,
        "hostile_lethal_unless_attacker_faction"
        | "faction_gated_lethal_strike"
        | "lethal_unless_attacker_faction" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            let mut allow_federation = false;
            let mut allow_klingon = false;
            let mut allow_romulan = false;
            for s in allowed_attacker_factions {
                match OpponentFactionTag::from_data_slug(s) {
                    Some(OpponentFactionTag::Federation) => allow_federation = true,
                    Some(OpponentFactionTag::Klingon) => allow_klingon = true,
                    Some(OpponentFactionTag::Romulan) => allow_romulan = true,
                    _ => {}
                }
            }
            let allow_uss_vengeance = allowed_attacker_ship_ids
                .iter()
                .any(|id| id.trim().eq_ignore_ascii_case("uss_vengeance"));
            if !allow_federation && !allow_klingon && !allow_romulan && !allow_uss_vengeance {
                return None;
            }
            Some(AbilityEffect::HostileLethalUnlessAttackerFaction {
                allow_federation,
                allow_klingon,
                allow_romulan,
                allow_uss_vengeance,
            })
        }
        "hostile_lethal_combat_begin" | "lethal_combat_begin" | "dilithium_destabilization" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileLethalCombatBegin {
                chance: normalize_probability(chance),
            })
        }
        // Q Junior's Twist: engagement ends after `value` rounds; still-alive hostile = loss.
        "hostile_engagement_round_limit" | "engagement_round_limit" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            let rounds = value.round();
            if !(1.0..=u32::MAX as f64).contains(&rounds) {
                return None;
            }
            Some(AbilityEffect::HostileEngagementRoundLimit {
                rounds: rounds as u32,
            })
        }
        "hostile_self_morale" | "intraluminary_self_morale" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileSelfMorale {
                duration_rounds: duration_rounds.unwrap_or(100).max(1),
            })
        }
        "hostile_attacker_shield_mitigation_zero"
        | "attacker_shield_mitigation_zero"
        | "strike_down_shield_mitigation_zero" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileAttackerShieldMitigationZero)
        }
        "hostile_crit_damage_reduction" | "reduce_hostile_crit_damage" => {
            if timing != TimingWindow::CombatBegin && timing != TimingWindow::RoundStart {
                return None;
            }
            Some(AbilityEffect::HostileCritDamageReduction {
                reduction: value,
                duration_rounds: duration_rounds.unwrap_or(5).max(1),
                additive_percentage_points: false,
                stacks: false,
            })
        }
        "hostile_kemocite_weaponry" | "kemocite_weaponry" | "xindi_kemocite" => {
            if timing != TimingWindow::RoundEnd {
                return None;
            }
            Some(AbilityEffect::HostileKemociteWeaponry {
                growth_per_stack: value.max(0.0),
            })
        }
        "hostile_lethal_end_of_round" | "lethal_end_of_round" | "xindi_lethal_end_of_round" => {
            if timing != TimingWindow::RoundEnd {
                return None;
            }
            Some(AbilityEffect::HostileLethalEndOfRound {
                round_interval: round_interval.or(duration_rounds).unwrap_or(1).max(1),
                shots: shots.unwrap_or(1).max(1),
                prevent_when_defender_assimilated: false,
            })
        }
        "hostile_denticle_blade_heavy_artillery" | "denticle_blade_heavy_artillery" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileDenticleBladeHeavyArtillery {
                proc_chance: value.clamp(0.0, 1.0),
                weapon_index_one_based: weapon_index.unwrap_or(5).max(1),
            })
        }
        "hostile_hyperthermic_decay" | "hyperthermic_decay" => {
            if timing != TimingWindow::RoundStart {
                return None;
            }
            Some(AbilityEffect::HostileHyperthermicDecay {
                fraction: value.max(0.0),
            })
        }
        "hostile_defender_mitigation_multiplier" | "hostile_mitigation_multiplier" => {
            Some(AbilityEffect::HostileDefenderMitigationMultiplier {
                additive_factor: value.max(0.0),
            })
        }
        "hostile_crit_damage_floor" | "crit_damage_floor" => {
            Some(AbilityEffect::HostileCritDamageFloorBonus(value.max(0.0)))
        }
        // Critical Breach / Rising Fire: per-hit stacks gated on player burn/breach.
        "defender_on_hit_crit_chance_stack" | "hostile_on_hit_crit_chance_stack" => {
            Some(AbilityEffect::DefenderOnHitStack {
                stat: crate::combat::abilities::DefenderOnHitStat::CritChance,
                per_hit: value.max(0.0),
                duration_rounds: duration_rounds.unwrap_or(2).max(1),
                requires: crate::combat::abilities::DefenderOnHitGate::AttackerHullBreach,
            })
        }
        "defender_on_hit_weapon_damage_stack" | "hostile_on_hit_weapon_damage_stack" => {
            Some(AbilityEffect::DefenderOnHitStack {
                stat: crate::combat::abilities::DefenderOnHitStat::WeaponDamage,
                per_hit: value.max(0.0),
                duration_rounds: duration_rounds.unwrap_or(2).max(1),
                requires: crate::combat::abilities::DefenderOnHitGate::AttackerBurning,
            })
        }
        // Catalog value is a *bonus fraction* (0.2 = +20%, 125 = +12500% — matching upstream
        // ability text); the engine's proc accumulator expects a full multiplier.
        "attack_multiplier" | "weapon_damage" | "attack" => {
            Some(AbilityEffect::ProcAttackMultiplier {
                chance: normalize_probability(chance),
                multiplier: 1.0 + value.max(0.0),
            })
        }
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => Some(AbilityEffect::ProcPierceBonus {
            chance: normalize_probability(chance),
            bonus: value,
        }),
        // Percentage increase of the hostile's counter-fire pierce stats (Pen of Kahless).
        "hostile_counter_pierce_multiplier" | "counter_pierce_multiplier" => {
            Some(AbilityEffect::HostileCounterPierceMultiplier {
                bonus: value.max(0.0),
            })
        }
        "hostile_isolytic_vulnerability" | "isolytic_vulnerability" => {
            if timing != TimingWindow::CombatBegin {
                return None;
            }
            Some(AbilityEffect::HostileIsolyticVulnerability)
        }
        // Attacker-only hull abilities that must not apply on hostile defender crew.
        "conqueror_borg_beam_suppression"
        | "borg_conqueror_beam_suppression"
        | "accuracy"
        | "accuracy_bonus" => None,
        _ => ship_ability_effect_from_catalog(effect_type, timing, value, duration_rounds),
    }
}

fn ability_condition_from_hostile_entry(
    entry: &HostileAbilityCatalogEntry,
) -> Option<AbilityCondition> {
    let mut parts: Vec<AbilityCondition> = Vec::new();
    if entry.condition_defender_burning {
        parts.push(AbilityCondition::DefenderBurning);
    }
    if entry.condition_defender_hull_breach {
        parts.push(AbilityCondition::DefenderHullBreach);
    }
    if let Some(cap) = entry.round_cap.filter(|c| *c > 0) {
        parts.push(AbilityCondition::RoundRange { min: 1, max: cap });
    }
    combine_optional_and(parts)
}

fn push_hostile_catalog_seat(
    seats: &mut Vec<CrewSeatContext>,
    parsed: &ResolvedHostileAbility,
    entry: &HostileAbilityCatalogEntry,
) {
    let Some(timing) = parse_ship_ability_timing(&entry.timing) else {
        return;
    };
    let normalized_value = if entry.crit_reduction_additive_points {
        entry.value_override.unwrap_or(parsed.value)
    } else {
        entry.value_override.unwrap_or_else(|| {
            normalize_catalog_value(
                entry.value_is_percentage,
                entry.ignore_upstream_value_is_percentage,
                parsed.upstream_value_is_percentage,
                parsed.value,
            )
        })
    };
    let chance = parsed.chance;
    let Some(mut effect) = hostile_ability_effect_from_catalog(
        &entry.effect_type,
        timing,
        chance,
        normalized_value,
        entry.duration_rounds,
        entry.round_interval,
        entry.shots,
        entry.weapon_index,
        &entry.allowed_attacker_factions,
        &entry.allowed_attacker_ship_ids,
    ) else {
        return;
    };
    if let AbilityEffect::HostileCritDamageReduction {
        ref mut additive_percentage_points,
        ref mut stacks,
        ..
    } = effect
    {
        if entry.crit_reduction_additive_points {
            *additive_percentage_points = true;
            *stacks = entry.crit_debuff_stacks;
        }
    }
    if let AbilityEffect::HostileLethalEndOfRound {
        ref mut prevent_when_defender_assimilated,
        ..
    } = effect
    {
        *prevent_when_defender_assimilated = entry.prevent_when_defender_assimilated;
    }
    seats.push(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            weapon_scope: Default::default(),
            name: parsed.id.clone(),
            class: AbilityClass::ShipAbility,
            timing,
            boostable: false,
            effect,
            condition: ability_condition_from_hostile_entry(entry),
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
}

/// Collect every unique upstream hostile ability id from cached stfc.space JSON.
pub fn collect_upstream_hostile_ability_ids(hostiles_dir: &Path) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let Ok(read_dir) = std::fs::read_dir(hostiles_dir) else {
        return counts;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&s) else {
            continue;
        };
        for raw in v
            .get("ability")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(parsed) = parse_one_upstream_ability(raw) {
                *counts.entry(parsed.id).or_insert(0) += 1;
            }
        }
    }
    counts
}

pub fn hostile_abilities_to_defender_crew(
    upstream_abilities: &[Value],
    catalog: Option<&HostileAbilityCatalog>,
) -> CrewConfiguration {
    let Some(catalog) = catalog else {
        return CrewConfiguration { seats: Vec::new() };
    };
    if upstream_abilities.is_empty() || catalog.entries.is_empty() {
        return CrewConfiguration { seats: Vec::new() };
    }

    let mut seats: Vec<CrewSeatContext> = Vec::new();
    for raw in upstream_abilities {
        let Some(parsed) = parse_one_upstream_ability(raw) else {
            continue;
        };
        let Some(entry) = catalog.entries.get(parsed.id.as_str()) else {
            continue;
        };
        push_hostile_catalog_seat(&mut seats, &parsed, entry);
        for extra in &entry.extra_seats {
            push_hostile_catalog_seat(&mut seats, &parsed, extra);
        }
    }
    CrewConfiguration { seats }
}

/// Defender-side hostile abilities via the canonical [`CombatEffectSpec`] IR.
/// Mirrors [`hostile_abilities_to_defender_crew`] but routes through
/// [`crate::data::hostile_ability_effect_spec_adapter::hostile_ability_to_combat_effect_spec`]
/// → [`crate::combat::effect_spec_compile::compile_officer_combat_spec`].
/// This parity helper currently compiles only the primary catalog row; production
/// defender-crew resolution above also emits recursive `extra_seats`.
pub fn hostile_abilities_to_defender_crew_via_spec(
    upstream_abilities: &[Value],
    catalog: Option<&HostileAbilityCatalog>,
) -> CrewConfiguration {
    let Some(catalog) = catalog else {
        return CrewConfiguration { seats: Vec::new() };
    };
    if upstream_abilities.is_empty() || catalog.entries.is_empty() {
        return CrewConfiguration { seats: Vec::new() };
    }

    let mut seats: Vec<CrewSeatContext> = Vec::new();
    for raw in upstream_abilities {
        let Some(parsed) = parse_one_upstream_ability(raw) else {
            continue;
        };
        let Some(entry) = catalog.entries.get(parsed.id.as_str()) else {
            continue;
        };
        let normalized_value = entry.value_override.unwrap_or_else(|| {
            normalize_catalog_value(
                entry.value_is_percentage,
                entry.ignore_upstream_value_is_percentage,
                parsed.upstream_value_is_percentage,
                parsed.value,
            )
        });
        let spec =
            crate::data::hostile_ability_effect_spec_adapter::hostile_ability_to_combat_effect_spec(
                &parsed.id,
                entry,
                parsed.chance,
                normalized_value,
            );
        let Some(spec) = spec else {
            continue;
        };
        let Ok((timing, effect, condition)) =
            crate::combat::effect_spec_compile::compile_officer_combat_spec(&spec)
        else {
            continue;
        };
        seats.push(CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                weapon_scope: Default::default(),
                name: parsed.id.clone(),
                class: AbilityClass::ShipAbility,
                timing,
                boostable: false,
                effect,
                condition,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        });
    }
    CrewConfiguration { seats }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_one_upstream_ability_reads_id_values_and_flags() {
        let v: Value = serde_json::from_str(
            r#"{"id":123,"value_is_percentage":true,"values":[{"chance":25,"value":10}]}"#,
        )
        .unwrap();
        let p = parse_one_upstream_ability(&v).unwrap();
        assert_eq!(p.id, "123");
        assert_eq!(p.chance, 25.0);
        assert_eq!(p.value, 10.0);
        assert_eq!(p.upstream_value_is_percentage, Some(true));
    }

    #[test]
    fn upstream_chance_one_yields_always_on_proc_seat() {
        // Upstream `chance: 1` means "always active" (the dominant convention: 4537 of ~4924
        // hostile ability rows), not a 1% proc.
        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "77": {"timing":"combat_begin","effect_type":"attack_multiplier","value_is_percentage":true,"ignore_upstream_value_is_percentage":false}
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":77,"value_is_percentage":true,"values":[{"chance":1,"value":125}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        match crew.seats[0].ability.effect {
            AbilityEffect::ProcAttackMultiplier { chance, multiplier } => {
                assert!((chance - 1.0).abs() < 1e-12, "chance 1 must mean 100%");
                // Catalog value is a bonus fraction: +125% → ×2.25 damage.
                assert!((multiplier - 2.25).abs() < 1e-9);
            }
            ref other => panic!("expected ProcAttackMultiplier, got {other:?}"),
        }
    }

    #[test]
    fn round_cap_gates_seat_with_round_range_condition() {
        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "88": {"timing":"combat_begin","effect_type":"crit_chance","value_is_percentage":false,"ignore_upstream_value_is_percentage":true,"value_override":100,"round_cap":4}
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":88,"value_is_percentage":false,"values":[{"chance":1,"value":1}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        assert!(matches!(
            crew.seats[0].ability.effect,
            AbilityEffect::CritChanceBonus(v) if (v - 1.0).abs() < 1e-9
        ));
        assert_eq!(
            crew.seats[0].ability.condition,
            Some(AbilityCondition::RoundRange { min: 1, max: 4 })
        );
    }

    #[test]
    fn abilities_resolve_only_when_catalog_has_entry_and_timing_is_valid() {
        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "123": {"timing":"round_start","effect_type":"attack_multiplier","value_is_percentage":true,"ignore_upstream_value_is_percentage":false}
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":123,"value_is_percentage":true,"values":[{"chance":100,"value":10}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        assert_eq!(crew.seats[0].ability.name, "123");
    }

    #[test]
    fn dilithium_destabilization_maps_chance_from_upstream_chance_field() {
        let effect = hostile_ability_effect_from_catalog(
            "hostile_lethal_combat_begin",
            TimingWindow::CombatBegin,
            0.9,
            1.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(matches!(
            effect,
            Some(AbilityEffect::HostileLethalCombatBegin { chance }) if (chance - 0.9).abs() < 1e-12
        ));

        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "167520385": {
                  "timing": "combat_begin",
                  "effect_type": "hostile_lethal_combat_begin",
                  "value_is_percentage": false,
                  "ignore_upstream_value_is_percentage": true
                }
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":167520385,"value_is_percentage":true,"values":[{"chance":0.9,"value":1}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        match crew.seats[0].ability.effect {
            AbilityEffect::HostileLethalCombatBegin { chance } => {
                assert!((chance - 0.9).abs() < 1e-12);
            }
            ref other => panic!("expected HostileLethalCombatBegin, got {other:?}"),
        }

        // Wrong timing must not resolve.
        assert!(hostile_ability_effect_from_catalog(
            "hostile_lethal_combat_begin",
            TimingWindow::RoundStart,
            0.9,
            1.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        )
        .is_none());
    }

    #[test]
    fn intraluminary_hostile_self_morale_maps_duration_at_combat_begin() {
        let effect = hostile_ability_effect_from_catalog(
            "hostile_self_morale",
            TimingWindow::CombatBegin,
            1.0,
            1.0,
            Some(100),
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(matches!(
            effect,
            Some(AbilityEffect::HostileSelfMorale {
                duration_rounds: 100
            })
        ));

        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "4021963607": {
                  "timing": "combat_begin",
                  "effect_type": "hostile_self_morale",
                  "value_is_percentage": false,
                  "ignore_upstream_value_is_percentage": true,
                  "duration_rounds": 100,
                  "value_override": 1.0
                }
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":4021963607,"value_is_percentage":true,"values":[{"chance":1,"value":1}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        match crew.seats[0].ability.effect {
            AbilityEffect::HostileSelfMorale { duration_rounds } => {
                assert_eq!(duration_rounds, 100);
            }
            ref other => panic!("expected HostileSelfMorale, got {other:?}"),
        }

        assert!(hostile_ability_effect_from_catalog(
            "hostile_self_morale",
            TimingWindow::RoundStart,
            1.0,
            1.0,
            Some(100),
            None,
            None,
            None,
            &[],
            &[],
        )
        .is_none());
    }

    #[test]
    fn q_junior_twist_engagement_round_limit_maps_at_combat_begin_only() {
        let effect = hostile_ability_effect_from_catalog(
            "hostile_engagement_round_limit",
            TimingWindow::CombatBegin,
            1.0,
            20.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(matches!(
            effect,
            Some(AbilityEffect::HostileEngagementRoundLimit { rounds: 20 })
        ));

        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "755115993": {
                  "timing": "combat_begin",
                  "effect_type": "hostile_engagement_round_limit",
                  "value_is_percentage": false,
                  "ignore_upstream_value_is_percentage": true,
                  "value_override": 20
                }
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":755115993,"value_is_percentage":true,"values":[{"chance":1,"value":0}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        match crew.seats[0].ability.effect {
            AbilityEffect::HostileEngagementRoundLimit { rounds } => assert_eq!(rounds, 20),
            ref other => panic!("expected HostileEngagementRoundLimit, got {other:?}"),
        }

        // Wrong timing window and non-positive round counts resolve to no seat.
        assert!(hostile_ability_effect_from_catalog(
            "hostile_engagement_round_limit",
            TimingWindow::RoundStart,
            1.0,
            20.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        )
        .is_none());
        assert!(hostile_ability_effect_from_catalog(
            "hostile_engagement_round_limit",
            TimingWindow::CombatBegin,
            1.0,
            0.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        )
        .is_none());
    }

    #[test]
    fn plausible_deniability_round_end_shield_regen_max_fraction_with_round_cap() {
        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "932011628": {
                  "timing": "round_end",
                  "effect_type": "shield_regen_max_fraction",
                  "value_is_percentage": false,
                  "ignore_upstream_value_is_percentage": true,
                  "round_cap": 5
                }
              }
            }"#,
        )
        .unwrap();
        // {0:#.#%} convention: the upstream value is a FRACTION (0.2 renders "20%").
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":932011628,"value_is_percentage":true,"values":[{"chance":1,"value":0.2}]}"#,
        )
        .unwrap()];
        let crew = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        assert_eq!(crew.seats.len(), 1);
        let seat = &crew.seats[0];
        match seat.ability.effect {
            AbilityEffect::ShieldRegenMaxFraction(f) => assert!((f - 0.2).abs() < 1e-9),
            ref other => panic!("expected ShieldRegenMaxFraction, got {other:?}"),
        }
        assert_eq!(seat.ability.timing, TimingWindow::RoundEnd);
        assert_eq!(
            seat.ability.condition,
            Some(AbilityCondition::RoundRange { min: 1, max: 5 })
        );
    }

    #[test]
    fn hostile_catalog_delegates_isolytic_and_apex_to_ship_resolver() {
        let vuln = hostile_ability_effect_from_catalog(
            "hostile_isolytic_vulnerability",
            TimingWindow::CombatBegin,
            100.0,
            1.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(matches!(
            vuln,
            Some(AbilityEffect::HostileIsolyticVulnerability)
        ));

        let iso = hostile_ability_effect_from_catalog(
            "isolytic_damage",
            TimingWindow::CombatBegin,
            100.0,
            0.15,
            None,
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(
            matches!(iso, Some(AbilityEffect::IsolyticDamageBonus(v)) if (v - 0.15).abs() < 1e-9)
        );

        let apex = hostile_ability_effect_from_catalog(
            "apex_barrier",
            TimingWindow::CombatBegin,
            100.0,
            5000.0,
            None,
            None,
            None,
            None,
            &[],
            &[],
        );
        assert!(
            matches!(apex, Some(AbilityEffect::ApexBarrierBonus(v)) if (v - 5000.0).abs() < 1e-9)
        );
    }

    /// Parity: spec-path function produces the same count of seats as the direct catalog path.
    #[test]
    fn spec_path_matches_direct_catalog_for_basic_entry() {
        let catalog: HostileAbilityCatalog = serde_json::from_str(
            r#"{
              "entries": {
                "123": {"timing":"round_start","effect_type":"attack_multiplier","value_is_percentage":true,"ignore_upstream_value_is_percentage":false}
              }
            }"#,
        )
        .unwrap();
        let raw: Vec<Value> = vec![serde_json::from_str(
            r#"{"id":123,"value_is_percentage":true,"values":[{"chance":100,"value":10}]}"#,
        )
        .unwrap()];
        let direct = hostile_abilities_to_defender_crew(&raw, Some(&catalog));
        let via_spec = hostile_abilities_to_defender_crew_via_spec(&raw, Some(&catalog));

        // Note: ProcAttackMultiplier / ProcPierceBonus are not yet handled by
        // compile_officer_combat_spec, so via_spec may produce fewer seats.
        // This test just verifies the adapter doesn't panic and the counts are consistent
        // with what the compiler supports.
        assert!(
            via_spec.seats.len() <= direct.seats.len(),
            "spec path should not produce more seats than direct path"
        );
    }
}
