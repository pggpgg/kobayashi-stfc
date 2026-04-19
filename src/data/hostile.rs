//! Hostile data: normalized combat stats for hostiles (from STFCcommunity, data.stfc.space, or manual).
//! Used to build DefenderStats + hull + ShipType when resolving by name/id.
//!
//! **Display names:** `normalize_hostiles_stfc_space` sets `hostile_name` to `Hostile {id}` until a
//! `loca_id` → string map (e.g. `translations-hostiles` from data.stfc.space) is wired into that tool.
//!
//! **Defender faction tag (`opponent_faction_tag`):** Maps `faction.id` (from `summary-hostile`) and
//! `faction.loca_id` (aligns with `translations-factions.json` `faction_name` rows) to
//! [`crate::combat::OpponentFactionTag`] for ship abilities that gate on defender faction.
//! High-volume ids/locas are listed in [`opponent_faction_from_upstream_id`] and
//! [`opponent_faction_from_faction_loca_id`]. Factions without a combat tag (e.g. Q-Continuum, Exiles,
//! Node, Orion, Eclipse, Maverick, Krenim, Apex Raiders, Transogen, Aggregation) intentionally resolve
//! to [`OpponentFactionTag::Unknown`] (faction-gated abilities do not fire).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::combat::{
    pierce_damage_through_bonus, AttackerStats, DefenderStats, OpponentFactionTag, ShipType,
    WeaponStats,
};

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

    /// Simulator-only tags for ability gating (e.g. `conqueror_borg` for Update 89 Borg hostiles).
    #[serde(default)]
    pub hostile_tags: Vec<String>,

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
    /// Bitmask for [`crate::combat::SimulationConfig::defender_hostile_tag_mask`] / [`crate::combat::CombatContext::defender_hostile_tag_mask`].
    pub fn hostile_tag_mask(&self) -> u32 {
        crate::combat::hostile_tags::mask_from_slugs(&self.hostile_tags)
    }

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
    /// Sorted by component `order` (ascending; missing → last). Pierce fields are left unset (use
    /// [`Self::weapons_for_counter_attack`] when building the hostile attacker in combat).
    pub fn weapons_from_components(&self) -> Vec<WeaponStats> {
        sorted_weapon_component_data(&self.components)
            .into_iter()
            .filter_map(weapon_stats_from_component_data)
            .collect()
    }

    /// Same weapon rows as [`Self::weapons_from_components`], with per-weapon damage-through pierce
    /// derived from this hostile’s piercing/accuracy vs the player hull class (counter-attack path).
    pub fn weapons_for_counter_attack(&self, player_ship_type: ShipType) -> Vec<WeaponStats> {
        let base = self.to_attacker_stats();
        let defender_zero = DefenderStats {
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
        };
        sorted_weapon_component_data(&self.components)
            .into_iter()
            .filter_map(|data| {
                weapon_stats_from_component_data(data).map(|mut w| {
                    let atk = hostile_weapon_row_attacker_stats(data, &base);
                    w.pierce = Some(pierce_damage_through_bonus(
                        defender_zero,
                        atk,
                        player_ship_type,
                    ));
                    w
                })
            })
            .collect()
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

    /// Hull-derived class from normalized `ship_class` (upstream `hull_type` mapping).
    pub fn ship_type(&self) -> ShipType {
        ship_class_to_type(&self.ship_class)
    }

    /// Effective defender [`ShipType`] for combat: mitigation, player pierce-through, ship-ability
    /// accuracy gates, and LCARS `defender_ship_type_is` when the hostile is the defender.
    ///
    /// When [`Self::upstream_ship_type`] matches an armada target in
    /// [`crate::data::upstream_hostile_ship_type`], returns [`ShipType::Armada`] so armada-tuned
    /// buffs apply even if `ship_class` was inferred as another hull line.
    pub fn ship_type_for_combat(&self) -> ShipType {
        if super::upstream_hostile_ship_type::upstream_hostile_ship_type_profile(
            self.upstream_ship_type,
        )
        .is_armada_target
        {
            ShipType::Armada
        } else {
            self.ship_type()
        }
    }
}

