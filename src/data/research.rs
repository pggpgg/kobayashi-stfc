//! Research catalog: rid + level → combat stat bonuses (KOBAYASHI schema).
//! Sync sends (rid, level); we look up record by rid and sum bonuses for levels 1..=level.
//! Same engine stat keys and add/multiply semantics as buildings and forbidden tech.
//!
//! **Across different research projects** (`rid`), per-project cumulative totals are combined
//! additively in [`cumulative_research_bonuses`] (then added into `profile.bonuses`). Multiply
//! operators apply only within a single project's level chain, in ascending level order.
//!
//! Bonuses with [`ResearchBonusConditionKey`] fields set (ship class, faction, morale, etc.) are
//! **excluded** from flat profile merge; [`crate::data::profile::research_derived_attack_phase_seats`]
//! turns them into gated attack-phase crit effects (see `docs/DESIGN.md` research section).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

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

/// When any optional field is set, this bonus is **conditional** (not merged into `profile.bonuses`).
/// Slugs match [`crate::combat::ShipType`] / [`crate::combat::OpponentFactionTag::from_data_slug`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ResearchBonusConditionKey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defender_ship_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defender_faction: Option<String>,
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
        || bonus.condition.requires_morale
        || bonus.condition.requires_defender_burning
        || bonus.condition.requires_defender_hull_breach
}

fn is_crit_seat_research_stat(stat: &str) -> bool {
    matches!(stat, "crit_chance" | "crit_damage")
}

/// Conditional **crit** rows are modeled as attack-phase seats; they must not also merge into `profile.bonuses`.
/// Other conditional stats (if they appear in the catalog) still use the flat profile layer until we add seats.
pub fn research_bonus_skipped_from_flat_profile_merge(bonus: &ResearchBonusEntry) -> bool {
    research_bonus_is_conditional(bonus) && is_crit_seat_research_stat(&bonus.stat)
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

fn accumulate_bonus(out: &mut HashMap<String, f64>, stat: &str, operator: &str, value: f64) {
    let key = stat.to_string();
    let current = out.get(&key).copied().unwrap_or(0.0);
    let is_multiply = operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul")
        || operator.eq_ignore_ascii_case("mult");
    let new_value = if is_multiply {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(key, new_value);
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
            if !is_crit_seat_research_stat(&bonus.stat) {
                continue;
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_conditional_bonus(
                &mut out,
                &bonus.condition,
                &bonus.stat,
                op,
                bonus.value,
            );
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
            let cur = out.get(&(key.clone(), stat.clone())).copied().unwrap_or(0.0);
            out.insert((key, stat), cur + value);
        }
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
    fn conditional_non_crit_still_in_flat_level_bonuses() {
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
        assert_eq!(flat.get("weapon_damage").copied(), Some(0.04));
        assert!(cumulative_research_level_conditional_bonuses(&r, 1).is_empty());
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
        assert_eq!(
            cond.get(&(key, "crit_chance".into())).copied(),
            Some(0.05)
        );
    }
}
