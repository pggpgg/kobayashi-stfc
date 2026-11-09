//! Native personal Syndicate reward provenance and an offline definition report.
//!
//! This module does not merge bonuses into profiles or combatants. Active native
//! BuffIDs already enter combat through their source compilers. Its totals are
//! hypothetical unlocked, unconditional definitions at the requested Ops rank.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

const CATALOG: &str = include_str!("../../data/native_syndicate_rewards.json");
const REVIEWED_CACHE: &str = "b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf";

#[derive(Debug, Deserialize)]
struct Catalog {
    schema_version: u32,
    provenance: Provenance,
    coverage: RewardCoverage,
    config: Config,
    tiers: Vec<Tier>,
    buffs: BTreeMap<u64, BuffSpec>,
    unresolved_buff_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct Provenance {
    static_cache_sha256: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    min_player_level: u32,
}

#[derive(Debug, Deserialize)]
struct Tier {
    level: u32,
    collections: Vec<Collection>,
}

#[derive(Debug, Deserialize)]
struct Collection {
    rewards: Vec<Reward>,
}

#[derive(Debug, Deserialize)]
struct Reward {
    buff_id: u64,
}

#[derive(Debug, Deserialize)]
struct BuffSpec {
    source_group: u32,
    rank_source: String,
    modifier_code: i64,
    operation_code: i64,
    target_code: i64,
    trigger_code: i64,
    condition_codes: Vec<i64>,
    attribute_fields: Vec<u32>,
    unknown_fields: Vec<u32>,
    #[serde(default)]
    num_rounds: u32,
    ranked_value_type: i32,
    ranked_values: Vec<f64>,
    ranked_value_count: usize,
    ranked_chances: Vec<f64>,
    ranked_chance_count: usize,
}

/// Coverage of the complete reviewed static source, including unresolved references.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RewardCoverage {
    pub tier_count: usize,
    pub reward_reference_count: usize,
    pub unique_buff_count: usize,
    pub resolved_buff_count: usize,
}

/// A definition's treatment in the report, never an active-profile eligibility claim.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewardStatus {
    UnconditionalCombat,
    Conditional,
    Unsupported,
    LinkedBuilding,
    Noncombat,
    MissingDefinition,
    RankUnavailable,
}

#[derive(Debug, Serialize)]
pub struct RewardReportRow {
    pub unlock_level: u32,
    pub buff_id: u64,
    pub source_group: Option<u32>,
    pub status: RewardStatus,
    /// Raw native snapshot value; a conditional value is not an applied bonus.
    pub raw_value: Option<f64>,
    pub stats: Vec<String>,
    pub condition_codes: Vec<i64>,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SyndicateRewardReport {
    pub syndicate_level: u32,
    pub operations_level: Option<u32>,
    pub static_cache_sha256: String,
    pub complete_source_coverage: RewardCoverage,
    pub selected_reward_references: usize,
    pub unique_selected_buff_ids: usize,
    /// Fractions except keys explicitly named `*_flat`. Only unconditional definitions enter.
    pub unconditional_combat_bonuses: BTreeMap<String, f64>,
    pub rows: Vec<RewardReportRow>,
    pub limitation: String,
}

fn raw_rank(spec: &BuffSpec, ops: Option<u32>) -> Option<f64> {
    if spec.source_group != 116 || spec.rank_source != "operations_level" {
        return None;
    }
    let rank = usize::try_from(ops?.checked_sub(1)?).ok()?;
    if rank >= spec.ranked_value_count || spec.ranked_values.is_empty() {
        return None;
    }
    let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
    value.is_finite().then_some(value)
}

fn unconditional_stats(spec: &BuffSpec) -> Option<&'static [&'static str]> {
    if spec.trigger_code != 24
        || spec.num_rounds != 0
        || spec.ranked_value_type != 0
        || spec.ranked_values.is_empty()
        || spec.ranked_values.len() > spec.ranked_value_count
        || spec.ranked_values.iter().any(|v| !v.is_finite())
        || spec.ranked_chances.is_empty()
        || spec.ranked_chances.len() > spec.ranked_chance_count
        || spec.ranked_chances.iter().any(|v| *v != 1.0)
        || !spec.attribute_fields.is_empty()
        || !spec.unknown_fields.is_empty()
    {
        return None;
    }
    if matches!(spec.modifier_code, 56..=58) {
        if spec.target_code != 5 || spec.operation_code != 3 {
            return None;
        }
        return match spec.modifier_code {
            56 => Some(&["officer_attack"]),
            57 => Some(&["officer_defense"]),
            58 => Some(&["officer_health"]),
            _ => None,
        };
    }
    if spec.target_code != 1 {
        return None;
    }
    match (spec.modifier_code, spec.operation_code) {
        (2, 3) => Some(&["weapon_damage"]),
        (60, 3) => Some(&["shield_hp"]),
        (61, 3) => Some(&["hull_hp"]),
        (73, 3) => Some(&["armor", "shield_deflection", "dodge"]),
        (74, 3) => Some(&["accuracy", "armor_piercing", "shield_piercing"]),
        (707, 1) => Some(&["isolytic_damage"]),
        (808, 1) => Some(&["isolytic_defense"]),
        _ => None,
    }
}

