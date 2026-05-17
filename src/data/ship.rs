//! Ship data: normalized combat stats for player ships (from STFCcommunity or manual).
//! Used to build AttackerStats + Combatant when resolving by name/id.
//!
//! **Hull abilities:** Upstream `ability` JSON is normalized into [`ShipAbility`] on
//! [`ExtendedShipRecord`] / [`ShipRecord`], then converted to combat seats in
//! [`crate::data::ship_ability_resolve`] and merged in
//! [`crate::optimizer::monte_carlo::scenario`]. See `docs/DESIGN.md` §3.6 (Ship hull abilities).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{AttackerStats, DefenderStats, ShipType, WeaponStats};

/// Per-weapon attack (and optional base shots) for sub-round resolution. When present on ShipRecord, used to build Combatant.weapons.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeaponRecord {
    pub attack: f64,
    /// Base shots per weapon per round. When absent, 1. Effective shots = round_half_even(shots * (1 + B_shots)).
    #[serde(default)]
    pub shots: Option<u32>,
    /// Damage-through pierce override for this weapon (same units as [`crate::combat::Combatant::pierce`]). Rare; usually unset.
    #[serde(default)]
    pub pierce: Option<f64>,
    /// Raw armor piercing from upstream (`penetration` / `armor_pierce`). When any of armor/shield/accuracy per row is set, scenario code may derive per-weapon pierce from mitigation math.
    #[serde(default)]
    pub armor_piercing: Option<f64>,
    /// Raw shield piercing from upstream (`modulation` / `shield_pierce`).
    #[serde(default)]
    pub shield_piercing: Option<f64>,
    /// Raw accuracy for this weapon (upstream component). Merged with profile / static accuracy bonuses when resolving pierce-through.
    #[serde(default)]
    pub accuracy: Option<f64>,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub proc_chance: Option<f64>,
    #[serde(default)]
    pub proc_multiplier: Option<f64>,
}

/// Normalized ship hull ability (from data.stfc.space ability array). Trigger and effect are resolved when building crew.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipAbility {
    /// Unique id for this ability (e.g. upstream numeric id as string, or semantic id like "pierce_on_hit").
    pub id: String,
    /// Timing window: "combat_begin", "round_start", "attack_phase", "defense_phase", "round_end", "receive_damage", "shield_break", "kill", "hull_breach", "combat_end".
    pub timing: String,
    /// Effect type: see `ship_ability_resolve` and `data/upstream/data-stfc-space/ship_ability_catalog.json`.
    pub effect_type: String,
    /// Effect magnitude (e.g. 0.1 for +10% pierce). Interpretation depends on effect_type.
    pub value: f64,
    /// Optional duration from catalog (e.g. hostile crit reduction rounds for U.S.S. Crozier).
    #[serde(default)]
    pub duration_rounds: Option<u32>,
    /// Gated on round-start Morale proc (see [AbilityCondition::MoraleActive]).
    #[serde(default)]
    pub condition_morale: bool,
    /// Gated on defender Burning state.
    #[serde(default)]
    pub condition_defender_burning: bool,
    /// Gated on defender Hull Breach state.
    #[serde(default)]
    pub condition_defender_hull_breach: bool,
    /// Gated on defending hostile faction slug (`klingon`, `romulan`, …); matches [`crate::combat::OpponentFactionTag`] serde names.
    #[serde(default)]
    pub condition_opponent_faction: Option<String>,
    /// Gated on defending ship hull class (`battleship`, `explorer`, `interceptor`, …); matches [`crate::combat::ShipType`] serde names.
    #[serde(default)]
    pub condition_opponent_ship_class: Option<String>,
    /// Gated on defending hostile tags (AND: every listed slug must map to a set bit on the defender mask). See [`crate::combat::hostile_tags`].
    #[serde(default)]
    pub condition_opponent_hostile_tags: Option<Vec<String>>,
    /// When set, the hull ability’s combat effects only apply for combat rounds `1..=round_cap` (inclusive).
    /// Combat-begin accuracy folded into static attacker stats ignores this field (see [`crate::data::ship_ability_resolve::sum_combat_begin_accuracy_from_ship_abilities`]).
    #[serde(default)]
    pub round_cap: Option<u32>,
    /// When present, index `level - 1` selects [`ShipAbility::value`] at [`ExtendedShipRecord::to_ship_record`] time
    /// (upstream `ability.values[]` curves, e.g. Galaxy class per-level weapon damage bonus). Omitted after resolution onto [`ShipRecord`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_scaled_values: Option<Vec<f64>>,
}

