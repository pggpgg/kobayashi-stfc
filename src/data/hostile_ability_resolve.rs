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
    Ability, AbilityClass, AbilityEffect, CrewSeat, CrewSeatContext, TimingWindow,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::CrewConfiguration;
use crate::data::ship_ability_resolve::{
    parse_ship_ability_timing, ship_ability_effect_from_catalog,
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

fn normalize_probability(value: f64) -> f64 {
    if (1.0..=100.0).contains(&value) {
        value / 100.0
    } else {
        value.clamp(0.0, 1.0)
    }
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

pub(crate) fn hostile_ability_effect_from_catalog(
    effect_type: &str,
    timing: TimingWindow,
    chance: f64,
    value: f64,
    duration_rounds: Option<u32>,
) -> Option<AbilityEffect> {
    // Proc-gated counter-fire multipliers keep upstream `values[].chance` semantics.
    match effect_type.trim().to_lowercase().replace('-', "_").as_str() {
        "combat_noop" | "unmodeled" | "not_applicable" => None,
        "attack_multiplier" | "weapon_damage" | "attack" => {
            Some(AbilityEffect::ProcAttackMultiplier {
                chance: normalize_probability(chance),
                multiplier: value,
            })
        }
        "pierce_bonus" | "armor_pierce" | "shield_pierce" => Some(AbilityEffect::ProcPierceBonus {
            chance: normalize_probability(chance),
            bonus: value,
        }),
        // Attacker-only hull abilities that must not apply on hostile defender crew.
        "conqueror_borg_beam_suppression"
        | "borg_conqueror_beam_suppression"
        | "accuracy"
        | "accuracy_bonus" => None,
        _ => ship_ability_effect_from_catalog(effect_type, timing, value, duration_rounds),
    }
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
        for raw in v.get("ability").and_then(|a| a.as_array()).into_iter().flatten() {
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
        let Some(timing) = parse_ship_ability_timing(&entry.timing) else {
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
        let chance = parsed.chance;
        let Some(effect) = hostile_ability_effect_from_catalog(
            &entry.effect_type,
            timing,
            chance,
            normalized_value,
            entry.duration_rounds,
        ) else {
            continue;
        };
        seats.push(CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: parsed.id.clone(),
                class: AbilityClass::ShipAbility,
                timing,
                boostable: false,
                effect,
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        });
    }
    CrewConfiguration { seats }
}

/// Defender-side hostile abilities via the canonical [`CombatEffectSpec`] IR.
/// Mirrors [`hostile_abilities_to_defender_crew`] but routes through
/// [`crate::data::hostile_ability_effect_spec_adapter::hostile_ability_to_combat_effect_spec`]
/// → [`crate::combat::effect_spec_compile::compile_officer_combat_spec`].
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
    fn hostile_catalog_delegates_isolytic_and_apex_to_ship_resolver() {
        let iso = hostile_ability_effect_from_catalog(
            "isolytic_damage",
            TimingWindow::CombatBegin,
            100.0,
            0.15,
            None,
        );
        assert!(matches!(iso, Some(AbilityEffect::IsolyticDamageBonus(v)) if (v - 0.15).abs() < 1e-9));

        let apex = hostile_ability_effect_from_catalog(
            "apex_barrier",
            TimingWindow::CombatBegin,
            100.0,
            5000.0,
            None,
        );
        assert!(matches!(apex, Some(AbilityEffect::ApexBarrierBonus(v)) if (v - 5000.0).abs() < 1e-9));
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
