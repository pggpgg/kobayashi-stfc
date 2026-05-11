//! Research catalog: rid + level → combat stat bonuses (KOBAYASHI schema).
//! Sync sends (rid, level); we look up record by rid and sum bonuses for levels 1..=level.
//! Same engine stat keys and add/multiply semantics as buildings and forbidden tech.
//!
//! **Across different research projects** (`rid`), per-project cumulative totals are combined
//! additively in [`cumulative_research_bonuses`] (then added into `profile.bonuses`). Multiply
//! operators apply only within a single project's level chain, in ascending level order.
//!
//! Bonuses with [`ResearchBonusConditionKey`] fields set (ship class, faction, morale, burning, etc.)
//! are **excluded** from flat profile merge for attack-scoped stats: conditional `crit_chance` /
//! `crit_damage` and conditional `weapon_damage` feed [`crate::data::profile::research_derived_attack_phase_seats`]
//! (gated attack-phase effects; see `docs/DESIGN.md` research section). By default those seats are built via
//! [`crate::data::combat_effect_spec::CombatEffectSpec`] + [`crate::combat::effect_spec_compile`]
//! (see [`crate::data::research_effect_spec_adapter`]).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::types::OpponentFactionTag;
use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, EffectCategory,
    EffectConfidence, SourceRef,
};

/// One research project (game rid). Bonuses are cumulative over levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchRecord {
    /// Game research id from sync payload.
    pub rid: i64,
    /// Optional display name.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub levels: Vec<ResearchLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchLevel {
    pub level: u32,
    pub bonuses: Vec<ResearchBonusEntry>,
}

/// Optional **defender** / engagement gates (`defender_*`, morale, burning, hull breach) and
/// optional **`attacker_faction`** (player ship owner faction slug from [`crate::data::ship::ShipRecord::faction`]).
/// Defender-gated `crit_*` / `weapon_damage` rows are attack-phase seats, not flat `profile.bonuses`.
/// `attacker_faction` rows merge into [`crate::data::profile::PlayerProfile::research_owner_faction_bonuses`]
/// and apply in [`crate::data::profile::apply_profile_to_attacker`] when the resolved ship matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ResearchBonusConditionKey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defender_ship_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defender_faction: Option<String>,
    /// Player hull owner faction slug (`federation`, `klingon`, …); matches ship extended `faction` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacker_faction: Option<String>,
    /// When non-empty, this row applies to **each** listed owner faction hull (Fed/Klg/Rom “all majors” wording).
    /// Merged into [`crate::data::profile::PlayerProfile::research_owner_faction_bonuses`] under each key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attacker_factions: Vec<String>,
    #[serde(default)]
    pub requires_morale: bool,
    #[serde(default)]
    pub requires_defender_burning: bool,
    #[serde(default)]
    pub requires_defender_hull_breach: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchBonusEntry {
    pub stat: String,
    pub value: f64,
    #[serde(default)]
    pub operator: String,
    #[serde(flatten)]
    pub condition: ResearchBonusConditionKey,
}

impl Default for ResearchBonusEntry {
    fn default() -> Self {
        Self {
            stat: String::new(),
            value: 0.0,
            operator: "add".into(),
            condition: ResearchBonusConditionKey::default(),
        }
    }
}

/// True when this row carries any research condition (hull class, faction, morale, etc.).
pub fn research_bonus_is_conditional(bonus: &ResearchBonusEntry) -> bool {
    bonus.condition.defender_ship_class.is_some()
        || bonus.condition.defender_faction.is_some()
        || bonus.condition.attacker_faction.is_some()
        || !bonus.condition.attacker_factions.is_empty()
        || bonus.condition.requires_morale
        || bonus.condition.requires_defender_burning
        || bonus.condition.requires_defender_hull_breach
}

/// True when this bonus is gated on the **player ship's** owner faction (merged separately from flat `profile.bonuses`).
pub fn research_bonus_is_owner_faction_gated(bonus: &ResearchBonusEntry) -> bool {
    if bonus
        .condition
        .attacker_faction
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    bonus
        .condition
        .attacker_factions
        .iter()
        .any(|s| !s.trim().is_empty())
}

