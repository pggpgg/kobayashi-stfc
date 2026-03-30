//! Hostile data: normalized combat stats for hostiles (from STFCcommunity, data.stfc.space, or manual).
//! Used to build DefenderStats + hull + ShipType when resolving by name/id.
//!
//! **Display names:** `normalize_hostiles_stfc_space` sets `hostile_name` to `Hostile {id}` until a
//! `loca_id` → string map (e.g. `translations-hostiles` from data.stfc.space) is wired into that tool.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::combat::{AttackerStats, DefenderStats, OpponentFactionTag, ShipType, WeaponStats};

#[derive(Debug, Clone)]
pub struct Hostile {
    pub id: String,
}

/// Faction reference from upstream hostile detail (`faction` object).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostileFactionRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub loca_id: Option<u64>,
}

/// Resource drop range from upstream `resources[]` (min/max may be negative in game data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileResourceDrop {
    pub resource_id: u64,
    pub min: i64,
    pub max: i64,
}

/// Normalized hostile record (KOBAYASHI schema). Written by normalizer, loaded at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileRecord {
    pub id: String,
    pub hostile_name: String,
    pub level: u32,
    pub ship_class: String,
    pub armor: f64,
    pub shield_deflection: f64,
    pub dodge: f64,
    pub hull_health: f64,
    pub shield_health: f64,
    /// Fraction of incoming damage to shield (rest to hull). Base 0.8 for most; some hostiles/ships (e.g. Sarcophagus) use 0.2.
    #[serde(default)]
    pub shield_mitigation: Option<f64>,
    /// Apex Barrier: true damage mitigation applied after other mitigation.
    #[serde(default)]
    pub apex_barrier: f64,
    /// Isolytic defense: flat reduction to isolytic damage taken.
    #[serde(default)]
    pub isolytic_defense: f64,
    /// Mitigation floor (e.g. 0.16). When absent, engine uses [`crate::combat::MITIGATION_FLOOR`].
    #[serde(default)]
    pub mitigation_floor: Option<f64>,
    /// Mitigation ceiling (e.g. 0.72). When absent, engine uses [`crate::combat::MITIGATION_CEILING`].
    #[serde(default)]
    pub mitigation_ceiling: Option<f64>,
    /// "Mystery" mitigation factor X: formula becomes 1 - (1-X)*(1-A)*(1-S)*(1-D). Used rarely by game for some hostiles.
    #[serde(default)]
    pub mystery_mitigation_factor: Option<f64>,

    // --- data.stfc.space / modern upstream (all optional for STFCcommunity JSON) ---
    /// Display-name localization key from upstream (`loca_id`).
    #[serde(default)]
    pub loca_id: Option<u64>,
    #[serde(default)]
    pub faction: Option<HostileFactionRef>,
    /// Upstream ship category enum (not hull class); distinct from `ship_class` string.
    #[serde(default)]
    pub upstream_ship_type: u32,
    /// Raw `hull_type` from upstream before mapping to `ship_class`.
    #[serde(default)]
    pub hull_type_raw: u32,
    #[serde(default)]
    pub rarity: u32,
    #[serde(default)]
    pub is_scout: bool,
    #[serde(default)]
    pub is_outpost: bool,
    #[serde(default)]
    pub strength: u64,
    #[serde(default)]
    pub systems: Vec<u64>,
    #[serde(default)]
    pub xp_amount: u32,
    #[serde(default)]
    pub warp: u32,
    #[serde(default)]
    pub warp_with_superhighway: u32,

    /// Upstream composite `stats.health`.
    #[serde(default)]
    pub stat_health: f64,
    /// Upstream composite `stats.defense`.
    #[serde(default)]
    pub stat_defense: f64,
    /// Upstream `stats.attack`.
    #[serde(default)]
    pub stat_attack: f64,
    /// Upstream `stats.dpr`.
    #[serde(default)]
    pub dpr: f64,
    /// Upstream `stats.strength` (aggregated); may mirror top-level `strength` loosely as f64.
    #[serde(default)]
    pub stat_strength: f64,

    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub armor_piercing: f64,
    #[serde(default)]
    pub shield_piercing: f64,
    #[serde(default)]
    pub crit_chance: f64,
    #[serde(default)]
    pub crit_damage: f64,

    /// Full upstream `components` array (warp, weapons, shield, etc.).
    #[serde(default)]
    pub components: Vec<Value>,
    /// Full upstream `ability` array.
    #[serde(default, rename = "ability")]
    pub ability: Vec<Value>,
    #[serde(default)]
    pub resources: Vec<HostileResourceDrop>,
}