/// Pick a per-level scalar from an upstream `values[]` curve (1-based `level`).
/// Trailing zero padding in the curve is skipped in favor of the last positive entry.
pub fn ship_ability_value_for_level(curve: &[f64], level: u32) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    let i = level.saturating_sub(1) as usize;
    if i < curve.len() {
        let v = curve[i];
        if v > 0.0 || i == 0 {
            return v;
        }
    }
    curve
        .iter()
        .copied()
        .rev()
        .find(|v| *v > 0.0)
        .unwrap_or(0.0)
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
    /// Optional owner faction slug (`federation`, `klingon`, `romulan`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    /// Aggregated armor piercing (from weapon components).
    pub armor_piercing: f64,
    /// Aggregated shield piercing (from weapon components).
    pub shield_piercing: f64,
    /// Aggregated accuracy (from weapon components).
    pub accuracy: f64,
    /// Raw armor stat (defender side). Symmetric to [`crate::data::HostileRecord::armor`]; feeds
    /// [`Self::to_defender_stats`] for hostile→player counter-fire mitigation. Default 0 when upstream
    /// data does not provide it; in that case counter-fire pierce/dodge collapses to a constant.
    #[serde(default)]
    pub armor: f64,
    /// Raw shield deflection stat (defender side). See [`Self::armor`].
    #[serde(default)]
    pub shield_deflection: f64,
    /// Raw dodge stat (defender side). See [`Self::armor`].
    #[serde(default)]
    pub dodge: f64,
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
    /// Raw armor stat for the defender side (hostile→player counter-fire). Default 0 until upstream
    /// ship data populates it; see [`ShipRecord::armor`].
    #[serde(default)]
    pub armor: f64,
    #[serde(default)]
    pub shield_deflection: f64,
    #[serde(default)]
    pub dodge: f64,
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

/// Single breakpoint in the per-ship officer-stat → bonus mapping.
/// When the cumulative officer rating (A/D/H sum across crewed officers) reaches `value`,
/// the bonus jumps to `bonus` (step function, not interpolated). See
/// `docs/OFFICER_STAT_FORMULA.md` §2a.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfficerBonusBreakpoint {
    pub value: f64,
    pub bonus: f64,
}

/// Per-ship breakpoint tables that map officer-stat ratings to bonus percentages.
/// Mirrors `officer_bonus` from upstream data.stfc.space ship JSON.
/// Consumed in Phase 2/3 to compute attack/defense/health bonuses from per-side rating sums.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct OfficerBonusTable {
    #[serde(default)]
    pub attack: Vec<OfficerBonusBreakpoint>,
    #[serde(default)]
    pub defense: Vec<OfficerBonusBreakpoint>,
    #[serde(default)]
    pub health: Vec<OfficerBonusBreakpoint>,
}

impl OfficerBonusTable {
    /// True when every channel is empty (no breakpoints).
    pub fn is_empty(&self) -> bool {
        self.attack.is_empty() && self.defense.is_empty() && self.health.is_empty()
    }
}

/// Below-decks officer slot unlock from upstream `crew_slots` (data.stfc.space ship JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrewSlotUnlock {
    /// Upstream 1-based slot ordinal as string (informational).
    #[serde(default)]
    pub slots: Option<String>,
    /// Ship level at which this below-decks slot becomes available.
    pub unlock_level: u32,
}

/// Count below-decks slots unlocked at `ship_level` using normalized `crew_slots` (empty → 0).
pub fn below_decks_slot_count_at_level(crew_slots: &[CrewSlotUnlock], ship_level: u32) -> usize {
    crew_slots
        .iter()
        .filter(|s| s.unlock_level <= ship_level)
        .count()
}