fn report_row(level: u32, bid: u64, spec: Option<&BuffSpec>, ops: Option<u32>) -> RewardReportRow {
    let mut row = RewardReportRow {
        unlock_level: level,
        buff_id: bid,
        source_group: spec.map(|s| s.source_group),
        status: RewardStatus::MissingDefinition,
        raw_value: None,
        stats: Vec::new(),
        condition_codes: spec.map_or_else(Vec::new, |s| s.condition_codes.clone()),
        reason: "Reward reference has no definition in reviewed groups116/68.".into(),
    };
    let Some(spec) = spec else { return row };
    if spec.source_group == 68 {
        row.status = RewardStatus::LinkedBuilding;
        row.reason = "Linked building effect: requires building level and its Syndicate gates; Ops is not a building rank.".into();
    } else if spec.source_group != 116 {
        row.status = RewardStatus::Unsupported;
        row.reason = "Unsupported reward source group.".into();
    } else if matches!(
        spec.modifier_code,
        29 | 40 | 55 | 63 | 66 | 67 | 83 | 105 | 107 | 112 | 131 | 137 | 71001
    ) {
        row.status = RewardStatus::Noncombat;
        row.reason = "Economy, travel, repair, cargo, reputation, XP or loot reward; excluded from combat totals.".into();
    } else if !spec.condition_codes.is_empty() {
        row.status = RewardStatus::Conditional;
        row.raw_value = raw_rank(spec, ops);
        row.reason = "Native predicates require additional context; raw value is diagnostic and excluded from totals.".into();
    } else if let Some(stats) = unconditional_stats(spec) {
        row.stats = stats.iter().map(|s| (*s).to_owned()).collect();
        row.raw_value = raw_rank(spec, ops);
        row.status = if row.raw_value.is_some() {
            RewardStatus::UnconditionalCombat
        } else {
            RewardStatus::RankUnavailable
        };
        row.reason = if row.raw_value.is_some() {
            "Unlocked definition at the requested Ops snapshot; active-profile membership is not checked."
        } else {
            "Provide an Ops level within the native rank array; no rank is guessed or extrapolated."
        }
        .into();
    } else {
        row.status = RewardStatus::Unsupported;
        row.reason = "Unsupported native operation, target, timing, attributes, chance or ranked-value contract.".into();
    }
    row
}