/// Index of all hostiles for name/level resolution. Includes data_version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileIndex {
    #[serde(default)]
    pub data_version: Option<String>,
    #[serde(default)]
    pub source_note: Option<String>,
    pub hostiles: Vec<HostileIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostileIndexEntry {
    pub id: String,
    pub hostile_name: String,
    pub level: u32,
    pub ship_class: String,
    #[serde(default)]
    pub rarity: Option<u32>,
    #[serde(default)]
    pub upstream_ship_type: Option<u32>,
    #[serde(default)]
    pub loca_id: Option<u64>,
}

impl HostileRecord {
    pub fn to_defender_stats(&self) -> DefenderStats {
        DefenderStats {
            armor: self.armor,
            shield_deflection: self.shield_deflection,
            dodge: self.dodge,
        }
    }

    /// Piercing + accuracy for mitigation / pierce-through formulas when this hostile is the attacker
    /// (e.g. counter-fire vs the player ship).
    pub fn to_attacker_stats(&self) -> AttackerStats {
        AttackerStats {
            armor_piercing: self.armor_piercing,
            shield_piercing: self.shield_piercing,
            accuracy: self.accuracy,
        }
    }

    /// Per-weapon stats from normalized `components` entries whose `data.tag` is `"Weapon"`.
    /// Matches the mean-of-min/max convention used for player ships in upstream normalization.
    /// Empty when `components` is empty or has no weapon rows (legacy hostiles).
    pub fn weapons_from_components(&self) -> Vec<WeaponStats> {
        let mut out = Vec::new();
        for comp in &self.components {
            let data = comp.as_object().and_then(|o| o.get("data")).unwrap_or(comp);
            if let Some(w) = weapon_stats_from_component_data(data) {
                out.push(w);
            }
        }
        out
    }

    /// Scalar attack when [`Self::weapons_from_components`] is empty: prefer `stat_attack`, then `dpr`.
    pub fn scalar_attack_fallback(&self) -> f64 {
        let a = self.stat_attack;
        if a > 0.0 {
            a
        } else if self.dpr > 0.0 {
            self.dpr
        } else {
            0.0
        }
    }

    /// Additive pierce damage-through bonus for this hostile firing at the player (engine counter-attack path).
    ///
    /// **Assumption:** [`ShipRecord`] does not yet expose player hull `DefenderStats` (armor / deflection /
    /// dodge components). We use [`DefenderStats::default`] (zeros) so hostile piercing is still
    /// reflected in the pierce-through term; the player’s profile-based [`crate::combat::Combatant::mitigation`]
    /// continues to apply separately on incoming fire.
    pub fn counter_pierce_damage_through_bonus(&self, player_ship_type: ShipType) -> f64 {
        crate::combat::pierce_damage_through_bonus(
            DefenderStats {
                armor: 0.0,
                shield_deflection: 0.0,
                dodge: 0.0,
            },
            self.to_attacker_stats(),
            player_ship_type,
        )
    }

    /// Faction tag for defender-context ship abilities ("weapon damage vs Klingon", etc.).
    ///
    /// Prefers mapped upstream `faction.id` values; when `id` is unknown (`-1`), uses `faction.loca_id`
    /// where that id matches `translations-factions.json` `faction_name` rows (e.g. 2 = Klingon).
    /// Unmapped combinations → [`OpponentFactionTag::Unknown`].
    pub fn opponent_faction_tag(&self) -> OpponentFactionTag {
        let Some(f) = self.faction.as_ref() else {
            return OpponentFactionTag::Unknown;
        };
        opponent_faction_from_upstream_id(f.id)
            .or_else(|| f.loca_id.and_then(opponent_faction_from_faction_loca_id))
            .unwrap_or(OpponentFactionTag::Unknown)
    }

    pub fn ship_type(&self) -> ShipType {
        ship_class_to_type(&self.ship_class)
    }
}