fn is_crit_seat_research_stat(stat: &str) -> bool {
    matches!(stat, "crit_chance" | "crit_damage")
}

/// Stats that, when conditional on [`ResearchBonusConditionKey`], are modeled as attack-phase seats
/// (not flat [`PlayerProfile::bonuses`] merge).
fn is_conditional_attack_seat_research_stat(stat: &str) -> bool {
    is_crit_seat_research_stat(stat) || stat == "weapon_damage"
}

/// Conditional rows that compile to timed/conditional seats (`research_derived_attack_phase_seats_from_spec`)
/// must not duplicate-merge into unconditional `profile.bonuses`.
pub fn research_bonus_skipped_from_flat_profile_merge(bonus: &ResearchBonusEntry) -> bool {
    if !research_bonus_is_conditional(bonus) {
        return false;
    }
    if skip_owner_faction_merge_for_defender_gated_hull_shield(bonus) {
        return true;
    }
    if bonus.condition.defender_faction.is_some()
        && research_defender_conditional_stat_skips_flat_profile(&bonus.stat)
    {
        return true;
    }
    if is_conditional_attack_seat_research_stat(&bonus.stat) {
        return research_bonus_is_owner_faction_gated(bonus)
            || defender_context_for_research_attack_seat(&bonus.condition);
    }
    false
}

fn defender_context_for_research_attack_seat(key: &ResearchBonusConditionKey) -> bool {
    key.defender_faction.is_some()
        || key.defender_ship_class.is_some()
        || key.requires_defender_burning
        || key.requires_defender_hull_breach
        || key.requires_morale
}

/// Stats that omit flat profile merge when `defender_faction` is set (compiled as conditional seats).
/// Stats keyed on **`defender_faction`** where we omit flat [`PlayerProfile::bonuses`] merge and rely on
/// [`research_derived_attack_phase_seats_from_spec`] (narrow research compile path + LCARS/officer fallback).
/// **Excludes** `hull_hp` / `shield_hp` (HullHp multiplier compile requires multiply-shaped ops — research CSV uses Add).
fn research_defender_conditional_stat_skips_flat_profile(stat: &str) -> bool {
    matches!(
        stat,
        "armor"
            | "shield_deflection"
            | "dodge"
            | "pierce"
            | "shield_mitigation"
            | "accuracy"
            | "isolytic_damage"
            | "isolytic_damage_morale"
            | "isolytic_defense"
            | "apex_shred"
            | "apex_barrier"
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCatalog {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub items: Vec<ResearchRecord>,
}

pub const DEFAULT_RESEARCH_CATALOG_PATH: &str = "data/research_catalog.json";
pub const DEFAULT_RESEARCH_CANONICAL_PATH: &str = "data/research_canonical.json";

pub fn load_research_catalog(path: &str) -> Option<ResearchCatalog> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load catalog from a directory's default file (data/research_catalog.json when path is data dir).
pub fn load_research_catalog_from_path(path: &Path) -> Option<ResearchCatalog> {
    let p = if path.is_dir() {
        path.join("research_catalog.json")
    } else {
        path.to_path_buf()
    };
    load_research_catalog(p.to_str()?)
}

// ── Research canonical overrides ────────────────────────────────────────────────────

/// Top-level container for the canonical research override file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCanonicalFile {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub overrides: Vec<ResearchCanonicalOverride>,
}

/// Per-RID canonical override. When present, replaces the auto-generated catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCanonicalOverride {
    pub rid: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    #[serde(default)]
    pub effects: Vec<ResearchCanonicalEffectEntry>,
}