fn build_report(
    catalog: Catalog,
    level: u32,
    ops: Option<u32>,
) -> Result<SyndicateRewardReport, String> {
    if catalog.schema_version != 1
        || catalog.provenance.static_cache_sha256 != REVIEWED_CACHE
        || catalog.coverage.tier_count != catalog.tiers.len()
        || catalog.coverage.resolved_buff_count != catalog.buffs.len()
        || catalog.coverage.unique_buff_count
            != catalog.buffs.len() + catalog.unresolved_buff_ids.len()
    {
        return Err("Invalid reviewed Syndicate catalog provenance or coverage.".into());
    }
    let max_level = catalog.tiers.iter().map(|t| t.level).max().unwrap_or(0);
    if level == 0 || level > max_level {
        return Err(format!(
            "Syndicate level must be within reviewed tiers1..={max_level}."
        ));
    }
    if ops.is_some_and(|o| o < catalog.config.min_player_level) {
        return Err(format!(
            "Native Syndicate requires Ops{} or higher.",
            catalog.config.min_player_level
        ));
    }
    let mut ids = BTreeSet::new();
    let mut selected_reward_references = 0;
    let mut rows = Vec::new();
    let mut totals = BTreeMap::new();
    for tier in catalog.tiers.iter().filter(|t| t.level <= level) {
        for reward in tier.collections.iter().flat_map(|c| &c.rewards) {
            selected_reward_references += 1;
            if !ids.insert(reward.buff_id) {
                continue;
            }
            let row = report_row(
                tier.level,
                reward.buff_id,
                catalog.buffs.get(&reward.buff_id),
                ops,
            );
            if row.status == RewardStatus::UnconditionalCombat {
                if let Some(value) = row.raw_value {
                    for stat in &row.stats {
                        *totals.entry(stat.clone()).or_insert(0.0) += value;
                    }
                }
            }
            rows.push(row);
        }
    }
    Ok(SyndicateRewardReport {
        syndicate_level: level,
        operations_level: ops,
        static_cache_sha256: catalog.provenance.static_cache_sha256,
        complete_source_coverage: catalog.coverage,
        selected_reward_references,
        unique_selected_buff_ids: ids.len(),
        unconditional_combat_bonuses: totals,
        rows,
        limitation: "Offline native reward definitions, not an active-profile combat total. Runtime already compiles active BuffIDs once; do not add this report over those bonuses. Conditional and linked-building effects are excluded. Frozen client configuration does not prove current server behavior.".into(),
    })
}

/// Report source-backed personal Syndicate definitions without reading a player profile.
pub fn native_reward_report(
    syndicate_level: u32,
    operations_level: Option<u32>,
) -> Result<SyndicateRewardReport, String> {
    let catalog =
        serde_json::from_str(CATALOG).map_err(|e| format!("Native Syndicate catalog: {e}"))?;
    build_report(catalog, syndicate_level, operations_level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_catalog_retains_complete_reward_coverage() {
        let report = native_reward_report(65, Some(61)).unwrap();
        assert_eq!(report.complete_source_coverage.tier_count, 65);
        assert_eq!(report.complete_source_coverage.reward_reference_count, 229);
        assert_eq!(report.unique_selected_buff_ids, 229);
        assert_eq!(
            report
                .rows
                .iter()
                .filter(|r| r.status == RewardStatus::MissingDefinition)
                .count(),
            1
        );
    }

    #[test]
    fn level45_report_uses_ops_snapshots_and_separates_building_ranks() {
        let report = native_reward_report(45, Some(61)).unwrap();
        for (stat, expected) in [
            ("weapon_damage", 2.6),
            ("hull_hp", 9.9),
            ("shield_hp", 9.9),
            ("accuracy", 6.4),
            ("armor_piercing", 6.4),
            ("shield_piercing", 6.4),
            ("armor", 6.5),
            ("shield_deflection", 6.5),
            ("dodge", 6.5),
            ("officer_defense", 7.65),
        ] {
            assert!(
                (report.unconditional_combat_bonuses[stat] - expected).abs() < 1e-10,
                "{stat}"
            );
        }
        assert!(report
            .rows
            .iter()
            .filter(|r| r.source_group == Some(68))
            .all(|r| r.status == RewardStatus::LinkedBuilding && r.raw_value.is_none()));
        assert!(report
            .rows
            .iter()
            .any(|r| r.buff_id == 831877216 && r.status == RewardStatus::Conditional));
    }

    #[test]
    fn unknown_or_out_of_array_ops_never_reuses_another_rank() {
        for ops in [None, Some(100)] {
            let report = native_reward_report(45, ops).unwrap();
            assert!(report.unconditional_combat_bonuses.is_empty());
            assert!(report
                .rows
                .iter()
                .any(|r| r.status == RewardStatus::RankUnavailable));
        }
        assert!(native_reward_report(0, Some(61)).is_err());
        assert!(native_reward_report(66, Some(61)).is_err());
        assert!(native_reward_report(45, Some(9)).is_err());
    }

    #[test]
    fn duplicate_reference_is_not_an_extra_buff_contribution() {
        let mut catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
        catalog.tiers[3].collections[0].rewards.push(Reward {
            buff_id: 1333781883,
        });
        let report = build_report(catalog, 4, Some(61)).unwrap();
        assert!((report.unconditional_combat_bonuses["accuracy"] - 0.4).abs() < 1e-12);
        assert!(report.selected_reward_references > report.unique_selected_buff_ids);
    }
}
