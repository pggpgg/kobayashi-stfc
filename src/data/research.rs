//! Research catalog: rid + level → combat stat bonuses (KOBAYASHI schema).
//! Sync sends (rid, level); we look up record by rid and sum bonuses for levels 1..=level.
//! Same engine stat keys and add/multiply semantics as buildings and forbidden tech.

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchBonusEntry {
    pub stat: String,
    pub value: f64,
    #[serde(default)]
    pub operator: String,
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
    let mut out: HashMap<String, f64> = HashMap::new();
    for lvl in record.levels.iter().filter(|l| l.level <= cap) {
        for bonus in &lvl.bonuses {
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
                    }],
                },
                ResearchLevel {
                    level: 2,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                    }],
                },
                ResearchLevel {
                    level: 3,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "hull_hp".to_string(),
                        value: 0.10,
                        operator: "add".to_string(),
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
}
