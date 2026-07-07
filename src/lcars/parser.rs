//! Parses LCARS YAML files into typed structures.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Root structure of an LCARS YAML file (e.g. one file per faction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsFile {
    pub officers: Vec<LcarsOfficer>,
}

/// Single officer definition with up to three ability blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsOfficer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub faction: Option<String>,
    #[serde(default)]
    pub rarity: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub captain_ability: Option<LcarsAbility>,
    #[serde(default)]
    pub bridge_ability: Option<LcarsAbility>,
    #[serde(default)]
    pub below_decks_ability: Option<LcarsAbility>,
    /// Officer's own per-level Attack/Defense/Health stats (sourced from upstream
    /// `data/upstream/data-stfc-space/officers/{id}.json`). Used to resolve officer-stat scaling
    /// (see [`LcarsScaling::officer_stat`]). Empty when unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<LcarsLevelStats>,
    /// Max level reachable at each rank (index 0 = rank 1). Used to pick a default officer level
    /// for stat lookups when the caller has not specified one. Empty when unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub max_level_by_rank: Vec<u32>,
}

/// One row of the officer's own per-level stat table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsLevelStats {
    pub level: u32,
    pub attack: f64,
    pub defense: f64,
    pub health: f64,
}

impl LcarsOfficer {
    /// Officer level to use for stat lookups: per-officer override → max level for the resolved
    /// rank → max level overall → 1.
    pub fn resolve_level(&self, override_level: Option<u32>, rank: Option<u8>) -> Option<u32> {
        if let Some(l) = override_level {
            return Some(l);
        }
        if let Some(r) = rank {
            let idx = (r as usize).saturating_sub(1);
            if let Some(&l) = self.max_level_by_rank.get(idx) {
                if l > 0 {
                    return Some(l);
                }
            }
        }
        let last = self
            .max_level_by_rank
            .iter()
            .rev()
            .find(|&&l| l > 0)
            .copied();
        last.or_else(|| self.stats.iter().map(|s| s.level).max())
    }

    /// Per-level stat row for the chosen level. Falls back to the closest level ≤ requested
    /// (typical when the upstream curve doesn't include every level), else the highest available.
    pub fn stats_at_level(&self, level: u32) -> Option<&LcarsLevelStats> {
        if self.stats.is_empty() {
            return None;
        }
        let mut best: Option<&LcarsLevelStats> = None;
        for s in &self.stats {
            if s.level <= level {
                best = Some(match best {
                    Some(prev) if prev.level >= s.level => prev,
                    _ => s,
                });
            }
        }
        best.or_else(|| self.stats.iter().max_by_key(|s| s.level))
    }
}

impl LcarsLevelStats {
    pub fn value_for(&self, stat: crate::data::combat_effect_spec::OfficerStat) -> f64 {
        use crate::data::combat_effect_spec::OfficerStat;
        match stat {
            OfficerStat::Attack => self.attack,
            OfficerStat::Defense => self.defense,
            OfficerStat::Health => self.health,
        }
    }
}

/// One ability block (captain, bridge, or below decks) with a name and effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsAbility {
    pub name: String,
    #[serde(default)]
    pub effects: Vec<LcarsEffect>,
}

/// Single effect within an ability. Unknown `type` values are preserved and
/// skipped at resolve time (graceful degradation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsEffect {
    #[serde(rename = "type")]
    pub effect_type: String,
    #[serde(default)]
    pub stat: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub duration: Option<LcarsDuration>,
    #[serde(default)]
    pub scaling: Option<LcarsScaling>,
    #[serde(default)]
    pub condition: Option<LcarsCondition>,
    // extra_attack-specific
    #[serde(default)]
    pub chance: Option<f64>,
    #[serde(default)]
    pub multiplier: Option<f64>,
    // tag (non-combat)
    #[serde(default)]
    pub tag: Option<String>,
    // accumulate: effects that grow over time
    #[serde(default)]
    pub accumulate: Option<LcarsAccumulate>,
    // decay: effects that decrease over time
    #[serde(default)]
    pub decay: Option<LcarsDecay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsAccumulate {
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub ceiling: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsDecay {
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub floor: Option<f64>,
}

/// Duration of an effect. In YAML: `permanent` (string) or `rounds: N` (map).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LcarsDuration {
    Permanent(String),
    Rounds { rounds: u32 },
    Stacks { stacks: u32 },
}

