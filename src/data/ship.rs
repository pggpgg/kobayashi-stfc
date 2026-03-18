//! Ship data: normalized combat stats for player ships (from STFCcommunity or manual).
//! Used to build AttackerStats + Combatant when resolving by name/id.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{AttackerStats, ShipType, WeaponStats};

/// Per-weapon attack (and optional base shots) for sub-round resolution. When present on ShipRecord, used to build Combatant.weapons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponRecord {
    pub attack: f64,
    /// Base shots per weapon per round. When absent, 1. Effective shots = round_half_even(shots * (1 + B_shots)).
    #[serde(default)]
    pub shots: Option<u32>,
}

/// Normalized ship hull ability (from data.stfc.space ability array). Trigger and effect are resolved when building crew.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipAbility {
    /// Unique id for this ability (e.g. upstream numeric id as string, or semantic id like "pierce_on_hit").
    pub id: String,
    /// Timing window: "combat_begin", "round_start", "attack_phase", "defense_phase", "round_end", "receive_damage", "shield_break", "kill", "hull_breach", "combat_end".
    pub timing: String,
    /// Effect type: "pierce_bonus", "attack_multiplier", "accuracy_bonus", etc. Value is in [value].
    pub effect_type: String,
    /// Effect magnitude (e.g. 0.1 for +10% pierce). Interpretation depends on effect_type.
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: String,
}

/// Normalized ship record (KOBAYASHI schema). Written by normalizer, loaded at runtime.
/// Stats are for a chosen tier/level (e.g. tier 1, level 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipRecord {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
    /// Aggregated armor piercing (from weapon components).
    pub armor_piercing: f64,
    /// Aggregated shield piercing (from weapon components).
    pub shield_piercing: f64,
    /// Aggregated accuracy (from weapon components).
    pub accuracy: f64,
    /// Representative attack/damage (e.g. damage_per_round from primary weapon).
    pub attack: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub hull_health: f64,
    pub shield_health: f64,
    /// Fraction of incoming damage to shield (rest to hull). Base 0.8 for most ships; Sarcophagus is 0.2 (STFC Toolbox).
    #[serde(default)]
    pub shield_mitigation: Option<f64>,
    /// Apex Shred: reduces defender's effective Apex Barrier. Stored as decimal (1.0 = 100%).
    #[serde(default)]
    pub apex_shred: f64,
    /// Isolytic damage bonus (decimal). Used in combat isolytic_damage().
    #[serde(default)]
    pub isolytic_damage: f64,
    /// Per-weapon attack values. When present, used to build Combatant.weapons for sub-round resolution.
    #[serde(default)]
    pub weapons: Option<Vec<WeaponRecord>>,
    /// Ship hull abilities (e.g. when hit, increase armor piercing). Evaluated per round in the combat engine.
    #[serde(default)]
    pub abilities: Option<Vec<ShipAbility>>,
}

/// Per-tier combat stats (from data-stfc.space or extended normalizer). Used to resolve ShipRecord for a given tier/level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierStats {
    pub tier: u32,
    pub armor_piercing: f64,
    pub shield_piercing: f64,
    pub accuracy: f64,
    pub attack: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub hull_health: f64,
    pub shield_health: f64,
    #[serde(default)]
    pub shield_mitigation: Option<f64>,
    #[serde(default)]
    pub weapons: Option<Vec<WeaponRecord>>,
}

/// Per-level bonus to shield and hull (additive to tier base). Level 1 is typically 0,0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelBonus {
    pub level: u32,
    pub shield: f64,
    pub health: f64,
}

/// Extended ship record: one file per ship with all tiers and level bonuses. Resolved at request time to ShipRecord.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedShipRecord {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
    pub tiers: Vec<TierStats>,
    pub levels: Vec<LevelBonus>,
    /// Ship hull abilities from data.stfc.space ability array. Applied to all tiers.
    #[serde(default)]
    pub abilities: Option<Vec<ShipAbility>>,
}

