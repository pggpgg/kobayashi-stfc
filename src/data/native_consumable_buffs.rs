//! Verified setup buffs from native static ConsumableBuffs group 116 only.
//!
//! This group also contains permanent account bonuses. Research, building and
//! forbidden-tech IDs are excluded during extraction, so importing global buff
//! state cannot apply those existing profile sources a second time.
//! GlobalBuffEntry.level is an external one-based level. Historical battle
//! ActiveBuff.Ranks use zero-based indices and are a different input contract.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext, ShipType,
    TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::import::GlobalBuffEntry;

const EMBEDDED_CATALOG: &str = include_str!("../../data/native_consumable_buff_catalog.json");

#[derive(Debug, Deserialize)]
pub struct NativeConsumableBuffCatalog {
    schema_version: u32,
    source_group: u32,
    buffs: BTreeMap<i64, NativeBuffSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct NativeBuffSpec {
    #[serde(default)]
    sha256: String,
    modifier_code: i64,
    operation_code: i32,
    target_code: i32,
    trigger_code: i32,
    #[serde(default)]
    faction_id: i64,
    #[serde(default)]
    num_rounds: i32,
    condition_codes: Vec<i64>,
    attribute_fields: Vec<u32>,
    unknown_fields: Vec<u32>,
    ranked_value_type: i32,
    ranked_values: Vec<f64>,
    ranked_value_count: usize,
    ranked_chances: Vec<f64>,
    ranked_chance_count: usize,
}

impl NativeConsumableBuffCatalog {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub(crate) fn contains_buff(&self, bid: i64) -> bool {
        self.buffs.contains_key(&bid)
    }
}

fn embedded_catalog() -> &'static NativeConsumableBuffCatalog {
    static CATALOG: OnceLock<NativeConsumableBuffCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        NativeConsumableBuffCatalog::from_json(EMBEDDED_CATALOG)
            .expect("reviewed embedded native consumable catalog must parse")
    })
}

/// Return additive profile-pool contributions at the imported current levels.
/// `attacker_ship_class` uses native class slugs, including `sentinel`; an absent
/// or unrecognized class cannot satisfy a class-gated definition. Expiry uses
/// Unix seconds, matching the mod's protobuf Timestamp.seconds export.
pub fn bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    bonuses_from_catalog(
        embedded_catalog(),
        imported,
        attacker_ship_class,
        now_unix_seconds,
    )
}

/// Compile native combat-start all-defense rows as conditional virtual ship
/// seats. These are separate from the precombat launch percentage pools.
pub fn combat_start_defense_seats_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    combat_start_defense_seats_from_catalog_for_group(
        embedded_catalog(),
        116,
        imported,
        attacker_ship_class,
        now_unix_seconds,
    )
}

/// Reviewed group-116 offensive rows: self, CombatStart, NPC and non-Armada.
/// Keep these out of launch pools and never infer other predicates from names.
pub fn combat_start_offense_seats_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    combat_start_offense_seats_from_catalog(embedded_catalog(), imported, now_unix_seconds)
}

fn combat_start_offense_seats_from_catalog(
    catalog: &NativeConsumableBuffCatalog,
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    if catalog.schema_version != 1 || catalog.source_group != 116 {
        return Vec::new();
    }
    active_highest_levels(imported, now_unix_seconds)
        .into_iter()
        .filter_map(|(bid, level)| {
            let spec = catalog.buffs.get(&bid)?;
            if spec.condition_codes != [27, 131] || spec.num_rounds != 0 {
                return None;
            }
            // Reuse the reviewed target, trigger, rank and predicate checks;
            // only the two explicitly selected offensive operations differ.
            let effect: fn(f64) -> AbilityEffect = match (spec.modifier_code, spec.operation_code) {
                (2, 3) => AbilityEffect::BaseWeaponDamageBonus,
                (10, 1) => AbilityEffect::BaseCritDamageBonus,
                _ => return None,
            };
            let condition = combat_start_condition(spec, 116, bid, None)?;
            let rank = usize::try_from(level - 1).unwrap_or(usize::MAX);
            let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
            (value != 0.0).then(|| CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    name: format!("native_116_{bid}"),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: effect(value),
                    condition,
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            })
        })
        .collect()
}