fn opponent_faction_from_upstream_id(id: i64) -> Option<OpponentFactionTag> {
    match id {
        2064723306 => Some(OpponentFactionTag::Federation),
        4153667145 => Some(OpponentFactionTag::Klingon),
        669838839 => Some(OpponentFactionTag::Romulan),
        2706737293 | 2943562711 | 356485724 => Some(OpponentFactionTag::Borg),
        3536012672 => Some(OpponentFactionTag::Cardassian),
        2504833658 | 1645257079 => Some(OpponentFactionTag::MirrorUniverse),
        1742105784 => Some(OpponentFactionTag::Actian),
        1530685377 => Some(OpponentFactionTag::Dominion),
        133823197 => Some(OpponentFactionTag::GornHuntingPack),
        4138978039 => Some(OpponentFactionTag::ExBorg),
        157476182 => Some(OpponentFactionTag::Assimilated),
        2352723133 | 2745774076 | 505966333 => Some(OpponentFactionTag::Xindi),
        2489857622 => Some(OpponentFactionTag::Swarm),
        1125688202 => Some(OpponentFactionTag::Breen),
        _ => None,
    }
}

fn opponent_faction_from_faction_loca_id(loca: u64) -> Option<OpponentFactionTag> {
    match loca {
        1 => Some(OpponentFactionTag::Federation),
        2 => Some(OpponentFactionTag::Klingon),
        3 => Some(OpponentFactionTag::Romulan),
        11 => Some(OpponentFactionTag::Augment),
        12 => Some(OpponentFactionTag::Swarm),
        14 => Some(OpponentFactionTag::Assimilated),
        17 => Some(OpponentFactionTag::Actian),
        19 => Some(OpponentFactionTag::Cardassian),
        21 => Some(OpponentFactionTag::Dominion),
        24 => Some(OpponentFactionTag::ExBorg),
        30 | 31 | 32 => Some(OpponentFactionTag::Xindi),
        33 => Some(OpponentFactionTag::GornHuntingPack),
        34 | 36 => Some(OpponentFactionTag::MirrorUniverse),
        84003 => Some(OpponentFactionTag::Borg),
        80004 => Some(OpponentFactionTag::Breen),
        _ => None,
    }
}

/// Parse one `components[].data` value into weapon stats when `tag == "Weapon"` and damage is present.
fn weapon_stats_from_component_data(data: &Value) -> Option<WeaponStats> {
    let obj = data.as_object()?;
    let tag = obj.get("tag")?.as_str()?;
    if !tag.eq_ignore_ascii_case("weapon") {
        return None;
    }
    let min = obj
        .get("minimum_damage")
        .or_else(|| obj.get("min_damage"))
        .and_then(json_f64);
    let max = obj
        .get("maximum_damage")
        .or_else(|| obj.get("max_damage"))
        .and_then(json_f64);
    let attack = match (min, max) {
        (Some(a), Some(b)) => (a + b) / 2.0,
        (None, Some(b)) => b,
        (Some(a), None) => a,
        (None, None) => obj
            .get("damage")
            .or_else(|| obj.get("dps"))
            .and_then(json_f64)?,
    };
    if !attack.is_finite() || attack <= 0.0 {
        return None;
    }
    let shots = obj.get("shots").and_then(|v| {
        v.as_u64()
            .map(|u| u as u32)
            .or_else(|| v.as_i64().map(|i| i.max(0) as u32))
    });
    let ap = obj.get("armor_piercing").and_then(json_f64).unwrap_or(0.0);
    let sp = obj.get("shield_piercing").and_then(json_f64).unwrap_or(0.0);
    let pierce = if ap > 0.0 || sp > 0.0 {
        Some(ap + sp)
    } else {
        None
    };
    let crit_chance = obj
        .get("crit_chance")
        .or_else(|| obj.get("critical_chance"))
        .and_then(json_f64);
    let crit_multiplier = obj
        .get("crit_damage")
        .or_else(|| obj.get("critical_damage"))
        .or_else(|| obj.get("crit_multiplier"))
        .and_then(json_f64)
        .filter(|v| v.is_finite() && *v > 0.0);
    let proc_chance = obj.get("proc_chance").and_then(json_f64);
    let proc_multiplier = obj
        .get("proc_multiplier")
        .and_then(json_f64)
        .filter(|v| v.is_finite() && *v > 0.0);

    Some(WeaponStats {
        attack,
        shots,
        pierce,
        crit_chance,
        crit_multiplier,
        proc_chance,
        proc_multiplier,
    })
}

fn json_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|u| u as f64))
}