impl ExtendedShipRecord {
    /// Resolve to a flat ShipRecord for the given tier and level (1-based). Level bonuses are added to tier base.
    /// Uses tier 1 and level 1 if out of range.
    pub fn to_ship_record(&self, tier: Option<u32>, level: Option<u32>) -> Option<ShipRecord> {
        let tier_num = tier.unwrap_or(1).max(1);
        let level_num = level.unwrap_or(1).max(1);
        let t = self.tiers.iter().find(|x| x.tier == tier_num)?;
        let shield_bonus = self
            .levels
            .iter()
            .find(|x| x.level == level_num)
            .map(|x| x.shield)
            .unwrap_or(0.0);
        let health_bonus = self
            .levels
            .iter()
            .find(|x| x.level == level_num)
            .map(|x| x.health)
            .unwrap_or(0.0);
        Some(ShipRecord {
            id: self.id.clone(),
            ship_name: self.ship_name.clone(),
            ship_class: self.ship_class.clone(),
            armor_piercing: t.armor_piercing,
            shield_piercing: t.shield_piercing,
            accuracy: t.accuracy,
            attack: t.attack,
            crit_chance: t.crit_chance,
            crit_damage: t.crit_damage,
            hull_health: t.hull_health + health_bonus,
            shield_health: t.shield_health + shield_bonus,
            shield_mitigation: t.shield_mitigation,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: t.weapons.clone(),
            abilities: self.abilities.clone(),
        })
    }
}

/// Index of all ships for name resolution. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub ships: Vec<ShipIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIndexEntry {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
}

impl ShipRecord {
    pub fn to_attacker_stats(&self) -> AttackerStats {
        AttackerStats {
            armor_piercing: self.armor_piercing,
            shield_piercing: self.shield_piercing,
            accuracy: self.accuracy,
        }
    }

    pub fn ship_type(&self) -> ShipType {
        crate::data::hostile::ship_class_to_type(&self.ship_class)
    }

    /// Per-weapon stats for sub-round resolution. If weapons list is present, returns it; otherwise one weapon with scalar attack.
    pub fn to_weapons(&self) -> Vec<WeaponStats> {
        self.weapons
            .as_ref()
            .map(|w| {
                w.iter()
                    .map(|r| WeaponStats {
                        attack: r.attack,
                        shots: r.shots,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![WeaponStats {
                attack: self.attack,
                shots: None,
            }])
    }
}

pub const DEFAULT_SHIPS_INDEX_PATH: &str = "data/ships/index.json";

/// Load ship index from data/ships/index.json. Returns None if file missing.
pub fn load_ship_index(path: &str) -> Option<ShipIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load a single ship record by id from data/ships/<id>.json.
pub fn load_ship_record(data_dir: &Path, id: &str) -> Option<ShipRecord> {
    let path = data_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub const DEFAULT_SHIPS_EXTENDED_DIR: &str = "data/ships_extended";

/// Load extended ship record (tiers + levels) from data/ships_extended/<id>.json.
pub fn load_extended_ship_record(extended_dir: &Path, id: &str) -> Option<ExtendedShipRecord> {
    let path = extended_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Ship id registry: numeric id (data.stfc.space) -> canonical id, ship_name, ship_class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIdRegistry {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub ships: Vec<ShipIdRegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipIdRegistryEntry {
    pub numeric_id: u64,
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
}

pub const DEFAULT_SHIP_ID_REGISTRY_PATH: &str = "data/upstream/data-stfc-space/ship_id_registry.json";

/// Load ship id registry from data/upstream/data-stfc-space/ship_id_registry.json.
pub fn load_ship_id_registry(path: &str) -> Option<ShipIdRegistry> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Index of ships in data/ships_extended for name resolution (by id or ship_name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedShipIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub ships: Vec<ExtendedShipIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedShipIndexEntry {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
}

/// Load extended ship index from data/ships_extended/index.json.
pub fn load_extended_ship_index(extended_dir: &Path) -> Option<ExtendedShipIndex> {
    let path = extended_dir.join("index.json");
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
