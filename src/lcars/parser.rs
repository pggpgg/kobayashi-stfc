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
        match self {
            LcarsDuration::Permanent(_) => true,
            _ => false,
        }
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
}

impl LcarsScaling {
    /// Value at given tier (1-based rank). Falls back to base or 0.0.
    pub fn value_at_rank(&self, rank: Option<u8>) -> f64 {
        let base = self.base.unwrap_or(0.0);
        let per = self.per_rank.unwrap_or(0.0);
        let max = self.max_rank.unwrap_or(5).max(1) as u8;
        let r = rank.map(|r| r.min(max)).unwrap_or(1);
        let index = (r.saturating_sub(1)).min(max.saturating_sub(1));
        base + per * (index as f64)
    }

    pub fn chance_at_rank(&self, rank: Option<u8>) -> f64 {
        let base = self.base_chance.unwrap_or(self.base.unwrap_or(0.0));
        let per = self.per_rank.unwrap_or(0.0);
        let max = self.max_rank.unwrap_or(5).max(1) as u8;
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
    #[serde(default)]
    pub conditions: Option<Vec<LcarsCondition>>,
}

/// Load a single `.lcars.yaml` file.
pub fn load_lcars_file(path: impl AsRef<Path>) -> Result<LcarsFile, Box<dyn std::error::Error + Send + Sync>> {
    let raw = fs::read_to_string(path)?;
    let parsed: LcarsFile = serde_yaml::from_str(&raw)?;
    Ok(parsed)
}

/// Load all `*.lcars.yaml` and `*.lcars.yml` files from a directory and merge officers.
/// Only filenames matching these patterns are loaded; other YAML files in the directory are ignored.
pub fn load_lcars_dir(dir: impl AsRef<Path>) -> Result<Vec<LcarsOfficer>, Box<dyn std::error::Error + Send + Sync>> {
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
            let is_lcars = name.map_or(false, |n| n.ends_with(".lcars.yaml") || n.ends_with(".lcars.yml"));
            if is_lcars {
                if let Ok(file) = load_lcars_file(&path) {
                    officers.extend(file.officers);
                }
            }
        }
    }
    Ok(officers)
}