/// One combat effect within a canonical research override. `by_level` maps
/// research level (1-indexed) to engine-scale additive scalar. The adapter sums
/// `by_level[0..player_level]` and compiles to a [`crate::data::combat_effect_spec::CombatEffectSpec`],
/// unless [`Self::snapshot_by_level`] is true: then `by_level[player_level - 1]` is the **total**
/// bonus at that tier (common for STFC “cumulative display” nodes — do not sum prior tiers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCanonicalEffectEntry {
    pub id: String,
    pub modifier: AbilityModifierSpec,
    pub operation: AbilityOperationSpec,
    #[serde(default)]
    pub by_level: Vec<f64>,
    #[serde(default)]
    pub conditions: Vec<AbilityConditionSpec>,
    #[serde(default)]
    pub category: Option<EffectCategory>,
    #[serde(default)]
    pub confidence: Option<EffectConfidence>,
    #[serde(default)]
    pub source_ref: Option<SourceRef>,
    /// When true, use `by_level[level - 1]` only (tier snapshot). When false (default), sum
    /// `by_level[0..level]` (legacy cumulative merge).
    #[serde(default)]
    pub snapshot_by_level: bool,
    /// When set, this effect is **not** compiled to attack-phase seats: it is handled elsewhere
    /// (e.g. extra attacker shield mitigation for incoming damage for the first N rounds).
    #[serde(default)]
    pub incoming_shield_mitigation_rounds: Option<u32>,
}