pub fn ship_class_to_type(ship_class: &str) -> ShipType {
    match ship_class.to_lowercase().as_str() {
        "battleship" => ShipType::Battleship,
        "explorer" => ShipType::Explorer,
        "interceptor" => ShipType::Interceptor,
        "survey" => ShipType::Survey,
        "armada" => ShipType::Armada,
        _ => ShipType::Battleship,
    }
}

/// Maps data.stfc.space `hull_type` to KOBAYASHI `ship_class` string.
pub fn hull_type_raw_to_ship_class(hull_type: u32) -> Option<&'static str> {
    match hull_type {
        0 => Some("battleship"),
        1 => Some("survey"),
        2 => Some("interceptor"),
        3 => Some("explorer"),
        5 => Some("survey"),
        _ => None,
    }
}

pub const DEFAULT_HOSTILES_INDEX_PATH: &str = "data/hostiles/index.json";

/// Load hostile index from data/hostiles/index.json. Returns None if file missing.
pub fn load_hostile_index(path: &str) -> Option<HostileIndex> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Load a single hostile record by id from data/hostiles/<id>.json.
pub fn load_hostile_record(data_dir: &Path, id: &str) -> Option<HostileRecord> {
    let path = data_dir.join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{OpponentFactionTag, ShipType};

    #[test]
    fn opponent_faction_tag_maps_upstream_faction_id_and_loca_fallback() {
        let j = r#"{
            "id":"k1","hostile_name":"K","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":4153667145,"loca_id":2}
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        assert_eq!(r.opponent_faction_tag(), OpponentFactionTag::Klingon);

        let j2 = r#"{
            "id":"f1","hostile_name":"F","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":-1,"loca_id":1}
        }"#;
        let r2: HostileRecord = serde_json::from_str(j2).unwrap();
        assert_eq!(r2.opponent_faction_tag(), OpponentFactionTag::Federation);
    }

    #[test]
    fn hostile_record_deserializes_legacy_minimal_json() {
        let j = r#"{"id":"actian_apex_33_interceptor","hostile_name":"Actian Apex","level":33,"ship_class":"interceptor","armor":1.0,"shield_deflection":2.0,"dodge":3.0,"hull_health":100.0,"shield_health":50.0}"#;
        let r: HostileRecord = serde_json::from_str(j).expect("legacy hostile JSON");
        assert_eq!(r.id, "actian_apex_33_interceptor");
        assert!(r.components.is_empty() && r.ability.is_empty());
        assert_eq!(r.upstream_ship_type, 0);
    }

    #[test]
    fn hostile_record_deserializes_extended_fields() {
        let j = r#"{"id":"2918121098","hostile_name":"Hostile 2918121098","level":81,"ship_class":"explorer","armor":1.0,"shield_deflection":2.0,"dodge":3.0,"hull_health":10.0,"shield_health":5.0,"upstream_ship_type":2,"hull_type_raw":3,"components":[{"k":1}],"ability":[]}"#;
        let r: HostileRecord = serde_json::from_str(j).expect("extended hostile JSON");
        assert_eq!(r.components.len(), 1);
    }

    #[test]
    fn hull_type_raw_mapping_known_values() {
        assert_eq!(hull_type_raw_to_ship_class(0), Some("battleship"));
        assert_eq!(hull_type_raw_to_ship_class(3), Some("explorer"));
        assert_eq!(hull_type_raw_to_ship_class(99), None);
    }

    #[test]
    fn weapons_from_components_parses_weapon_tag_and_shots() {
        let j = r#"{
            "id":"h1","hostile_name":"X","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "components":[
                {"data":{"tag":"Weapon","minimum_damage":100,"maximum_damage":200,"shots":3}}
            ]
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        let w = r.weapons_from_components();
        assert_eq!(w.len(), 1);
        assert!((w[0].attack - 150.0).abs() < 1e-9);
        assert_eq!(w[0].shots, Some(3));
    }

    #[test]
    fn counter_pierce_uses_attacker_stats() {
        let j = r#"{
            "id":"h2","hostile_name":"Y","level":1,"ship_class":"explorer",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "armor_piercing":500.0,"shield_piercing":400.0,"accuracy":300.0
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        let p = r.counter_pierce_damage_through_bonus(ShipType::Explorer);
        assert!(p > 0.0 && p <= crate::combat::PIERCE_CAP);
    }
}