pub(crate) fn combat_start_defense_seats_from_catalog_for_group(
    catalog: &NativeConsumableBuffCatalog,
    expected_group: u32,
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    if catalog.schema_version != 1 || catalog.source_group != expected_group {
        return Vec::new();
    }
    active_highest_levels(imported, now_unix_seconds)
        .into_iter()
        .filter_map(|(bid, level)| {
            let spec = catalog.buffs.get(&bid)?;
            if spec.modifier_code != 73 || spec.operation_code != 3 {
                return None;
            }
            let condition = combat_start_condition(spec, expected_group, bid, attacker_ship_class)?;
            let rank = usize::try_from(level - 1).unwrap_or(usize::MAX);
            let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
            (value != 0.0).then(|| CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    name: format!("native_{expected_group}_{bid}"),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::BaseMitigationStatsBonus(value),
                    condition,
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            })
        })
        .collect()
}

fn combat_start_condition(
    spec: &NativeBuffSpec,
    source_group: u32,
    bid: i64,
    attacker_ship_class: Option<&str>,
) -> Option<Option<AbilityCondition>> {
    // The native condition enum does not expose condition 17's predicate.
    // This exact research row is separately bound by its frozen BuffSpec hash,
    // faction attribute, and Eclipse project/localization evidence. Keep this
    // exception narrower than the other condition-17/attribute-8 rows.
    if source_group == 66 && bid == 1238121830 {
        const ECLIPSE_RANKS: [f64; 11] = [0.5, 1.0, 1.6, 2.1, 2.8, 3.6, 4.4, 5.2, 6.0, 7.0, 0.0];
        let exact = spec.sha256
            == "4f04a8d92ca23d5966f461f6e9339ec8a403d14c8a755e617765750f8bce1013"
            && spec.modifier_code == 73
            && spec.operation_code == 3
            && spec.target_code == 1
            && spec.trigger_code == 25
            && spec.condition_codes == [17]
            && spec.attribute_fields == [8]
            && spec.unknown_fields.is_empty()
            && spec.num_rounds == 0
            && spec.faction_id == 1_750_120_904
            && spec.ranked_value_type == 0
            && spec.ranked_values == ECLIPSE_RANKS
            && spec.ranked_value_count == 99
            && spec.ranked_chances == [1.0]
            && spec.ranked_chance_count == 99;
        return exact.then_some(Some(AbilityCondition::DefenderHullFactionIdIs(
            1_750_120_904,
        )));
    }
    if spec.target_code != 1
        || spec.trigger_code != 25
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
    let mut conditions = Vec::new();
    let mut seen = HashSet::new();
    let mut class_count = 0;
    for code in &spec.condition_codes {
        if !seen.insert(*code) {
            return None;
        }
        let condition = match code {
            0 => AbilityCondition::AttackerShipTypeIs(ShipType::Explorer),
            1 => AbilityCondition::AttackerShipTypeIs(ShipType::Interceptor),
            2 => AbilityCondition::AttackerShipTypeIs(ShipType::Battleship),
            3 => AbilityCondition::AttackerShipTypeIs(ShipType::Survey),
            // Sentinel is a native class but not a simulator ShipType. The
            // scenario hull class is fixed before combat, so retain its gate.
            116 => AbilityCondition::LiteralBool(attacker_ship_class == Some("sentinel")),
            27 => AbilityCondition::DefenderIsNpcHostile,
            // These three exact research IDs have independent native project
            // localization binding the selected condition to an enemy class.
            // The enum evaluator does not establish a general 4/5/6 meaning.
            4 if source_group == 66 && bid == 2708670536 => {
                AbilityCondition::DefenderShipTypeIs(ShipType::Explorer)
            }
            5 if source_group == 66 && bid == 266932754 => {
                AbilityCondition::DefenderShipTypeIs(ShipType::Interceptor)
            }
            6 if source_group == 66 && bid == 455315064 => {
                AbilityCondition::DefenderShipTypeIs(ShipType::Battleship)
            }
            131 => AbilityCondition::Not(Box::new(AbilityCondition::DefenderShipTypeIs(
                ShipType::Armada,
            ))),
            _ => return None,
        };
        if matches!(code, 0 | 1 | 2 | 3 | 116) {
            class_count += 1;
            if class_count > 1 {
                return None;
            }
        }
        conditions.push(condition);
    }
    Some(match conditions.len() {
        0 => None,
        1 => Some(conditions.remove(0)),
        _ => Some(AbilityCondition::And(conditions)),
    })
}