/// Load canonical research overrides from a JSON file. Returns an empty map when
/// the file is missing or unparseable (canonical overrides are optional).
pub fn load_research_canonical_overrides(path: &str) -> HashMap<i64, ResearchCanonicalOverride> {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let file: ResearchCanonicalFile = match serde_json::from_str(&data) {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::with_capacity(file.overrides.len());
    for ov in file.overrides {
        map.insert(ov.rid, ov);
    }
    map
}

// ── Research bonus helpers ───────────────────────────────────────────────────────────

pub(crate) fn accumulate_research_scalar(current: f64, operator: &str, value: f64) -> f64 {
    let is_multiply = operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul")
        || operator.eq_ignore_ascii_case("mult");
    if is_multiply {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    }
}

fn accumulate_bonus(out: &mut HashMap<String, f64>, stat: &str, operator: &str, value: f64) {
    let key = stat.to_string();
    let current = out.get(&key).copied().unwrap_or(0.0);
    let new_value = accumulate_research_scalar(current, operator, value);
    out.insert(key, new_value);
}

/// Owner-hull rows also keyed on opponent faction must not merge into [`crate::data::profile::PlayerProfile::research_owner_faction_bonuses`]
/// (that map applies vs every hostile). Scenario build applies **faction-only** dual gates via
/// [`cumulative_dual_gate_hull_shield_research_fractions`].
pub(crate) fn skip_owner_faction_merge_for_defender_gated_hull_shield(bonus: &ResearchBonusEntry) -> bool {
    research_bonus_is_owner_faction_gated(bonus)
        && bonus
            .condition
            .defender_faction
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
        && matches!(bonus.stat.as_str(), "hull_hp" | "shield_hp")
}

/// Dual gate applies at scenario build only when the defender gate is **faction-only** (rows that also need morale / hull class /
/// burning still need attack-phase seats — not merged here; see roadmap).
fn dual_gate_hull_shield_scenario_apply_condition(key: &ResearchBonusConditionKey) -> bool {
    key.defender_faction
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
        && !key.requires_morale
        && !key.requires_defender_burning
        && !key.requires_defender_hull_breach
        && key.defender_ship_class.is_none()
}

fn owner_faction_keys_match_bonus(bonus: &ResearchBonusEntry, owner_lc: &str) -> bool {
    if !bonus.condition.attacker_factions.is_empty() {
        return bonus.condition.attacker_factions.iter().any(|raw| {
            raw.trim()
                .to_ascii_lowercase()
                .eq_ignore_ascii_case(owner_lc)
        });
    }
    bonus
        .condition
        .attacker_faction
        .as_ref()
        .is_some_and(|raw| raw.trim().to_ascii_lowercase() == owner_lc)
}

/// Cumulative fractional `hull_hp` / `shield_hp` from research rows gated on **both** player hull faction and **defender_faction**
/// (only rows passing [`dual_gate_hull_shield_scenario_apply_condition`]). Same level walk and `add`/`multiply` semantics as catalog merge.
pub fn cumulative_dual_gate_hull_shield_research_fractions(
    records: &[&ResearchRecord],
    levels_by_rid: &HashMap<i64, u32>,
    owner_faction_slug: Option<&str>,
    defender_faction: OpponentFactionTag,
) -> (f64, f64) {
    let Some(owner_lc) = owner_faction_slug.map(str::trim).filter(|s| !s.is_empty()).map(|s| {
        s.to_ascii_lowercase()
    }) else {
        return (0.0, 0.0);
    };

    let by_rid: HashMap<i64, &ResearchRecord> = records.iter().copied().map(|r| (r.rid, r)).collect();
    let mut hull_frac = 0.0_f64;
    let mut shield_frac = 0.0_f64;

    for (&rid, &level) in levels_by_rid {
        let Some(rec) = by_rid.get(&rid) else {
            continue;
        };
        if level == 0 {
            continue;
        }
        let cap = level.min(max_level(rec));
        let mut level_refs: Vec<(u32, usize, &ResearchLevel)> = rec
            .levels
            .iter()
            .enumerate()
            .filter(|(_, l)| l.level <= cap)
            .map(|(i, l)| (l.level, i, l))
            .collect();
        level_refs.sort_by_key(|(lev, idx, _)| (*lev, *idx));

        for (_, _, lvl) in level_refs {
            for bonus in &lvl.bonuses {
                if !skip_owner_faction_merge_for_defender_gated_hull_shield(bonus) {
                    continue;
                }
                if !dual_gate_hull_shield_scenario_apply_condition(&bonus.condition) {
                    continue;
                }
                if !owner_faction_keys_match_bonus(bonus, owner_lc.as_str()) {
                    continue;
                }
                let Some(ref def_slug) = bonus.condition.defender_faction else {
                    continue;
                };
                let matches_def = match OpponentFactionTag::from_data_slug(def_slug) {
                    Some(t) => t == defender_faction,
                    None => false,
                };
                if !matches_def {
                    continue;
                }

                let op = if bonus.operator.is_empty() {
                    "add"
                } else {
                    bonus.operator.as_str()
                };
                match bonus.stat.as_str() {
                    "hull_hp" => {
                        hull_frac = accumulate_research_scalar(hull_frac, op, bonus.value);
                    }
                    "shield_hp" => {
                        shield_frac = accumulate_research_scalar(shield_frac, op, bonus.value);
                    }
                    _ => {}
                }
            }
        }
    }

    (hull_frac, shield_frac)
}

/// Maximum level defined in this research record.
pub fn max_level(record: &ResearchRecord) -> u32 {
    record.levels.iter().map(|l| l.level).max().unwrap_or(0)
}

/// Returns cumulative bonuses for a single research project up to and including the given level.
/// Level 0 => no bonuses. Level above max => capped at max_level(record).
pub fn cumulative_research_level_bonuses(
    record: &ResearchRecord,
    level: u32,
) -> HashMap<String, f64> {
    if level == 0 {
        return HashMap::new();
    }
    let cap = level.min(max_level(record));
    let mut level_refs: Vec<(u32, usize, &ResearchLevel)> = record
        .levels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.level <= cap)
        .map(|(i, l)| (l.level, i, l))
        .collect();
    level_refs.sort_by_key(|(lev, idx, _)| (*lev, *idx));

    let mut out: HashMap<String, f64> = HashMap::new();
    for (_, _, lvl) in level_refs {
        for bonus in &lvl.bonuses {
            if research_bonus_skipped_from_flat_profile_merge(bonus) {
                continue;
            }
            if research_bonus_is_owner_faction_gated(bonus) {
                continue;
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_bonus(&mut out, &bonus.stat, op, bonus.value);
        }
    }
    out
}

/// Cumulative owner-faction-gated bonuses for one research project (same level walk as [`cumulative_research_level_bonuses`]).
pub fn cumulative_research_level_owner_faction_bonuses(
    record: &ResearchRecord,
    level: u32,
) -> HashMap<String, HashMap<String, f64>> {
    if level == 0 {
        return HashMap::new();
    }
    let cap = level.min(max_level(record));
    let mut level_refs: Vec<(u32, usize, &ResearchLevel)> = record
        .levels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.level <= cap)
        .map(|(i, l)| (l.level, i, l))
        .collect();
    level_refs.sort_by_key(|(lev, idx, _)| (*lev, *idx));

    let mut out: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (_, _, lvl) in level_refs {
        for bonus in &lvl.bonuses {
            if research_bonus_skipped_from_flat_profile_merge(bonus) {
                if !research_bonus_is_owner_faction_gated(bonus) {
                    continue;
                }
                if bonus.condition.defender_faction.is_some()
                    && is_conditional_attack_seat_research_stat(&bonus.stat)
                {
                    continue;
                }
            }
            if !research_bonus_is_owner_faction_gated(bonus) {
                continue;
            }
            if skip_owner_faction_merge_for_defender_gated_hull_shield(bonus) {
                continue;
            }
            let mut faction_keys: Vec<String> = Vec::new();
            if !bonus.condition.attacker_factions.is_empty() {
                for raw in &bonus.condition.attacker_factions {
                    let fk = raw.trim().to_ascii_lowercase();
                    if !fk.is_empty() {
                        faction_keys.push(fk);
                    }
                }
            } else if let Some(raw_f) = bonus.condition.attacker_faction.as_ref() {
                let fk = raw_f.trim().to_ascii_lowercase();
                if !fk.is_empty() {
                    faction_keys.push(fk);
                }
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            for faction_key in faction_keys {
                let inner = out.entry(faction_key).or_default();
                accumulate_bonus(inner, &bonus.stat, op, bonus.value);
            }
        }
    }
    out
}

fn accumulate_conditional_bonus(
    out: &mut HashMap<(ResearchBonusConditionKey, String), f64>,
    key: &ResearchBonusConditionKey,
    stat: &str,
    operator: &str,
    value: f64,
) {
    let map_key = (key.clone(), stat.to_string());
    let current = out.get(&map_key).copied().unwrap_or(0.0);
    let is_multiply = operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul")
        || operator.eq_ignore_ascii_case("mult");
    let new_value = if is_multiply {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(map_key, new_value);
}

/// Cumulative **conditional** bonuses for one research project (same level walk as [`cumulative_research_level_bonuses`]).
pub fn cumulative_research_level_conditional_bonuses(
    record: &ResearchRecord,
    level: u32,
) -> HashMap<(ResearchBonusConditionKey, String), f64> {
    if level == 0 {
        return HashMap::new();
    }
    let cap = level.min(max_level(record));
    let mut level_refs: Vec<(u32, usize, &ResearchLevel)> = record
        .levels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.level <= cap)
        .map(|(i, l)| (l.level, i, l))
        .collect();
    level_refs.sort_by_key(|(lev, idx, _)| (*lev, *idx));

    let mut out: HashMap<(ResearchBonusConditionKey, String), f64> = HashMap::new();
    for (_, _, lvl) in level_refs {
        for bonus in &lvl.bonuses {
            if !research_bonus_is_conditional(bonus) {
                continue;
            }
            if !(is_conditional_attack_seat_research_stat(&bonus.stat)
                || (bonus.condition.defender_faction.is_some()
                    && research_defender_conditional_stat_skips_flat_profile(&bonus.stat)))
            {
                continue;
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_conditional_bonus(&mut out, &bonus.condition, &bonus.stat, op, bonus.value);
        }
    }
    out
}

/// Merge conditional rows across rids (same condition + stat → sum values).
pub fn cumulative_conditional_research_bonuses(
    records: &[&ResearchRecord],
    levels_by_rid: &HashMap<i64, u32>,
) -> HashMap<(ResearchBonusConditionKey, String), f64> {
    let by_rid: HashMap<i64, &ResearchRecord> = records.iter().map(|r| (r.rid, *r)).collect();
    let mut out: HashMap<(ResearchBonusConditionKey, String), f64> = HashMap::new();
    for (&rid, &level) in levels_by_rid {
        let Some(rec) = by_rid.get(&rid) else {
            continue;
        };
        let partial = cumulative_research_level_conditional_bonuses(rec, level);
        for ((key, stat), value) in partial {
            let cur = out
                .get(&(key.clone(), stat.clone()))
                .copied()
                .unwrap_or(0.0);
            out.insert((key, stat), cur + value);
        }
    }
    out
}

fn merge_owner_faction_nested_maps(
    into: &mut HashMap<String, HashMap<String, f64>>,
    partial: HashMap<String, HashMap<String, f64>>,
) {
    for (faction, inner) in partial {
        let dest = into.entry(faction).or_default();
        for (stat, value) in inner {
            accumulate_bonus(dest, &stat, "add", value);
        }
    }
}

/// Returns cumulative **owner-faction-gated** bonuses from multiple research projects.
pub fn cumulative_research_owner_faction_bonuses(
    records: &[&ResearchRecord],
    levels_by_rid: &HashMap<i64, u32>,
) -> HashMap<String, HashMap<String, f64>> {
    let by_rid: HashMap<i64, &ResearchRecord> = records.iter().map(|r| (r.rid, *r)).collect();
    let mut out: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (&rid, &level) in levels_by_rid {
        let Some(rec) = by_rid.get(&rid) else {
            continue;
        };
        let partial = cumulative_research_level_owner_faction_bonuses(rec, level);
        merge_owner_faction_nested_maps(&mut out, partial);
    }
    out
}

/// Returns cumulative bonuses from multiple research projects, given levels by rid.
pub fn cumulative_research_bonuses(
    records: &[&ResearchRecord],
    levels_by_rid: &HashMap<i64, u32>,
) -> HashMap<String, f64> {
    let by_rid: HashMap<i64, &ResearchRecord> = records.iter().map(|r| (r.rid, *r)).collect();
    let mut out: HashMap<String, f64> = HashMap::new();
    for (&rid, &level) in levels_by_rid {
        let Some(rec) = by_rid.get(&rid) else {
            continue;
        };
        let bonuses = cumulative_research_level_bonuses(rec, level);
        for (stat, value) in bonuses {
            // Research bonuses are typically "add"; we aggregate into out as additive.
            accumulate_bonus(&mut out, &stat, "add", value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record() -> ResearchRecord {
        ResearchRecord {
            rid: 100,
            name: Some("Combat I".to_string()),
            data_version: None,
            source_note: None,
            levels: vec![
                ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
                ResearchLevel {
                    level: 3,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "hull_hp".to_string(),
                        value: 0.10,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
            ],
        }
    }

    #[test]
    fn max_level_returns_highest_level() {
        let r = test_record();
        assert_eq!(max_level(&r), 3);
    }

    #[test]
    fn cumulative_level_0_is_empty() {
        let r = test_record();
        let b = cumulative_research_level_bonuses(&r, 0);
        assert!(b.is_empty());
    }

    #[test]
    fn cumulative_level_1_single_bonus() {
        let r = test_record();
        let b = cumulative_research_level_bonuses(&r, 1);
        assert_eq!(b.get("weapon_damage"), Some(&0.05));
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn cumulative_level_2_stacks() {
        let r = test_record();
        let b = cumulative_research_level_bonuses(&r, 2);
        assert_eq!(b.get("weapon_damage"), Some(&0.10));
    }

    #[test]
    fn cumulative_level_3_includes_hull_hp() {
        let r = test_record();
        let b = cumulative_research_level_bonuses(&r, 3);
        assert_eq!(b.get("weapon_damage"), Some(&0.10));
        assert_eq!(b.get("hull_hp"), Some(&0.10));
    }

    #[test]
    fn cumulative_level_above_max_caps() {
        let r = test_record();
        let b = cumulative_research_level_bonuses(&r, 10);
        assert_eq!(b.get("weapon_damage"), Some(&0.10));
        assert_eq!(b.get("hull_hp"), Some(&0.10));
    }

    #[test]
    fn cumulative_research_bonuses_aggregates_multiple() {
        let r1 = test_record();
        let r2 = ResearchRecord {
            rid: 200,
            name: Some("Shields I".to_string()),
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "shield_hp".to_string(),
                    value: 0.08,
                    operator: "add".to_string(),
                    condition: Default::default(),
                }],
            }],
        };
        let records: Vec<&ResearchRecord> = vec![&r1, &r2];
        let mut levels = HashMap::new();
        levels.insert(100i64, 1u32);
        levels.insert(200i64, 1u32);
        let b = cumulative_research_bonuses(&records, &levels);
        assert_eq!(b.get("weapon_damage"), Some(&0.05));
        assert_eq!(b.get("shield_hp"), Some(&0.08));
    }

    #[test]
    fn unknown_rid_skipped() {
        let r = test_record();
        let records: Vec<&ResearchRecord> = vec![&r];
        let mut levels = HashMap::new();
        levels.insert(999i64, 5u32); // not in catalog
        let b = cumulative_research_bonuses(&records, &levels);
        assert!(b.is_empty());
    }

    /// Levels JSON order [2, 1] must not change add→mult composition vs canonical order 1 then 2.
    #[test]
    fn cumulative_level_bonuses_apply_levels_in_ascending_order() {
        let r = ResearchRecord {
            rid: 1,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.10,
                        operator: "mult".to_string(),
                        condition: Default::default(),
                    }],
                },
                ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.10,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                },
            ],
        };
        let b = cumulative_research_level_bonuses(&r, 2);
        let wd = b.get("weapon_damage").copied().unwrap_or_default();
        let mut expected: HashMap<String, f64> = HashMap::new();
        super::accumulate_bonus(&mut expected, "weapon_damage", "add", 0.10);
        super::accumulate_bonus(&mut expected, "weapon_damage", "mult", 0.10);
        let want = *expected.get("weapon_damage").unwrap();
        assert!(
            (wd - want).abs() < 1e-9,
            "got weapon_damage {wd}, want {want} (add then mult)"
        );
    }

    #[test]
    fn conditional_weapon_damage_not_in_flat_level_bonuses() {
        let r = ResearchRecord {
            rid: 502,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.04,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        defender_ship_class: Some("battleship".into()),
                        ..Default::default()
                    },
                }],
            }],
        };
        let flat = cumulative_research_level_bonuses(&r, 1);
        assert!(
            !flat.contains_key("weapon_damage"),
            "conditional weapon_damage must not merge into flat level bonuses"
        );
        let key = ResearchBonusConditionKey {
            defender_ship_class: Some("battleship".into()),
            ..Default::default()
        };
        let cond = cumulative_research_level_conditional_bonuses(&r, 1);
        assert_eq!(
            cond.get(&(key, "weapon_damage".into())).copied(),
            Some(0.04)
        );
    }

    #[test]
    fn conditional_crit_not_in_flat_level_bonuses() {
        let r = ResearchRecord {
            rid: 501,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![
                    ResearchBonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.05,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            defender_ship_class: Some("explorer".into()),
                            ..Default::default()
                        },
                    },
                    ResearchBonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.01,
                        operator: "add".into(),
                        condition: Default::default(),
                    },
                ],
            }],
        };
        let flat = cumulative_research_level_bonuses(&r, 1);
        assert_eq!(flat.get("crit_chance").copied(), Some(0.01));

        let cond = cumulative_research_level_conditional_bonuses(&r, 1);
        let key = ResearchBonusConditionKey {
            defender_ship_class: Some("explorer".into()),
            ..Default::default()
        };
        assert_eq!(cond.get(&(key, "crit_chance".into())).copied(), Some(0.05));
    }

    #[test]
    fn conditional_weapon_damage_burning_not_in_flat_conditional_map() {
        let r = ResearchRecord {
            rid: 503,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.01,
                    operator: "add".into(),
                    condition: ResearchBonusConditionKey {
                        requires_defender_burning: true,
                        ..Default::default()
                    },
                }],
            }],
        };
        assert!(cumulative_research_level_bonuses(&r, 1).is_empty());
        let key = ResearchBonusConditionKey {
            requires_defender_burning: true,
            ..Default::default()
        };
        let cond = cumulative_research_level_conditional_bonuses(&r, 1);
        assert_eq!(
            cond.get(&(key, "weapon_damage".into())).copied(),
            Some(0.01)
        );
    }
}
