//! Ship data: normalized combat stats for player ships (from STFCcommunity or manual).
//! Used to build AttackerStats + Combatant when resolving by name/id.
//!
//! **Hull abilities:** Upstream `ability` JSON is normalized into [`ShipAbility`] on
//! [`ExtendedShipRecord`] / [`ShipRecord`], then converted to combat seats in
//! [`crate::data::ship_ability_resolve`] and merged in
//! [`crate::optimizer::monte_carlo::scenario`]. See `docs/DESIGN.md` §3.6 (Ship hull abilities).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// Weapon damage type slug (`"energy"` / `"kinetic"`), from upstream `weapon_type` (1 = Energy, 2 = Kinetic). Unset for legacy data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type: Option<String>,
}

/// Maps the upstream numeric `weapon_type` (1 = Energy, 2 = Kinetic) to the slug stored on [`WeaponRecord`].
pub fn weapon_type_slug_from_upstream(v: Option<i64>) -> Option<&'static str> {
    match v {
        Some(1) => Some("energy"),
        Some(2) => Some("kinetic"),
        _ => None,
    }
}

/// Parses a [`WeaponRecord::weapon_type`] slug into the engine [`crate::combat::WeaponType`].
pub fn weapon_type_from_slug(slug: Option<&str>) -> crate::combat::WeaponType {
    match slug {
        Some("energy") => crate::combat::WeaponType::Energy,
        Some("kinetic") => crate::combat::WeaponType::Kinetic,
        _ => crate::combat::WeaponType::Unknown,
    }
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
    /// Negative hull-class gate: effect applies only when the defender is **not** this class
    /// (e.g. `armada` for "vs non-Armada hostiles" ability text such as Dauntless Seek and Destroy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition_opponent_ship_class_not: Option<String>,
    /// Gated on the defender being an NPC hostile (not a player ship): "against hostiles" ability
    /// text that must stay inert in PvP (e.g. Dauntless Seek and Destroy cannot target players).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub condition_defender_is_npc_hostile: bool,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Per-ship officer-stat breakpoint tables (attack/defense/health rating → bonus%). Carried
    /// from [`ExtendedShipRecord::officer_bonus`] so combat consumers don't need the unresolved
    /// extended record. Empty for legacy/hostile-derived ships; safe default produces zero
    /// bonus from the lookup helpers. See `docs/OFFICER_STAT_FORMULA.md` §2a.
    #[serde(default, skip_serializing_if = "OfficerBonusTable::is_empty")]
    pub officer_bonus: OfficerBonusTable,
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

const COMPONENT_WEAPON_ORDER_LAST: i64 = i64::MAX;

pub const DEFAULT_HULL_ID_REGISTRY_PATH: &str = "data/hull_id_registry.json";
pub const DEFAULT_UPSTREAM_SHIPS_DIR: &str = "data/upstream/data-stfc-space/ships";

/// Load hull id -> canonical ship id mapping. Returns an empty map if the registry is missing.
pub fn load_hull_id_registry() -> HashMap<i64, String> {
    let raw = match fs::read_to_string(DEFAULT_HULL_ID_REGISTRY_PATH) {
        Ok(s) => s,
        _ => return HashMap::new(),
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        _ => return HashMap::new(),
    };
    let obj = match parsed.get("hull_id_to_ship_id").and_then(Value::as_object) {
        Some(o) => o,
        None => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for (k, v) in obj {
        if let (Ok(hid), Some(sid)) = (k.parse::<i64>(), v.as_str()) {
            out.insert(hid, sid.to_string());
        }
    }
    out
}

/// Load one raw data.stfc.space ship payload by numeric hull id.
pub fn load_upstream_ship_raw(upstream_dir: &Path, numeric_id: i64) -> Option<Value> {
    if numeric_id <= 0 {
        return None;
    }
    let path = upstream_dir.join(format!("{numeric_id}.json"));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn component_data(comp: &Value) -> Option<&Value> {
    comp.get("data")
}

fn component_tag(comp: &Value) -> Option<&str> {
    component_data(comp)?
        .get("tag")
        .and_then(Value::as_str)
        .filter(|tag| !tag.is_empty())
}

fn component_order(comp: &Value) -> i64 {
    comp.get("order")
        .and_then(Value::as_i64)
        .filter(|order| *order >= 0)
        .unwrap_or(-1)
}

fn component_slot_index(components: &[Value]) -> HashMap<(String, i64, usize), usize> {
    let mut index = HashMap::new();
    let mut counts: HashMap<(String, i64), usize> = HashMap::new();
    for (idx, comp) in components.iter().enumerate() {
        let Some(tag) = component_tag(comp) else {
            continue;
        };
        let tag = tag.to_string();
        let order = component_order(comp);
        let count_key = (tag.clone(), order);
        let occurrence = counts.entry(count_key).or_insert(0);
        index.insert((tag, order, *occurrence), idx);
        *occurrence += 1;
    }
    index
}

fn raw_ship_components_for_tier(raw_ship: &Value, tier: u32) -> Option<Vec<Value>> {
    let tiers = raw_ship.get("tiers").and_then(Value::as_array)?;
    let tier_row = tiers
        .iter()
        .find(|row| row.get("tier").and_then(Value::as_u64) == Some(u64::from(tier)))?;
    Some(
        tier_row
            .get("components")
            .and_then(Value::as_array)?
            .to_vec(),
    )
}

fn component_id_index(raw_ship: &Value) -> HashMap<i64, Value> {
    let mut out = HashMap::new();
    let Some(tiers) = raw_ship.get("tiers").and_then(Value::as_array) else {
        return out;
    };
    for tier in tiers {
        let Some(components) = tier.get("components").and_then(Value::as_array) else {
            continue;
        };
        for comp in components {
            let Some(id) = comp.get("id").and_then(Value::as_i64).filter(|id| *id > 0) else {
                continue;
            };
            out.entry(id).or_insert_with(|| comp.clone());
        }
    }
    out
}

fn patched_components_for_ids(
    base_components: &[Value],
    raw_ship: &Value,
    component_ids: &[i64],
) -> Option<Vec<Value>> {
    let by_id = component_id_index(raw_ship);
    if by_id.is_empty() {
        return None;
    }

    let base_slots = component_slot_index(base_components);
    if base_slots.is_empty() {
        return None;
    }

    let mut patched = base_components.to_vec();
    let mut selected_counts: HashMap<(String, i64), usize> = HashMap::new();
    let mut matched = 0usize;

    for id in component_ids.iter().copied().filter(|id| *id > 0) {
        let Some(component) = by_id.get(&id) else {
            continue;
        };
        let Some(tag) = component_tag(component) else {
            continue;
        };
        let tag = tag.to_string();
        let order = component_order(component);
        let count_key = (tag.clone(), order);
        let occurrence = selected_counts.entry(count_key).or_insert(0);
        let slot_key = (tag, order, *occurrence);
        *occurrence += 1;

        if let Some(idx) = base_slots.get(&slot_key) {
            patched[*idx] = component.clone();
            matched += 1;
        }
    }

    (matched > 0).then_some(patched)
}

/// Extract combat stats from a component list using the same rules as the ship normalizer.
///
/// These stats are **one component tier only**. Normalized [`TierStats`] in `data/ships_extended`
/// keeps cumulative offensive piercing/accuracy across tiers; component overrides apply a delta
/// from this current-tier extraction onto the already-resolved base ship record.
pub fn extract_component_tier_stats(components: &[Value], tier: u32) -> TierStats {
    let mut hull_health = 0.0;
    let mut shield_health = 0.0;
    let mut shield_mitigation = 0.8;
    let mut armor_stat = 0.0;
    let mut shield_deflection_stat = 0.0;
    let mut dodge_stat = 0.0;
    let mut weapon_components: Vec<(i64, &Value)> = Vec::new();

    for comp in components {
        let Some(data) = component_data(comp) else {
            continue;
        };
        match data.get("tag").and_then(Value::as_str).unwrap_or("") {
            "Weapon" => {
                let order = comp
                    .get("order")
                    .and_then(Value::as_i64)
                    .filter(|order| *order >= 0)
                    .unwrap_or(COMPONENT_WEAPON_ORDER_LAST);
                weapon_components.push((order, data));
            }
            "Shield" => {
                shield_health = data.get("hp").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(mitigation) = data.get("mitigation").and_then(Value::as_f64) {
                    shield_mitigation = mitigation;
                }
                if let Some(absorption) = data.get("absorption").and_then(Value::as_f64) {
                    shield_deflection_stat = absorption;
                }
            }
            "Armor" => {
                hull_health = data.get("hp").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(plating) = data.get("plating").and_then(Value::as_f64) {
                    armor_stat = plating;
                }
            }
            "Impulse" => {
                if let Some(dodge) = data.get("dodge").and_then(Value::as_f64) {
                    dodge_stat = dodge;
                }
            }
            _ => {}
        }
    }

    weapon_components.sort_by_key(|(order, _)| *order);

    let mut armor_piercing = 0.0;
    let mut shield_piercing = 0.0;
    let mut accuracy = 0.0;
    let mut attack = 0.0;
    let mut crit_chance = 0.1;
    let mut crit_damage = 1.5;
    let mut weapons = Vec::new();
    let mut first_weapon = true;

    for (_, data) in weapon_components {
        let penetration = data
            .get("penetration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let modulation = data
            .get("modulation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let weapon_accuracy = data.get("accuracy").and_then(Value::as_f64).unwrap_or(0.0);
        let min_damage = data
            .get("minimum_damage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let max_damage = data
            .get("maximum_damage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let shots = data
            .get("shots")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as u32;

        armor_piercing += penetration;
        shield_piercing += modulation;
        accuracy += weapon_accuracy;

        let avg_damage = (min_damage + max_damage) * 0.5;
        attack += avg_damage * f64::from(shots);

        let row_crit_chance = data.get("crit_chance").and_then(Value::as_f64);
        let row_crit_damage = data
            .get("crit_modifier")
            .or_else(|| data.get("crit_damage"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0);
        let row_proc_chance = data.get("proc_chance").and_then(Value::as_f64);
        let row_proc_multiplier = data
            .get("proc_multiplier")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0);

        if first_weapon {
            first_weapon = false;
            if let Some(value) = row_crit_chance {
                crit_chance = value;
            }
            if let Some(value) = row_crit_damage {
                crit_damage = value;
            }
        }

        weapons.push(WeaponRecord {
            attack: avg_damage,
            shots: Some(shots),
            armor_piercing: Some(penetration),
            shield_piercing: Some(modulation),
            accuracy: Some(weapon_accuracy),
            crit_chance: row_crit_chance,
            crit_multiplier: row_crit_damage,
            proc_chance: row_proc_chance,
            proc_multiplier: row_proc_multiplier,
            weapon_type: weapon_type_slug_from_upstream(
                data.get("weapon_type").and_then(Value::as_i64),
            )
            .map(str::to_string),
            ..Default::default()
        });
    }

    if attack <= 0.0 {
        attack = 100.0;
    }
    if hull_health <= 0.0 {
        hull_health = shield_health * 2.0;
    }

    TierStats {
        tier,
        armor_piercing,
        shield_piercing,
        accuracy,
        armor: armor_stat,
        shield_deflection: shield_deflection_stat,
        dodge: dodge_stat,
        attack,
        crit_chance,
        crit_damage,
        hull_health,
        shield_health,
        shield_mitigation: Some(shield_mitigation),
        weapons: (!weapons.is_empty()).then_some(weapons),
    }
}

fn add_finite_delta(target: &mut f64, before: f64, after: f64) {
    let delta = after - before;
    if delta.is_finite() {
        *target += delta;
    }
}

/// Finite delta `after - before`, or 0.0 when the difference is non-finite (NaN/inf).
fn finite_delta(before: f64, after: f64) -> f64 {
    let delta = after - before;
    if delta.is_finite() {
        delta
    } else {
        0.0
    }
}

/// Per-stat deltas applied to a ship's base tier/level stats by synced component overrides.
///
/// Produced by [`apply_component_overrides_to_ship_record`] and surfaced to the UI so players can
/// see that upgraded components above hull tier are actually factored into sim/optimize. All fields
/// are additive deltas over the base record (0.0 = unchanged).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ComponentOverrideSummary {
    pub armor_piercing: f64,
    pub shield_piercing: f64,
    pub accuracy: f64,
    pub armor: f64,
    pub shield_deflection: f64,
    pub dodge: f64,
    pub attack: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub hull_health: f64,
    pub shield_health: f64,
}

impl ComponentOverrideSummary {
    /// True when any stat delta is non-zero — i.e. components differ from the base hull tier.
    pub fn applied(&self) -> bool {
        [
            self.armor_piercing,
            self.shield_piercing,
            self.accuracy,
            self.armor,
            self.shield_deflection,
            self.dodge,
            self.attack,
            self.crit_chance,
            self.crit_damage,
            self.hull_health,
            self.shield_health,
        ]
        .iter()
        .any(|d| *d != 0.0)
    }
}

/// Apply synced per-component ids to an already-resolved ship record.
///
/// The base record remains the source of cumulative tier/level stats. Component ids replace the
/// matching raw tier components by slot (tag + order + occurrence), then the current-tier delta is
/// applied to the base record. Matching component lists are therefore a no-op, while upgraded parts
/// above the hull tier adjust only their affected combat stats.
///
/// Returns the per-stat deltas it applied, or `None` when component data is unavailable.
pub fn apply_component_overrides_to_ship_record(
    ship: &mut ShipRecord,
    raw_ship: &Value,
    tier: u32,
    component_ids: &[i64],
) -> Option<ComponentOverrideSummary> {
    let expected_components = raw_ship_components_for_tier(raw_ship, tier)?;
    let patched_components =
        patched_components_for_ids(&expected_components, raw_ship, component_ids)?;

    let expected = extract_component_tier_stats(&expected_components, tier);
    let patched = extract_component_tier_stats(&patched_components, tier);

    let summary = ComponentOverrideSummary {
        armor_piercing: finite_delta(expected.armor_piercing, patched.armor_piercing),
        shield_piercing: finite_delta(expected.shield_piercing, patched.shield_piercing),
        accuracy: finite_delta(expected.accuracy, patched.accuracy),
        armor: finite_delta(expected.armor, patched.armor),
        shield_deflection: finite_delta(expected.shield_deflection, patched.shield_deflection),
        dodge: finite_delta(expected.dodge, patched.dodge),
        attack: finite_delta(expected.attack, patched.attack),
        crit_chance: finite_delta(expected.crit_chance, patched.crit_chance),
        crit_damage: finite_delta(expected.crit_damage, patched.crit_damage),
        hull_health: finite_delta(expected.hull_health, patched.hull_health),
        shield_health: finite_delta(expected.shield_health, patched.shield_health),
    };

    ship.armor_piercing += summary.armor_piercing;
    ship.shield_piercing += summary.shield_piercing;
    ship.accuracy += summary.accuracy;
    ship.armor += summary.armor;
    ship.shield_deflection += summary.shield_deflection;
    ship.dodge += summary.dodge;
    ship.attack += summary.attack;
    ship.crit_chance += summary.crit_chance;
    ship.crit_damage += summary.crit_damage;
    ship.hull_health += summary.hull_health;
    ship.shield_health += summary.shield_health;

    if let (Some(current), Some(before), Some(after)) = (
        ship.shield_mitigation,
        expected.shield_mitigation,
        patched.shield_mitigation,
    ) {
        let mut next = current;
        add_finite_delta(&mut next, before, after);
        ship.shield_mitigation = Some(next);
    } else {
        ship.shield_mitigation = patched.shield_mitigation;
    }
    ship.weapons = patched.weapons;
    Some(summary)
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

    /// Step-function lookup: returns the largest `bonus` whose `value` threshold is reached by
    /// `rating`. Returns `0.0` when rating is below the first breakpoint or the table is empty.
    /// Assumes the breakpoint array is sorted by `value` ascending (the normalizer enforces this).
    fn bonus_for(table: &[OfficerBonusBreakpoint], rating: f64) -> f64 {
        let mut result = 0.0;
        for bp in table {
            if rating >= bp.value {
                result = bp.bonus;
            } else {
                break;
            }
        }
        result
    }

    pub fn attack_bonus(&self, rating: f64) -> f64 {
        Self::bonus_for(&self.attack, rating)
    }

    pub fn defense_bonus(&self, rating: f64) -> f64 {
        Self::bonus_for(&self.defense, rating)
    }

    pub fn health_bonus(&self, rating: f64) -> f64 {
        Self::bonus_for(&self.health, rating)
    }
}

#[cfg(test)]
mod officer_bonus_table_tests {
    use super::*;

    fn cerritos_attack_table() -> OfficerBonusTable {
        // Cerritos attack channel: 1500→50%, 3000→100%, ..., 45000→500%. Verified in
        // docs/OFFICER_STAT_FORMULA.md §2b against in-game Sesha L15 (1726 rating → 50%) and
        // Ghrush L30 (91354 rating → 500%) observations.
        OfficerBonusTable {
            attack: vec![
                OfficerBonusBreakpoint {
                    value: 1500.0,
                    bonus: 0.5,
                },
                OfficerBonusBreakpoint {
                    value: 3000.0,
                    bonus: 1.0,
                },
                OfficerBonusBreakpoint {
                    value: 45000.0,
                    bonus: 5.0,
                },
            ],
            defense: vec![],
            health: vec![],
        }
    }

    #[test]
    fn bonus_below_first_breakpoint_is_zero() {
        let t = cerritos_attack_table();
        assert_eq!(t.attack_bonus(0.0), 0.0);
        assert_eq!(t.attack_bonus(1499.99), 0.0);
    }

    #[test]
    fn bonus_at_breakpoint_takes_that_breakpoint() {
        let t = cerritos_attack_table();
        assert_eq!(t.attack_bonus(1500.0), 0.5);
        assert_eq!(t.attack_bonus(3000.0), 1.0);
        assert_eq!(t.attack_bonus(45000.0), 5.0);
    }

    #[test]
    fn bonus_between_breakpoints_holds_lower_value() {
        // Step function: between 1500 and 3000, bonus stays at 0.5; not interpolated.
        let t = cerritos_attack_table();
        assert_eq!(t.attack_bonus(2000.0), 0.5);
        assert_eq!(t.attack_bonus(2999.99), 0.5);
    }

    #[test]
    fn bonus_above_last_breakpoint_caps_at_last_bonus() {
        let t = cerritos_attack_table();
        assert_eq!(t.attack_bonus(100_000.0), 5.0);
    }

    #[test]
    fn empty_channel_returns_zero() {
        let t = OfficerBonusTable::default();
        assert_eq!(t.attack_bonus(99_999.0), 0.0);
        assert_eq!(t.defense_bonus(99_999.0), 0.0);
        assert_eq!(t.health_bonus(99_999.0), 0.0);
    }

    #[test]
    fn cerritos_in_game_observations_match() {
        // From docs/OFFICER_STAT_FORMULA.md §2b/§2c/§2d (Sesha L15 + Ghrush L30 + Chen).
        // Use the actual Cerritos breakpoints rather than the simplified table.
        let cerritos: OfficerBonusTable = serde_json::from_value(serde_json::json!({
            "attack": [
                {"value": 1500.0, "bonus": 0.5},
                {"value": 3000.0, "bonus": 1.0},
                {"value": 4875.0, "bonus": 1.5},
                {"value": 7500.0, "bonus": 2.0},
                {"value": 11250.0, "bonus": 2.5},
                {"value": 15000.0, "bonus": 3.0},
                {"value": 20625.0, "bonus": 3.5},
                {"value": 27500.0, "bonus": 4.0},
                {"value": 35000.0, "bonus": 4.5},
                {"value": 45000.0, "bonus": 5.0}
            ],
            "defense": [
                {"value": 1500.0, "bonus": 0.5},
                {"value": 3000.0, "bonus": 1.0},
                {"value": 45000.0, "bonus": 5.0}
            ],
            "health": [
                {"value": 1500.0, "bonus": 0.5},
                {"value": 4875.0, "bonus": 1.5},
                {"value": 11250.0, "bonus": 2.5},
                {"value": 45000.0, "bonus": 5.0}
            ]
        }))
        .unwrap();
        // Sesha L15 alone: attack=1726 → 50%, defense=4374 → 100%, health=834 → 0%.
        assert_eq!(cerritos.attack_bonus(1726.0), 0.5);
        assert_eq!(cerritos.defense_bonus(4374.0), 1.0);
        assert_eq!(cerritos.health_bonus(834.0), 0.0);
        // Chen: health=11620 → 250%.
        assert_eq!(cerritos.health_bonus(11620.0), 2.5);
        // Ghrush L30 alone: attack=91354 → 500%, defense=93209 → 500%, health=91240 → 500%.
        assert_eq!(cerritos.attack_bonus(91354.0), 5.0);
        assert_eq!(cerritos.defense_bonus(93209.0), 5.0);
        assert_eq!(cerritos.health_bonus(91240.0), 5.0);
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
            officer_bonus: self.officer_bonus.clone(),
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
                        weapon_type: weapon_type_from_slug(r.weapon_type.as_deref()),
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

    fn component_fixture_raw_ship() -> Value {
        serde_json::json!({
            "tiers": [
                {
                    "tier": 1,
                    "components": [
                        {"id": 1, "order": -1, "data": {"tag": "Armor", "hp": 100.0, "plating": 10.0}},
                        {"id": 2, "order": 8, "data": {"tag": "Shield", "hp": 200.0, "mitigation": 0.8, "absorption": 20.0}},
                        {"id": 3, "order": -1, "data": {"tag": "Impulse", "dodge": 5.0}},
                        {"id": 4, "order": 1, "data": {
                            "tag": "Weapon",
                            "minimum_damage": 10.0,
                            "maximum_damage": 20.0,
                            "shots": 2,
                            "penetration": 7.0,
                            "modulation": 8.0,
                            "accuracy": 9.0,
                            "crit_chance": 0.2,
                            "crit_modifier": 2.0
                        }}
                    ]
                },
                {
                    "tier": 2,
                    "components": [
                        {"id": 5, "order": -1, "data": {"tag": "Armor", "hp": 150.0, "plating": 14.0}},
                        {"id": 6, "order": 8, "data": {"tag": "Shield", "hp": 260.0, "mitigation": 0.8, "absorption": 30.0}},
                        {"id": 7, "order": -1, "data": {"tag": "Impulse", "dodge": 8.0}},
                        {"id": 8, "order": 1, "data": {
                            "tag": "Weapon",
                            "minimum_damage": 20.0,
                            "maximum_damage": 40.0,
                            "shots": 3,
                            "penetration": 11.0,
                            "modulation": 13.0,
                            "accuracy": 17.0,
                            "crit_chance": 0.25,
                            "crit_modifier": 2.5
                        }}
                    ]
                }
            ]
        })
    }

    fn component_fixture_ship_record() -> ShipRecord {
        ShipRecord {
            id: "fixture".to_string(),
            ship_name: "Fixture".to_string(),
            ship_class: "battleship".to_string(),
            faction: None,
            // Pretend these are cumulative tier values; component overrides add only deltas.
            armor_piercing: 70.0,
            shield_piercing: 80.0,
            accuracy: 90.0,
            armor: 10.0,
            shield_deflection: 20.0,
            dodge: 5.0,
            attack: 30.0,
            crit_chance: 0.2,
            crit_damage: 2.0,
            hull_health: 110.0,
            shield_health: 205.0,
            shield_mitigation: Some(0.8),
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: Some(vec![WeaponRecord {
                attack: 15.0,
                shots: Some(2),
                armor_piercing: Some(7.0),
                shield_piercing: Some(8.0),
                accuracy: Some(9.0),
                crit_chance: Some(0.2),
                crit_multiplier: Some(2.0),
                ..Default::default()
            }]),
            abilities: None,
            officer_bonus: OfficerBonusTable::default(),
        }
    }

    #[test]
    fn component_overrides_are_noop_when_profile_ids_match_base_tier() {
        let raw = component_fixture_raw_ship();
        let mut ship = component_fixture_ship_record();

        let summary = apply_component_overrides_to_ship_record(&mut ship, &raw, 1, &[1, 2, 3, 4])
            .expect("override resolves");
        assert!(!summary.applied(), "matching components produce no delta");

        assert_eq!(ship.armor_piercing, 70.0);
        assert_eq!(ship.shield_piercing, 80.0);
        assert_eq!(ship.accuracy, 90.0);
        assert_eq!(ship.attack, 30.0);
        assert_eq!(ship.hull_health, 110.0);
        assert_eq!(ship.shield_health, 205.0);
        assert_eq!(ship.armor, 10.0);
        assert_eq!(ship.shield_deflection, 20.0);
        assert_eq!(ship.dodge, 5.0);
        assert_eq!(ship.weapons.as_ref().unwrap()[0].attack, 15.0);
    }

    #[test]
    fn component_overrides_apply_deltas_above_base_tier() {
        let raw = component_fixture_raw_ship();
        let mut ship = component_fixture_ship_record();

        let summary = apply_component_overrides_to_ship_record(&mut ship, &raw, 1, &[5, 6, 7, 8])
            .expect("override resolves");
        assert!(summary.applied());
        assert_eq!(summary.attack, 60.0); // 90 - 30
        assert_eq!(summary.hull_health, 50.0); // 160 - 110
        assert_eq!(summary.armor_piercing, 4.0); // 74 - 70

        assert_eq!(ship.armor_piercing, 74.0);
        assert_eq!(ship.shield_piercing, 85.0);
        assert_eq!(ship.accuracy, 98.0);
        assert_eq!(ship.attack, 90.0);
        assert_eq!(ship.crit_chance, 0.25);
        assert_eq!(ship.crit_damage, 2.5);
        assert_eq!(ship.hull_health, 160.0);
        assert_eq!(ship.shield_health, 265.0);
        assert_eq!(ship.armor, 14.0);
        assert_eq!(ship.shield_deflection, 30.0);
        assert_eq!(ship.dodge, 8.0);
        let weapon = &ship.weapons.as_ref().unwrap()[0];
        assert_eq!(weapon.attack, 30.0);
        assert_eq!(weapon.shots, Some(3));
        assert_eq!(weapon.armor_piercing, Some(11.0));
        assert_eq!(weapon.shield_piercing, Some(13.0));
        assert_eq!(weapon.accuracy, Some(17.0));
    }

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