fn active_highest_levels(
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> BTreeMap<i64, i64> {
    // Sync ingress stores one row per bid; duplicate inputs still select the
    // highest nonexpired current level deterministically.
    let mut levels = BTreeMap::new();
    for buff in imported.iter().filter(|buff| {
        buff.bid > 0
            && buff.level > 0
            && buff
                .expiry_time
                .is_none_or(|expiry| expiry > now_unix_seconds)
    }) {
        levels
            .entry(buff.bid)
            .and_modify(|level: &mut i64| *level = (*level).max(buff.level))
            .or_insert(buff.level);
    }
    levels
}

/// Explicit-catalog variant for validation and future runtime data reloads.
pub fn bonuses_from_catalog(
    catalog: &NativeConsumableBuffCatalog,
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    bonuses_from_catalog_for_group(
        catalog,
        116,
        imported,
        attacker_ship_class,
        None,
        now_unix_seconds,
    )
}

pub(crate) fn bonuses_from_catalog_for_group(
    catalog: &NativeConsumableBuffCatalog,
    expected_group: u32,
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    syndicate_level: Option<u32>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    bonuses_from_catalog_for_ship(
        catalog,
        expected_group,
        imported,
        attacker_ship_class,
        syndicate_level,
        None,
        now_unix_seconds,
    )
}

pub(crate) fn bonuses_from_catalog_for_ship(
    catalog: &NativeConsumableBuffCatalog,
    expected_group: u32,
    imported: &[GlobalBuffEntry],
    attacker_ship_class: Option<&str>,
    syndicate_level: Option<u32>,
    ship_grade: Option<u32>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    if catalog.schema_version != 1 || catalog.source_group != expected_group {
        return out;
    }
    for (bid, level) in active_highest_levels(imported, now_unix_seconds) {
        let Some(spec) = catalog.buffs.get(&bid) else {
            continue;
        };
        let stats =
            if expected_group == 66 && matches!(spec.modifier_code, 6 | 7 | 8 | 74 | 56..=59) {
                supported_research_stat_sources(spec, bid, attacker_ship_class, ship_grade)
            } else {
                supported_stats_with_syndicate(
                    spec,
                    attacker_ship_class,
                    syndicate_level,
                    expected_group == 68,
                )
            };
        let Some(stats) = stats else {
            continue;
        };
        let rank = usize::try_from(level - 1).unwrap_or(usize::MAX);
        let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
        if value != 0.0 {
            for stat in stats {
                *out.entry((*stat).to_owned()).or_insert(0.0) += value;
            }
        }
    }
    out
}

/// Same reviewed launch families as the native research importer. Target 5 is
/// crew-wide; target 11 is admitted only for the hash-bound Orlop Defense row.
fn supported_research_stat_sources(
    spec: &NativeBuffSpec,
    bid: i64,
    ship_class: Option<&str>,
    ship_grade: Option<u32>,
) -> Option<&'static [&'static str]> {
    if spec.operation_code != 3
        || spec.trigger_code != 24
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
    if spec.target_code == 11 {
        return (bid == 1895868262
            && spec.modifier_code == 57
            && spec.sha256 == "90eb257edcaceff8659184bcf2cdf1b41d4ce616dded4db5487b3ad806ec11ac"
            && spec.condition_codes.is_empty()
            && spec.ranked_values
                == [0.2, 0.26, 0.33, 0.41, 0.5, 0.59, 0.69, 0.79, 0.89, 1.0, 0.0]
            && spec.ranked_value_count == 99
            && spec.ranked_chances == [1.0]
            && spec.ranked_chance_count == 99)
            .then_some(&["officer_defense_below_decks"]);
    }
    if matches!(spec.modifier_code, 56..=59) {
        if spec.target_code != 5 || !spec.condition_codes.is_empty() {
            return None;
        }
        return match spec.modifier_code {
            56 => Some(&["officer_attack"]),
            57 => Some(&["officer_defense"]),
            58 => Some(&["officer_health"]),
            59 => Some(&["officer_attack", "officer_defense", "officer_health"]),
            _ => None,
        };
    }
    if spec.target_code != 1 {
        return None;
    }
    let matches_scope = match spec.condition_codes.as_slice() {
        [] => true,
        [0] => ship_class == Some("explorer"),
        [1] => ship_class == Some("interceptor"),
        [2] => ship_class == Some("battleship"),
        [3] => ship_class == Some("survey"),
        [116] => ship_class == Some("sentinel"),
        [123] if spec.modifier_code == 74 => ship_grade.is_some_and(|grade| grade >= 4),
        _ => false,
    };
    if !matches_scope {
        return None;
    }
    match spec.modifier_code {
        6 => Some(&["accuracy"]),
        7 => Some(&["armor_piercing"]),
        8 => Some(&["shield_piercing"]),
        74 => Some(&["accuracy", "armor_piercing", "shield_piercing"]),
        _ => None,
    }
}