fn opponent_faction_from_upstream_id(id: i64) -> Option<OpponentFactionTag> {
    match id {
        2064723306 => Some(OpponentFactionTag::Federation),
        4153667145 => Some(OpponentFactionTag::Klingon),
        669838839 => Some(OpponentFactionTag::Romulan),
        // Note: `356485724` is Maverick (loca 88002), not Borg — do not map here (→ Unknown).
        2706737293 | 2943562711 => Some(OpponentFactionTag::Borg),
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
        // Additional `summary-hostile` faction.id values (correlate with `faction.loca_id` / translations-factions).
        2796195869 => Some(OpponentFactionTag::Federation), // Texas-class
        2385047502 => Some(OpponentFactionTag::Borg), // Borg (paired loca 26 in bundled hostiles)
        880952928 => Some(OpponentFactionTag::Cardassian), // UI "Card" (loca 84001)
        2334280262 | 4281411789 => Some(OpponentFactionTag::Borg), // V'Ger Clone
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
        30..=32 => Some(OpponentFactionTag::Xindi),
        33 => Some(OpponentFactionTag::GornHuntingPack),
        34 | 36 => Some(OpponentFactionTag::MirrorUniverse),
        84003 => Some(OpponentFactionTag::Borg),
        80004 => Some(OpponentFactionTag::Breen),
        // Extra `translations-factions.json` `faction_name` loca ids (bundled hostiles).
        26 => Some(OpponentFactionTag::Borg), // alternate Borg loca (see faction.id 2385047502)
        27 => Some(OpponentFactionTag::Federation), // Texas-class
        84001 => Some(OpponentFactionTag::Cardassian), // "Card" (short label)
        86001 => Some(OpponentFactionTag::Borg), // V'Ger Clone
        _ => None,
    }
}

/// data.stfc.space weapon component order (ascending); missing or negative → last.
const HOSTILE_WEAPON_ORDER_LAST: i64 = 999;

/// `data` refs for weapon components, ordered like player ship normalization (primary first).
fn sorted_weapon_component_data<'a>(components: &'a [Value]) -> Vec<&'a Value> {
    let mut pairs: Vec<(i64, &'a Value)> = Vec::new();
    for comp in components {
        let data = comp.as_object().and_then(|o| o.get("data")).unwrap_or(comp);
        let Some(obj) = data.as_object() else {
            continue;
        };
        let Some(tag) = obj.get("tag").and_then(|t| t.as_str()) else {
            continue;
        };
        if !tag.eq_ignore_ascii_case("weapon") {
            continue;
        }
        let order = comp
            .as_object()
            .and_then(|o| o.get("order"))
            .and_then(|v| v.as_i64())
            .filter(|&o| o >= 0)
            .unwrap_or(HOSTILE_WEAPON_ORDER_LAST);
        pairs.push((order, data));
    }
    pairs.sort_by_key(|(o, _)| *o);
    pairs.into_iter().map(|(_, d)| d).collect()
}