/// Extended ship record: one file per ship with all tiers and level bonuses. Resolved at request time to ShipRecord.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedShipRecord {
    pub id: String,
    pub ship_name: String,
    pub ship_class: String,
    /// Optional owner faction slug (`federation`, `klingon`, `romulan`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    pub tiers: Vec<TierStats>,
    pub levels: Vec<LevelBonus>,
    /// Below-decks slot unlock schedule from upstream `crew_slots` (optional; when empty, UI/API use tier/level heuristics).
    #[serde(default)]
    pub crew_slots: Vec<CrewSlotUnlock>,
    /// Ship hull abilities from data.stfc.space ability array. Applied to all tiers.
    #[serde(default)]
    pub abilities: Option<Vec<ShipAbility>>,
    /// Per-ship officer-stat breakpoint tables from upstream `officer_bonus`. Empty when upstream
    /// data is missing (legacy ships); consumers must tolerate that. See `docs/OFFICER_STAT_FORMULA.md`.
    #[serde(default, skip_serializing_if = "OfficerBonusTable::is_empty")]
    pub officer_bonus: OfficerBonusTable,
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
        let abilities = self.abilities.as_ref().map(|rows| {
            rows.iter()
                .map(|a| {
                    let mut a = a.clone();
                    if let Some(ref curve) = a.level_scaled_values {
                        a.value = ship_ability_value_for_level(curve, level_num);
                        a.level_scaled_values = None;
                    }
                    a
                })
                .collect()
        });
        Some(ShipRecord {
            id: self.id.clone(),
            ship_name: self.ship_name.clone(),
            ship_class: self.ship_class.clone(),
            faction: self.faction.clone(),
            armor_piercing: t.armor_piercing,
            shield_piercing: t.shield_piercing,
            accuracy: t.accuracy,
            armor: t.armor,
            shield_deflection: t.shield_deflection,
            dodge: t.dodge,
            attack: t.attack,
            crit_chance: t.crit_chance,
            crit_damage: t.crit_damage,
            hull_health: t.hull_health + health_bonus,
            shield_health: t.shield_health + shield_bonus,
            shield_mitigation: t.shield_mitigation,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: t.weapons.clone(),
            abilities,
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

    /// Raw defender stats for the hostile→player counter-fire path. Symmetric to
    /// [`crate::data::HostileRecord::to_defender_stats`]. Returns zeros until upstream ship data
    /// populates `armor` / `shield_deflection` / `dodge`; in that case hostile counter-fire
    /// pierce-through and dodge collapse to a constant (the historical placeholder).
    pub fn to_defender_stats(&self) -> DefenderStats {
        DefenderStats {
            armor: self.armor,
            shield_deflection: self.shield_deflection,
            dodge: self.dodge,
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
                        pierce: r.pierce,
                        crit_chance: r.crit_chance,
                        crit_multiplier: r.crit_multiplier,
                        proc_chance: r.proc_chance,
                        proc_multiplier: r.proc_multiplier,
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![WeaponStats {
                    attack: self.attack,
                    ..Default::default()
                }]
            })
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

pub const DEFAULT_SHIP_ID_REGISTRY_PATH: &str =
    "data/upstream/data-stfc-space/ship_id_registry.json";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_ship_json_abilities_round_trip_to_ship_record() {
        let json = r#"{
            "id": "fixture_ship_abilities",
            "ship_name": "Fixture",
            "ship_class": "battleship",
            "tiers": [{
                "tier": 1,
                "armor_piercing": 1.0,
                "shield_piercing": 1.0,
                "accuracy": 1.0,
                "attack": 100.0,
                "crit_chance": 0.0,
                "crit_damage": 1.0,
                "hull_health": 1000.0,
                "shield_health": 0.0
            }],
            "levels": [{ "level": 1, "shield": 0.0, "health": 0.0 }],
            "abilities": [{
                "id": "42",
                "timing": "round_start",
                "effect_type": "pierce_bonus",
                "value": 0.05
            }]
        }"#;

        let extended: ExtendedShipRecord = serde_json::from_str(json).expect("parse extended ship");
        let rec = extended
            .to_ship_record(Some(1), Some(1))
            .expect("tier 1 level 1");
        let abilities = rec.abilities.expect("abilities present");
        assert_eq!(abilities.len(), 1);
        assert_eq!(abilities[0].effect_type, "pierce_bonus");
    }

    #[test]
    fn level_scaled_ability_resolves_value_from_curve() {
        let json = r#"{
            "id": "fixture_level_curve",
            "ship_name": "Fixture",
            "ship_class": "battleship",
            "tiers": [{
                "tier": 1,
                "armor_piercing": 1.0,
                "shield_piercing": 1.0,
                "accuracy": 1.0,
                "attack": 100.0,
                "crit_chance": 0.0,
                "crit_damage": 1.0,
                "hull_health": 1000.0,
                "shield_health": 0.0
            }],
            "levels": [
                { "level": 1, "shield": 0.0, "health": 0.0 },
                { "level": 7, "shield": 0.0, "health": 0.0 }
            ],
            "abilities": [{
                "id": "99",
                "timing": "round_start",
                "effect_type": "attack_multiplier",
                "value": 0.0,
                "level_scaled_values": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]
            }]
        }"#;

        let extended: ExtendedShipRecord = serde_json::from_str(json).expect("parse");
        let rec = extended
            .to_ship_record(Some(1), Some(7))
            .expect("tier 1 level 7");
        let a = &rec.abilities.as_ref().expect("abilities")[0];
        assert!(
            (a.value - 0.7).abs() < 1e-9,
            "level 7 → index 6 = 0.7, got {}",
            a.value
        );
        assert!(a.level_scaled_values.is_none());
    }

    #[test]
    fn below_decks_slot_count_matches_unlock_levels() {
        let slots = [
            CrewSlotUnlock {
                slots: Some("1".into()),
                unlock_level: 5,
            },
            CrewSlotUnlock {
                slots: Some("2".into()),
                unlock_level: 10,
            },
            CrewSlotUnlock {
                slots: Some("3".into()),
                unlock_level: 20,
            },
            CrewSlotUnlock {
                slots: Some("4".into()),
                unlock_level: 30,
            },
        ];
        assert_eq!(below_decks_slot_count_at_level(&slots, 4), 0);
        assert_eq!(below_decks_slot_count_at_level(&slots, 5), 1);
        assert_eq!(below_decks_slot_count_at_level(&slots, 30), 4);
    }
}