#[cfg(test)]
fn supported_stats(
    spec: &NativeBuffSpec,
    ship_class: Option<&str>,
) -> Option<&'static [&'static str]> {
    supported_stats_with_syndicate(spec, ship_class, None, false)
}

fn supported_stats_with_syndicate(
    spec: &NativeBuffSpec,
    ship_class: Option<&str>,
    syndicate_level: Option<u32>,
    allow_syndicate: bool,
) -> Option<&'static [&'static str]> {
    if spec.target_code != 1
        || !matches!(spec.trigger_code, 24 | 25)
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
    let required_class = match spec.condition_codes.as_slice() {
        [] => None,
        [0] => Some("explorer"),
        [1] => Some("interceptor"),
        [2] => Some("battleship"),
        [3] => Some("survey"),
        [116] => Some("sentinel"),
        [code @ 203..=205] if allow_syndicate => {
            let minimum = match code {
                203 => 30,
                204 => 33,
                _ => 35,
            };
            if syndicate_level.is_none_or(|level| level < minimum) {
                return None;
            }
            None
        }
        _ => return None,
    };
    if required_class.is_some() && required_class != ship_class {
        return None;
    }
    // All values are native doubles. ADD is absolute HP or a direct addition to
    // critical/isolytic baselines; MULT is the additive percentage pool. The
    // new defense families are limited to proven launch-time effects.
    if matches!(spec.modifier_code, 11 | 12 | 13 | 73) && spec.trigger_code != 24 {
        return None;
    }
    match (spec.modifier_code, spec.operation_code) {
        (61, 1) => Some(&["hull_hp_flat"]),
        (60, 1) => Some(&["shield_hp_flat"]),
        (61, 3) => Some(&["hull_hp"]),
        (60, 3) => Some(&["shield_hp"]),
        (2, 3) => Some(&["weapon_damage"]),
        (9, 1) => Some(&["crit_chance"]),
        (10, 1) => Some(&["crit_damage_flat"]),
        // Native ModIsolyticDamage707 ADD enters the baseline pool. MULT is a
        // separate downstream cascade and cannot be flattened into this key.
        (707, 1) => Some(&["isolytic_damage"]),
        // Native ModIsolyticDefense808 ADD enters the defense baseline pool.
        (808, 1) => Some(&["isolytic_defense"]),
        (11, 3) => Some(&["dodge"]),
        (12, 3) => Some(&["armor"]),
        (13, 3) => Some(&["shield_deflection"]),
        (73, 3) => Some(&["armor", "shield_deflection", "dodge"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{CombatContext, EnemyTypes, OpponentFactionTag};
    use std::sync::Arc;

    fn entry(bid: i64, level: i64, expiry_time: Option<i64>) -> GlobalBuffEntry {
        GlobalBuffEntry {
            bid,
            level,
            expiry_time,
        }
    }

    fn spec() -> NativeBuffSpec {
        NativeBuffSpec {
            sha256: String::new(),
            modifier_code: 61,
            operation_code: 3,
            target_code: 1,
            trigger_code: 24,
            faction_id: -1,
            num_rounds: 0,
            condition_codes: vec![],
            attribute_fields: vec![],
            unknown_fields: vec![],
            ranked_value_type: 0,
            ranked_values: vec![0.5, 5.0, 0.0],
            ranked_value_count: 3,
            ranked_chances: vec![1.0],
            ranked_chance_count: 3,
        }
    }

    fn catalog(rows: Vec<(i64, NativeBuffSpec)>) -> NativeConsumableBuffCatalog {
        NativeConsumableBuffCatalog {
            schema_version: 1,
            source_group: 116,
            buffs: rows.into_iter().collect(),
        }
    }

    fn combat_context() -> CombatContext {
        CombatContext {
            round_index: 1,
            defender_hull_pct: 1.0,
            defender_shield_pct: 1.0,
            attacker_hull_pct: 1.0,
            attacker_shield_pct: 1.0,
            attacker_morale_active: false,
            defender_morale_active: false,
            defender_burning_active: false,
            defender_hull_breach_active: false,
            attacker_burning_active: false,
            attacker_hull_breach_active: false,
            defender_assimilated_active: false,
            defender_faction: OpponentFactionTag::Unknown,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Explorer,
            attacker_ship_id: Arc::from("test_ship"),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: Arc::new(EnemyTypes::default()),
            combat_battle_type_id: None,
            defender_level: None,
        }
    }

    #[test]
    fn combat_start_defense_seat_keeps_npc_non_armada_and_self_class_gates() {
        let mut row = spec();
        row.modifier_code = 73;
        row.trigger_code = 25;
        row.condition_codes = vec![0, 27, 131];
        row.ranked_values = vec![1.25, 2.5, 0.0];
        let c = catalog(vec![(42, row)]);
        let imported = [
            entry(42, 1, None),
            entry(42, 2, None),
            entry(42, 3, Some(100)),
        ];
        let seats = combat_start_defense_seats_from_catalog_for_group(
            &c,
            116,
            &imported,
            Some("explorer"),
            100,
        );
        assert_eq!(seats.len(), 1);
        let seat = &seats[0];
        assert_eq!(seat.seat, CrewSeat::Ship);
        assert_eq!(seat.ability.name, "native_116_42");
        assert_eq!(seat.ability.class, AbilityClass::ShipAbility);
        assert_eq!(seat.ability.timing, TimingWindow::CombatBegin);
        assert!(!seat.ability.boostable);
        assert_eq!(
            seat.ability.effect,
            AbilityEffect::BaseMitigationStatsBonus(2.5)
        );
        let condition = seat.ability.condition.as_ref().unwrap();
        assert!(condition.evaluate(&combat_context()));
        let mut pvp = combat_context();
        pvp.defender_is_npc_hostile = false;
        pvp.defender_is_player_ship = true;
        assert!(!condition.evaluate(&pvp));
        let mut armada = combat_context();
        armada.defender_ship_type = ShipType::Armada;
        assert!(!condition.evaluate(&armada));
        let mut interceptor = combat_context();
        interceptor.attacker_ship_type = ShipType::Interceptor;
        assert!(!condition.evaluate(&interceptor));
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &c,
            68,
            &imported,
            Some("explorer"),
            100,
        )
        .is_empty());
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &c,
            116,
            &[entry(42, 3, None)],
            Some("explorer"),
            100,
        )
        .is_empty());
    }

    #[test]
    fn combat_start_defense_rejects_unproven_conditions_and_bad_native_facts() {
        let mut valid = spec();
        valid.modifier_code = 73;
        valid.trigger_code = 25;
        valid.ranked_values = vec![1.0];
        let mut unsupported = Vec::new();
        for codes in [
            vec![4],
            vec![5],
            vec![6],
            vec![27, 4],
            vec![0, 1],
            vec![27, 27],
        ] {
            let mut row = valid.clone();
            row.condition_codes = codes;
            unsupported.push(row);
        }
        for change in [
            (1, 24, 3, 1),
            (73, 18, 3, 1),
            (73, 25, 1, 1),
            (73, 25, 3, 6),
        ] {
            let mut row = valid.clone();
            (
                row.modifier_code,
                row.trigger_code,
                row.operation_code,
                row.target_code,
            ) = change;
            unsupported.push(row);
        }
        for change in [1, 2, 3, 4] {
            let mut row = valid.clone();
            match change {
                1 => row.ranked_values = vec![f64::INFINITY],
                2 => row.ranked_chances = vec![0.5],
                3 => row.attribute_fields = vec![12],
                _ => row.unknown_fields = vec![99],
            }
            unsupported.push(row);
        }
        for row in unsupported {
            assert!(combat_start_defense_seats_from_catalog_for_group(
                &catalog(vec![(42, row)]),
                116,
                &[entry(42, 1, None)],
                Some("explorer"),
                0,
            )
            .is_empty());
        }
        let mut sentinel = valid;
        sentinel.condition_codes = vec![116, 27];
        let c = catalog(vec![(42, sentinel)]);
        let seats = combat_start_defense_seats_from_catalog_for_group(
            &c,
            116,
            &[entry(42, 1, None)],
            Some("sentinel"),
            0,
        );
        assert_eq!(seats.len(), 1);
        assert_eq!(
            seats[0].ability.condition,
            Some(AbilityCondition::And(vec![
                AbilityCondition::LiteralBool(true),
                AbilityCondition::DefenderIsNpcHostile,
            ]))
        );
    }

    #[test]
    fn exact_research_enemy_class_rows_preserve_localized_static_scope() {
        let rows = [
            (2708670536, 4, ShipType::Explorer),
            (266932754, 5, ShipType::Interceptor),
            (455315064, 6, ShipType::Battleship),
        ];
        for (bid, code, class) in rows {
            let mut row = spec();
            row.modifier_code = 73;
            row.trigger_code = 25;
            row.condition_codes = vec![27, code];
            row.ranked_values = vec![0.75];
            let mut c = catalog(vec![(bid, row.clone())]);
            c.source_group = 66;
            let seats = combat_start_defense_seats_from_catalog_for_group(
                &c,
                66,
                &[entry(bid, 1, None)],
                Some("explorer"),
                0,
            );
            assert_eq!(seats.len(), 1);
            let condition = seats[0].ability.condition.as_ref().unwrap();
            let mut matching = combat_context();
            matching.defender_ship_type = class;
            assert!(condition.evaluate(&matching));
            matching.defender_is_npc_hostile = false;
            matching.defender_is_player_ship = true;
            assert!(!condition.evaluate(&matching));
            let mut other_class = combat_context();
            other_class.defender_ship_type = if class == ShipType::Explorer {
                ShipType::Battleship
            } else {
                ShipType::Explorer
            };
            assert!(!condition.evaluate(&other_class));
            c.buffs.remove(&bid);
            c.buffs.insert(999, row);
            assert!(combat_start_defense_seats_from_catalog_for_group(
                &c,
                66,
                &[entry(999, 1, None)],
                Some("explorer"),
                0,
            )
            .is_empty());
            assert!(combat_start_defense_seats_from_catalog_for_group(
                &c,
                116,
                &[entry(999, 1, None)],
                Some("explorer"),
                0,
            )
            .is_empty());
        }
    }

    #[test]
    fn eclipse_combat_start_buff_requires_the_exact_native_row_and_faction() {
        let catalog = NativeConsumableBuffCatalog::from_json(include_str!(
            "../../data/research/client_buff_semantics_2026-10-20.json"
        ))
        .unwrap();
        let imported = [entry(1238121830, 10, None)];
        let seats = combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            66,
            &imported,
            Some("explorer"),
            0,
        );
        assert_eq!(seats.len(), 1);
        assert_eq!(
            seats[0].ability.effect,
            AbilityEffect::BaseMitigationStatsBonus(7.0)
        );
        let condition = seats[0].ability.condition.as_ref().unwrap();
        let mut eclipse = combat_context();
        eclipse.defender_hull_faction_id = 1_750_120_904;
        assert!(condition.evaluate(&eclipse));
        eclipse.defender_hull_faction_id = 157_476_182;
        assert!(!condition.evaluate(&eclipse));
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            116,
            &imported,
            Some("explorer"),
            0,
        )
        .is_empty());

        // Same broad native condition/attribute shape, but a different BID,
        // faction and rank curve: never infer a generic condition-17 meaning.
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            66,
            &[entry(2117532952, 10, None)],
            Some("explorer"),
            0,
        )
        .is_empty());

        let row = catalog.buffs.get(&1238121830).unwrap().clone();
        let mut altered = Vec::new();
        for change in 0..6 {
            let mut candidate = row.clone();
            match change {
                0 => candidate.sha256.push('x'),
                1 => candidate.faction_id += 1,
                2 => candidate.condition_codes = vec![17, 27],
                3 => candidate.target_code = 2,
                4 => candidate.ranked_values[9] = 6.9,
                _ => candidate.ranked_chances[0] = 0.5,
            }
            altered.push(candidate);
        }
        for candidate in altered {
            let wrong = NativeConsumableBuffCatalog {
                schema_version: 1,
                source_group: 66,
                buffs: [(1238121830, candidate)].into_iter().collect(),
            };
            assert!(combat_start_defense_seats_from_catalog_for_group(
                &wrong,
                66,
                &imported,
                Some("explorer"),
                0,
            )
            .is_empty());
        }
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            66,
            &[],
            Some("explorer"),
            0,
        )
        .is_empty());
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            66,
            &[entry(1238121830, 10, Some(100))],
            Some("explorer"),
            100,
        )
        .is_empty());
        assert!(combat_start_defense_seats_from_catalog_for_group(
            &catalog,
            66,
            &[entry(1238121830, 0, None)],
            Some("explorer"),
            0,
        )
        .is_empty());
        assert!(
            bonuses_from_catalog_for_group(&catalog, 66, &imported, Some("explorer"), None, 0,)
                .is_empty(),
            "CombatStart source must not be duplicated in launch pools"
        );
    }

    #[test]
    fn pike_realta_active_npc_account_rows_sum_three_without_launch_leak() {
        let ids = [
            4285570420, 3568908575, 3241601998, 2646870581, 1531532954, 1232730640, 446001877,
            361450259, 35633979,
        ];
        let imported: Vec<_> = ids.into_iter().map(|bid| entry(bid, 61, None)).collect();
        let seats = combat_start_defense_seats_for_imported_buffs(&imported, Some("explorer"), 0);
        assert_eq!(seats.len(), 9);
        let total: f64 = seats
            .iter()
            .map(|seat| match seat.ability.effect {
                AbilityEffect::BaseMitigationStatsBonus(value) => value,
                other => panic!("unexpected {other:?}"),
            })
            .sum();
        assert!((total - 3.0).abs() < 1e-12);
        assert!(seats
            .iter()
            .all(|seat| seat.ability.condition == Some(AbilityCondition::DefenderIsNpcHostile)));
        let launch = bonuses_for_imported_buffs(&imported, Some("explorer"), 0);
        assert!(!launch.contains_key("armor"));
    }

    #[test]
    fn rank_units_expiry_and_dedup_do_not_revive_or_multiply_bonuses() {
        let c = catalog(vec![(1, spec())]);
        let eval = |rows: Vec<_>| bonuses_from_catalog(&c, &rows, None, 100);
        assert_eq!(eval(vec![entry(1, 2, None)]).get("hull_hp"), Some(&5.0));
        assert!(eval(vec![entry(1, 0, None), entry(1, -1, None)]).is_empty());
        assert!(eval(vec![entry(1, 3, None), entry(1, 100, None)]).is_empty());
        assert!(eval(vec![entry(1, 2, Some(100)), entry(1, 2, Some(99))]).is_empty());
        assert_eq!(
            eval(vec![
                entry(1, 1, None),
                entry(1, 2, Some(101)),
                entry(1, 2, None)
            ])
            .get("hull_hp"),
            Some(&5.0)
        );
        assert_eq!(
            eval(vec![entry(1, 1, None), entry(1, 2, Some(99))]).get("hull_hp"),
            Some(&0.5)
        );
    }

    #[test]
    fn unsupported_native_scope_never_becomes_unconditional() {
        let mut changes = vec![];
        let mut s = spec();
        s.condition_codes = vec![0, 999];
        changes.push(s);
        let mut s = spec();
        s.condition_codes = vec![999];
        changes.push(s);
        let mut s = spec();
        s.target_code = 6;
        changes.push(s);
        let mut s = spec();
        s.operation_code = 4;
        changes.push(s);
        let mut s = spec();
        s.trigger_code = 18;
        changes.push(s);
        let mut s = spec();
        s.ranked_value_type = 1;
        changes.push(s);
        let mut s = spec();
        s.ranked_chances = vec![0.5];
        changes.push(s);
        let mut s = spec();
        s.attribute_fields = vec![12];
        changes.push(s);
        let mut s = spec();
        s.unknown_fields = vec![99];
        changes.push(s);
        let mut s = spec();
        s.modifier_code = 0;
        changes.push(s);
        let mut s = spec();
        s.ranked_values = vec![f64::NAN];
        changes.push(s);
        for s in changes {
            assert!(bonuses_from_catalog(
                &catalog(vec![(1, s)]),
                &[entry(1, 1, None)],
                Some("explorer"),
                100
            )
            .is_empty());
        }
    }

    #[test]
    fn native_class_and_flat_additions_keep_their_distinct_meanings() {
        for (code, class) in [
            (0, "explorer"),
            (1, "interceptor"),
            (2, "battleship"),
            (3, "survey"),
            (116, "sentinel"),
        ] {
            let mut s = spec();
            s.condition_codes = vec![code];
            s.operation_code = 1;
            s.ranked_values = vec![3400.0];
            let c = catalog(vec![(1, s)]);
            assert_eq!(
                bonuses_from_catalog(&c, &[entry(1, 1, None)], Some(class), 0).get("hull_hp_flat"),
                Some(&3400.0)
            );
            assert!(bonuses_from_catalog(&c, &[entry(1, 1, None)], None, 0).is_empty());
            assert!(bonuses_from_catalog(&c, &[entry(1, 1, None)], Some("other"), 0).is_empty());
        }
        let mut s = spec();
        s.modifier_code = 10;
        s.operation_code = 1;
        s.ranked_values = vec![0.69];
        let c = catalog(vec![(1, s)]);
        let out = bonuses_from_catalog(&c, &[entry(1, 1, None)], None, 0);
        assert_eq!(out.get("crit_damage_flat"), Some(&0.69));
        assert!(!out.contains_key("crit_damage"));
    }

    #[test]
    fn isolytic_add_is_a_baseline_and_mult_or_unknown_scope_is_not() {
        let mut add = spec();
        add.modifier_code = 707;
        add.operation_code = 1;
        add.ranked_values = vec![0.015, 0.1, 0.11];
        let mut mult = add.clone();
        mult.operation_code = 3;
        let mut unknown = add.clone();
        unknown.condition_codes = vec![129];
        let c = catalog(vec![(1, add), (2, mult), (3, unknown)]);
        let out = bonuses_from_catalog(
            &c,
            &[entry(1, 3, None), entry(2, 3, None), entry(3, 3, None)],
            Some("explorer"),
            0,
        );
        assert_eq!(out.get("isolytic_damage"), Some(&0.11));
        assert_eq!(out.len(), 1);
        assert!(bonuses_from_catalog(&c, &[entry(1, 3, Some(0))], None, 0).is_empty());
        assert!(
            bonuses_from_catalog_for_group(&c, 68, &[entry(1, 3, None)], None, None, 0).is_empty()
        );
    }

    #[test]
    fn isolytic_defense_add_is_raw_and_unknown_scope_cannot_enter_setup_pool() {
        let mut add = spec();
        add.modifier_code = 808;
        add.operation_code = 1;
        add.ranked_values = vec![0.2, 0.63];
        let mut mult = add.clone();
        mult.operation_code = 3;
        let mut unknown = add.clone();
        unknown.condition_codes = vec![28];
        let c = catalog(vec![(1, add), (2, mult), (3, unknown)]);
        let out = bonuses_from_catalog(
            &c,
            &[entry(1, 2, None), entry(2, 2, None), entry(3, 2, None)],
            Some("explorer"),
            0,
        );
        assert_eq!(out.get("isolytic_defense"), Some(&0.63));
        assert_eq!(out.len(), 1);
        assert!(bonuses_from_catalog(&c, &[entry(1, 2, Some(0))], None, 0).is_empty());
    }

    #[test]
    fn launch_defense_modifier_maps_three_axes_and_quarantines_combat_start() {
        let mut all = spec();
        all.modifier_code = 73;
        all.ranked_values = vec![0.5, 1.25];
        let mut combat_start = all.clone();
        combat_start.trigger_code = 25;
        let mut plating = all.clone();
        plating.modifier_code = 12;
        plating.ranked_values = vec![0.2];
        let mut absorption = plating.clone();
        absorption.modifier_code = 13;
        let mut dodge = plating.clone();
        dodge.modifier_code = 11;
        let c = catalog(vec![
            (1, all),
            (2, combat_start),
            (3, plating),
            (4, absorption),
            (5, dodge),
        ]);
        let out = bonuses_from_catalog(
            &c,
            &[
                entry(1, 2, None),
                entry(2, 2, None),
                entry(3, 1, None),
                entry(4, 1, None),
                entry(5, 1, None),
            ],
            Some("explorer"),
            0,
        );
        assert_eq!(out.get("armor"), Some(&1.45));
        assert_eq!(out.get("shield_deflection"), Some(&1.45));
        assert_eq!(out.get("dodge"), Some(&1.45));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn realta_launch_defense_account_buffs_sum_verified_native_ranks() {
        let rows = [
            entry(2413654195, 61, None),
            entry(1037434034, 61, None),
            entry(542496768, 61, None),
            entry(291258618, 61, None),
            entry(158369823, 61, None),
            entry(109639085, 61, None),
        ];
        let out = bonuses_for_imported_buffs(&rows, Some("explorer"), 0);
        for stat in ["armor", "shield_deflection", "dodge"] {
            assert!((out[stat] - 6.5).abs() < 1e-12);
        }
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn embedded_source_never_reimports_research_building_or_forbidden_tech_ids() {
        let out = bonuses_for_imported_buffs(
            &[
                entry(434613423, 10, None),  // research flat hull
                entry(4191797822, 10, None), // research flat shield
                entry(488045092, 20, None),  // Ablative hull
                entry(3207768523, 20, None), // Ablative shield
                entry(1474824297, 48, None), // building hull
            ],
            Some("explorer"),
            0,
        );
        assert!(out.is_empty());
        let c = embedded_catalog();
        assert_eq!(c.source_group, 116);
        assert_eq!(c.buffs.len(), 285);
        let classes = [
            None,
            Some("explorer"),
            Some("interceptor"),
            Some("battleship"),
            Some("survey"),
            Some("sentinel"),
        ];
        assert_eq!(
            c.buffs
                .values()
                .filter(|spec| classes
                    .iter()
                    .any(|class| supported_stats(spec, *class).is_some()))
                .count(),
            114
        );
    }
}