impl LcarsDuration {
    pub fn is_permanent(&self) -> bool {
        matches!(self, LcarsDuration::Permanent(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsScaling {
    #[serde(default)]
    pub base: Option<f64>,
    #[serde(default)]
    pub per_rank: Option<f64>,
    #[serde(default)]
    pub max_rank: Option<u8>,
    #[serde(default)]
    pub base_chance: Option<f64>,
    /// Discrete value per rank (index 0 = rank 1). When non-empty, [value_at_rank] uses this
    /// instead of `base` + `per_rank`.
    #[serde(default)]
    pub values: Option<Vec<f64>>,
    /// Discrete proc chance per rank (index 0 = rank 1). When non-empty, [chance_at_rank] uses
    /// this instead of linear `base_chance`/`base` + `per_rank`.
    #[serde(default)]
    pub chance_values: Option<Vec<f64>>,
    /// When set, [`Self::values`] (or `base + per_rank`) are interpreted as percentage coefficients
    /// to multiply by the officer's own stat (Attack / Defense / Health). E.g. `officer_stat:
    /// health` with `values: [15, 15, 25]` means "+15% / +15% / +25% of officer health". Resolved
    /// at LCARS-spec compile time when officer per-level stats are available; otherwise the rank
    /// value passes through unchanged (no-op fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub officer_stat: Option<crate::data::combat_effect_spec::OfficerStat>,
}

impl LcarsScaling {
    fn linear_max_rank(&self) -> u8 {
        self.max_rank.unwrap_or(5).max(1)
    }

    fn table_clamped_index(rank: Option<u8>, max: u8) -> usize {
        let r = rank.map(|r| r.min(max)).unwrap_or(1);
        (r.saturating_sub(1)).min(max.saturating_sub(1)) as usize
    }

    /// Value at given tier (1-based rank). Prefers [Self::values] when set; else `base` + `(rank-1)*per_rank`.
    pub fn value_at_rank(&self, rank: Option<u8>) -> f64 {
        if let Some(ref table) = self.values {
            if !table.is_empty() {
                let n = table.len().min(u8::MAX as usize) as u8;
                let max = self.max_rank.unwrap_or(n).max(1).min(n);
                let idx = Self::table_clamped_index(rank, max);
                return table[idx];
            }
        }
        let base = self.base.unwrap_or(0.0);
        let per = self.per_rank.unwrap_or(0.0);
        let max = self.linear_max_rank();
        let r = rank.map(|r| r.min(max)).unwrap_or(1);
        let index = (r.saturating_sub(1)).min(max.saturating_sub(1));
        base + per * (index as f64)
    }

    pub fn chance_at_rank(&self, rank: Option<u8>) -> f64 {
        if let Some(ref table) = self.chance_values {
            if !table.is_empty() {
                let n = table.len().min(u8::MAX as usize) as u8;
                let max = self.max_rank.unwrap_or(n).max(1).min(n);
                let idx = Self::table_clamped_index(rank, max);
                return table[idx];
            }
        }
        let base = self.base_chance.unwrap_or(self.base.unwrap_or(0.0));
        let per = self.per_rank.unwrap_or(0.0);
        let max = self.linear_max_rank();
        let r = rank.map(|r| r.min(max)).unwrap_or(1);
        let index = (r.saturating_sub(1)).min(max.saturating_sub(1));
        base + per * (index as f64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcarsCondition {
    #[serde(rename = "type")]
    pub condition_type: String,
    #[serde(default)]
    pub stat: Option<String>,
    #[serde(default)]
    pub threshold_pct: Option<f64>,
    #[serde(default)]
    pub min: Option<u32>,
    #[serde(default)]
    pub max: Option<u32>,
    #[serde(default)]
    pub faction: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub min_members: Option<u32>,
    #[serde(default)]
    pub tag: Option<String>,
    /// Hull class slug for ship-class conditions (`battleship`, `explorer`, `interceptor`, `survey`, `armada`).
    /// Used with `defender_ship_type_is` / `opponent_ship_class_is` (enemy) or `attacker_ship_type_is` / `self_ship_class_is` (player ship).
    /// Opponent-category conditions (`defender_is_npc_hostile`, `defender_is_player_ship`) and
    /// `attacker_officer_tal_not_on_bridge` use no extra fields.
    #[serde(default)]
    pub ship_type: Option<String>,
    /// Upstream hostile `faction.id` for `defender_hull_faction_id` / canonical `EnemyHullFaction`.
    #[serde(default)]
    pub faction_id: Option<i64>,
    /// Kobayashi `ships_extended` id for `attacker_ship_id_is` / canonical `SelfHull*` tokens.
    #[serde(default)]
    pub ship_id: Option<String>,
    /// Engagement tag slug for `engagement_includes` (e.g. `group_armadas`); same strings as [`crate::combat::EnemyType`] JSON.
    #[serde(default)]
    pub enemy_type: Option<String>,
    /// Weapon damage-type slug (`"kinetic"` / `"energy"`) for `attacker_weapon_scope`
    /// (canonical `ModuleKinetic` / `ModuleEnergy`). Extracted out-of-band at compile time
    /// into [`crate::combat::abilities::Ability::weapon_scope`].
    #[serde(default)]
    pub weapon_scope: Option<String>,
    /// Raw STFC `battle_types` ids (from canonical attributes `battle_types=[...]`).
    /// Used by `combat_battle_type_any`.
    #[serde(default)]
    pub battle_types: Option<Vec<u32>>,
    #[serde(default)]
    pub conditions: Option<Vec<LcarsCondition>>,
}

/// Load a single `.lcars.yaml` file.
pub fn load_lcars_file(
    path: impl AsRef<Path>,
) -> Result<LcarsFile, Box<dyn std::error::Error + Send + Sync>> {
    let raw = fs::read_to_string(path)?;
    let parsed: LcarsFile = serde_yaml::from_str(&raw)?;
    Ok(parsed)
}

/// Load all `*.lcars.yaml` and `*.lcars.yml` files from a directory and merge officers.
/// Only filenames matching these patterns are loaded; other YAML files in the directory are ignored.
pub fn load_lcars_dir(
    dir: impl AsRef<Path>,
) -> Result<Vec<LcarsOfficer>, Box<dyn std::error::Error + Send + Sync>> {
    let mut officers = Vec::new();
    let dir = dir.as_ref();
    if !dir.is_dir() {
        return Ok(officers);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().and_then(|n| n.to_str());
            let is_lcars =
                name.is_some_and(|n| n.ends_with(".lcars.yaml") || n.ends_with(".lcars.yml"));
            if is_lcars {
                // Lenient at runtime (the engine still loads the remaining files) but loud:
                // a silently skipped monolith would mean simulating with zero officers.
                // `validate_lcars_dir` treats the same failure as a hard validation Error.
                match load_lcars_file(&path) {
                    Ok(file) => officers.extend(file.officers),
                    Err(e) => eprintln!(
                        "warning: skipping malformed LCARS file '{}': {e}",
                        path.display()
                    ),
                }
            }
        }
    }
    Ok(officers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::combat_effect_spec::OfficerStat;

    fn sample_stats() -> Vec<LcarsLevelStats> {
        vec![
            LcarsLevelStats {
                level: 1,
                attack: 100.0,
                defense: 50.0,
                health: 50.0,
            },
            LcarsLevelStats {
                level: 5,
                attack: 150.0,
                defense: 80.0,
                health: 80.0,
            },
            LcarsLevelStats {
                level: 10,
                attack: 250.0,
                defense: 130.0,
                health: 130.0,
            },
            LcarsLevelStats {
                level: 30,
                attack: 800.0,
                defense: 400.0,
                health: 400.0,
            },
        ]
    }

    #[test]
    fn lcars_officer_round_trips_stats_and_max_level_by_rank_through_yaml() {
        let officer = LcarsOfficer {
            id: "x".into(),
            name: "X".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: sample_stats(),
            max_level_by_rank: vec![5, 10, 15, 25, 30],
        };
        let s = serde_yaml::to_string(&officer).expect("serialize");
        let back: LcarsOfficer = serde_yaml::from_str(&s).expect("deserialize");
        assert_eq!(back.stats.len(), 4);
        assert_eq!(back.max_level_by_rank, vec![5, 10, 15, 25, 30]);
        assert!((back.stats[3].health - 400.0).abs() < 1e-12);
    }

    #[test]
    fn lcars_officer_yaml_back_compat_when_stats_absent() {
        let yaml = r#"
id: legacy
name: Legacy
captain_ability: null
"#;
        let o: LcarsOfficer = serde_yaml::from_str(yaml).expect("legacy yaml deserialize");
        assert!(o.stats.is_empty());
        assert!(o.max_level_by_rank.is_empty());
    }

    #[test]
    fn lcars_scaling_yaml_back_compat_when_officer_stat_absent() {
        let yaml = r#"
base: 0.1
per_rank: 0.05
max_rank: 5
"#;
        let s: LcarsScaling = serde_yaml::from_str(yaml).expect("legacy yaml deserialize");
        assert!(s.officer_stat.is_none());
        assert_eq!(s.max_rank, Some(5));
    }

    #[test]
    fn lcars_scaling_round_trips_officer_stat_clause() {
        let s = LcarsScaling {
            base: None,
            per_rank: None,
            max_rank: Some(3),
            base_chance: None,
            values: Some(vec![15.0, 15.0, 25.0]),
            chance_values: None,
            officer_stat: Some(OfficerStat::Health),
        };
        let yaml = serde_yaml::to_string(&s).expect("serialize");
        assert!(
            yaml.contains("officer_stat: health"),
            "expected officer_stat: health in:\n{yaml}"
        );
        let back: LcarsScaling = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back.officer_stat, Some(OfficerStat::Health));
    }

    #[test]
    fn resolve_level_prefers_explicit_override() {
        let officer = LcarsOfficer {
            id: "o".into(),
            name: "O".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: sample_stats(),
            max_level_by_rank: vec![5, 10, 15, 25, 30],
        };
        assert_eq!(officer.resolve_level(Some(7), Some(2)), Some(7));
    }

    #[test]
    fn resolve_level_uses_rank_max_when_no_override() {
        let officer = LcarsOfficer {
            id: "o".into(),
            name: "O".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: sample_stats(),
            max_level_by_rank: vec![5, 10, 15, 25, 30],
        };
        assert_eq!(officer.resolve_level(None, Some(2)), Some(10));
        assert_eq!(officer.resolve_level(None, Some(5)), Some(30));
    }

    #[test]
    fn resolve_level_falls_back_to_overall_max_then_stat_table() {
        let officer = LcarsOfficer {
            id: "o".into(),
            name: "O".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: sample_stats(),
            max_level_by_rank: Vec::new(),
        };
        // No rank table → fall through to highest level on the stats curve.
        assert_eq!(officer.resolve_level(None, Some(3)), Some(30));
        assert_eq!(officer.resolve_level(None, None), Some(30));
    }

    #[test]
    fn stats_at_level_picks_closest_below_or_overall_max() {
        let officer = LcarsOfficer {
            id: "o".into(),
            name: "O".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: sample_stats(),
            max_level_by_rank: Vec::new(),
        };
        // Exact match.
        let s = officer.stats_at_level(10).expect("level 10 row");
        assert!((s.attack - 250.0).abs() < 1e-12);
        // Between sampled levels: pick the closest level ≤ requested.
        let s = officer.stats_at_level(7).expect("level ≤ 7");
        assert_eq!(s.level, 5);
        // Above max sampled level: pick the highest available.
        let s = officer.stats_at_level(99).expect("highest");
        assert_eq!(s.level, 30);
    }

    #[test]
    fn level_stats_value_for_returns_correct_stat() {
        let row = LcarsLevelStats {
            level: 30,
            attack: 800.0,
            defense: 400.0,
            health: 350.0,
        };
        assert!((row.value_for(OfficerStat::Attack) - 800.0).abs() < 1e-12);
        assert!((row.value_for(OfficerStat::Defense) - 400.0).abs() < 1e-12);
        assert!((row.value_for(OfficerStat::Health) - 350.0).abs() < 1e-12);
    }

    // ── malformed-input robustness: parsing must Err cleanly, never panic ──

    fn parse(yaml: &str) -> Result<LcarsFile, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn broken_yaml_syntax_returns_err() {
        assert!(parse("officers: [ {id: \"x\", name: ").is_err());
        assert!(parse(":\n  - :::").is_err());
    }

    #[test]
    fn missing_top_level_officers_key_returns_err() {
        assert!(parse("crew: []").is_err());
        assert!(parse("{}").is_err());
    }

    #[test]
    fn wrong_type_in_typed_field_returns_err() {
        // `value` is Option<f64>; a string must be rejected at deserialization.
        let yaml = r#"
officers:
  - id: x
    name: X
    captain_ability:
      name: A
      effects:
        - type: stat_modify
          value: "not a number"
"#;
        assert!(parse(yaml).is_err());
        // `officers` must be a sequence.
        assert!(parse("officers: 17").is_err());
    }

    /// YAML scalar extremes are accepted by serde at parse time (`.inf` → f64::INFINITY,
    /// huge literals → finite f64); rejecting unreasonable values is post-parse validation's
    /// job, not the parser's. Pinned so the boundary of responsibility stays explicit.
    #[test]
    fn extreme_numeric_scalars_are_parse_accepted() {
        let yaml = r#"
officers:
  - id: "  "
    name: X
    captain_ability:
      name: A
      effects:
        - type: stat_modify
          value: .inf
        - type: stat_modify
          value: 1.0e308
"#;
        let file = parse(yaml).expect("scalar extremes parse");
        let effects = &file.officers[0].captain_ability.as_ref().unwrap().effects;
        assert!(effects[0].value.unwrap().is_infinite());
        assert!(effects[1].value.unwrap().is_finite());
        // Whitespace-only id also parse-accepted; validate_lcars_dir flags it as an Error.
        assert_eq!(file.officers[0].id, "  ");
    }

    /// serde_yaml rejects duplicate mapping keys outright — a hand-edited officer with two
    /// `value:` lines fails the whole parse rather than silently taking either value.
    #[test]
    fn duplicate_mapping_keys_return_err() {
        let yaml = r#"
officers:
  - id: x
    name: X
    captain_ability:
      name: A
      effects:
        - type: stat_modify
          value: 0.1
          value: 0.2
"#;
        assert!(parse(yaml).is_err());
    }

    /// Pathologically deep nesting where a scalar is expected must come back as Err (or hit
    /// serde's recursion limit) — never a stack overflow.
    #[test]
    fn deeply_nested_yaml_errs_without_stack_overflow() {
        let mut value = String::from("0.1");
        for _ in 0..200 {
            value = format!("{{a: {value}}}");
        }
        let yaml = format!(
            "officers:\n  - id: x\n    name: X\n    captain_ability:\n      name: A\n      effects:\n        - type: t\n          value: {value}\n"
        );
        assert!(parse(&yaml).is_err());
    }
}