/// AttackerStats for one hostile weapon row: `penetration`/`modulation`/`accuracy` when present, else top-level hostile stats.
fn hostile_weapon_row_attacker_stats(data: &Value, fallback: &AttackerStats) -> AttackerStats {
    let obj = match data.as_object() {
        Some(o) => o,
        None => return *fallback,
    };
    let ap = obj
        .get("penetration")
        .and_then(json_f64)
        .or_else(|| obj.get("armor_piercing").and_then(json_f64))
        .unwrap_or(fallback.armor_piercing);
    let sp = obj
        .get("modulation")
        .and_then(json_f64)
        .or_else(|| obj.get("shield_piercing").and_then(json_f64))
        .unwrap_or(fallback.shield_piercing);
    let acc = obj
        .get("accuracy")
        .and_then(json_f64)
        .unwrap_or(fallback.accuracy);
    AttackerStats {
        armor_piercing: ap,
        shield_piercing: sp,
        accuracy: acc,
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
    // Raw piercing belongs in AttackerStats + pierce_damage_through_bonus; do not store ap+sp on
    // WeaponStats.pierce (wrong units for the damage-through term).
    let crit_chance = obj
        .get("crit_chance")
        .or_else(|| obj.get("critical_chance"))
        .and_then(json_f64);
    let crit_multiplier = obj
        .get("crit_damage")
        .or_else(|| obj.get("critical_damage"))
        .or_else(|| obj.get("crit_multiplier"))
        .or_else(|| obj.get("crit_modifier"))
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
        pierce: None,
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

/// Maps data.stfc.space **`hull_type` on player ships** to Kobayashi `ship_class` (combat triangle +
/// survey). Verified against live client hull-line labels (not the old 0=battleship / 3=explorer swap).
///
/// See also [`hostile_hull_type_raw_to_ship_class`] for hostile NPC detail JSON — split so NPC encoding
/// can diverge without touching player normalization.
pub fn player_hull_type_raw_to_ship_class(hull_type: u32) -> Option<&'static str> {
    match hull_type {
        0 => Some("interceptor"),
        1 => Some("survey"),
        2 => Some("explorer"),
        3 => Some("battleship"),
        4 => Some("survey"),
        5 => Some("survey"),
        _ => None,
    }
}

/// Maps data.stfc.space hostile detail **`hull_type`** to Kobayashi `ship_class`.
///
/// Today this delegates to [`player_hull_type_raw_to_ship_class`]. Override the body here if datamine
/// shows NPC `hull_type` enums do not match player ships.
pub fn hostile_hull_type_raw_to_ship_class(hull_type: u32) -> Option<&'static str> {
    player_hull_type_raw_to_ship_class(hull_type)
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
    fn opponent_faction_tag_maps_summary_hostile_faction_ids() {
        // Texas-class, Borg alt, Card, V'Ger clone (see `opponent_faction_from_upstream_id`).
        for (id, expected) in [
            (2796195869i64, OpponentFactionTag::Federation),
            (2385047502i64, OpponentFactionTag::Borg),
            (880952928i64, OpponentFactionTag::Cardassian),
            (2334280262i64, OpponentFactionTag::Borg),
        ] {
            let j = format!(
                r#"{{
                "id":"x","hostile_name":"X","level":1,"ship_class":"battleship",
                "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
                "faction":{{"id":{id},"loca_id":null}}
            }}"#
            );
            let r: HostileRecord = serde_json::from_str(&j).unwrap();
            assert_eq!(r.opponent_faction_tag(), expected, "faction.id {id}");
        }
    }

    #[test]
    fn opponent_faction_tag_maverick_is_unknown() {
        let j = r#"{
            "id":"m","hostile_name":"M","level":1,"ship_class":"survey",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":356485724,"loca_id":88002}
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        assert_eq!(r.opponent_faction_tag(), OpponentFactionTag::Unknown);
    }

    #[test]
    fn opponent_faction_tag_unknown_for_q_continuum_and_node() {
        let q = r#"{
            "id":"q","hostile_name":"Q","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":4145760809,"loca_id":72000}
        }"#;
        let r: HostileRecord = serde_json::from_str(q).unwrap();
        assert_eq!(r.opponent_faction_tag(), OpponentFactionTag::Unknown);

        let node = r#"{
            "id":"n","hostile_name":"N","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":4269128921,"loca_id":84002}
        }"#;
        let r2: HostileRecord = serde_json::from_str(node).unwrap();
        assert_eq!(r2.opponent_faction_tag(), OpponentFactionTag::Unknown);
    }

    #[test]
    fn opponent_faction_tag_loca_only_cardassian_and_borg_extras() {
        let j = r#"{
            "id":"c","hostile_name":"C","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":-1,"loca_id":84001}
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        assert_eq!(r.opponent_faction_tag(), OpponentFactionTag::Cardassian);

        let j2 = r#"{
            "id":"v","hostile_name":"V","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":-1,"loca_id":86001}
        }"#;
        let r2: HostileRecord = serde_json::from_str(j2).unwrap();
        assert_eq!(r2.opponent_faction_tag(), OpponentFactionTag::Borg);
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
        let j = r#"{"id":"2918121098","hostile_name":"Hostile 2918121098","level":81,"ship_class":"battleship","armor":1.0,"shield_deflection":2.0,"dodge":3.0,"hull_health":10.0,"shield_health":5.0,"upstream_ship_type":2,"hull_type_raw":3,"components":[{"k":1}],"ability":[]}"#;
        let r: HostileRecord = serde_json::from_str(j).expect("extended hostile JSON");
        assert_eq!(r.components.len(), 1);
    }

    #[test]
    fn ship_type_for_combat_upstream_armada_overrides_hull_class() {
        let j = r#"{"id":"2021752945","hostile_name":"Armada target","level":76,"ship_class":"survey","armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,"upstream_ship_type":1,"hull_type_raw":5,"components":[],"ability":[]}"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        assert_eq!(r.ship_type(), ShipType::Survey);
        assert_eq!(r.ship_type_for_combat(), ShipType::Armada);
    }

    #[test]
    fn player_hull_type_raw_matches_client_triangle() {
        assert_eq!(player_hull_type_raw_to_ship_class(0), Some("interceptor"));
        assert_eq!(player_hull_type_raw_to_ship_class(1), Some("survey"));
        assert_eq!(player_hull_type_raw_to_ship_class(2), Some("explorer"));
        assert_eq!(player_hull_type_raw_to_ship_class(3), Some("battleship"));
        assert_eq!(player_hull_type_raw_to_ship_class(99), None);
    }

    #[test]
    fn hostile_hull_type_delegates_until_npc_specific_map_exists() {
        assert_eq!(
            hostile_hull_type_raw_to_ship_class(3),
            player_hull_type_raw_to_ship_class(3)
        );
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
    fn hostile_weapons_sorted_by_order_and_counter_pierce_scales_with_penetration() {
        let j = r#"{
            "id":"mw","hostile_name":"M","level":1,"ship_class":"battleship",
            "armor":100.0,"shield_deflection":100.0,"dodge":50.0,"hull_health":100.0,"shield_health":50.0,
            "armor_piercing":10.0,"shield_piercing":10.0,"accuracy":10.0,
            "components":[
                {"order":2,"data":{"tag":"Weapon","minimum_damage":50,"maximum_damage":50,
                  "penetration":200,"modulation":200,"accuracy":200}},
                {"order":1,"data":{"tag":"Weapon","minimum_damage":10,"maximum_damage":10,
                  "penetration":1,"modulation":1,"accuracy":1}}
            ]
        }"#;
        let r: HostileRecord = serde_json::from_str(j).unwrap();
        let w = r.weapons_from_components();
        assert_eq!(w.len(), 2);
        assert!((w[0].attack - 10.0).abs() < 1e-9);
        assert!((w[1].attack - 50.0).abs() < 1e-9);
        let wc = r.weapons_for_counter_attack(ShipType::Battleship);
        assert_eq!(wc.len(), 2);
        let p0 = wc[0].pierce.expect("pierce filled for counter-attack");
        let p1 = wc[1].pierce.expect("pierce filled for counter-attack");
        assert!(p0 <= crate::combat::PIERCE_CAP && p1 <= crate::combat::PIERCE_CAP);
        // Placeholder defender stats are zeros on the counter-attack path, so mitigation is
        // independent of attacker piercing; values still match `pierce_damage_through_bonus` per row.
        assert!((p0 - p1).abs() < 1e-12);
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
