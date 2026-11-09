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

    pub(crate) fn unsupported_officer_stat_ids(
        &self,
        imported: &[GlobalBuffEntry],
        now_unix_seconds: i64,
    ) -> Vec<i64> {
        active_highest_levels(imported, now_unix_seconds)
            .into_iter()
            .filter_map(|(bid, _)| {
                let spec = self.buffs.get(&bid)?;
                (matches!(spec.modifier_code, 56..=59)
                    && supported_officer_stat_sources(spec, self.source_group == 66).is_none())
                .then_some(bid)
            })
            .collect()
    }

    /// Apex Barrier rows were added for scope auditing, not for the native
    /// setup-family replacement. Their presence must not suppress fallback
    /// contributions for unrelated stats.
    pub(crate) fn contains_setup_family_buff(&self, bid: i64) -> bool {
        self.buffs
            .get(&bid)
            .is_some_and(|spec| spec.modifier_code != 67001)
    }

    /// Source rank value for an Apex research buff, before unknown gates.
    pub(crate) fn raw_apex_rank_value(&self, bid: i64, level: i64) -> Option<f64> {
        let spec = self.buffs.get(&bid)?;
        if spec.modifier_code != 67001
            || level <= 0
            || usize::try_from(level).ok()? > spec.ranked_value_count
            || spec.ranked_values.is_empty()
        {
            return None;
        }
        let rank = usize::try_from(level - 1).ok()?;
        let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
        value.is_finite().then_some(value)
    }
}

fn embedded_catalog() -> &'static NativeConsumableBuffCatalog {
    static CATALOG: OnceLock<NativeConsumableBuffCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        NativeConsumableBuffCatalog::from_json(EMBEDDED_CATALOG)
            .expect("reviewed embedded native consumable catalog must parse")
    })
}

/// Active officer-stat sources whose native predicates or targets cannot be evaluated.
/// Ownership alone never enables these effects.
pub fn quarantined_officer_stat_warning(
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> Option<String> {
    let mut ids: Vec<i64> = [
        embedded_catalog(),
        crate::data::native_building_buffs::embedded_catalog(),
        crate::data::native_research_buffs::embedded_catalog(),
    ]
    .into_iter()
    .flat_map(|catalog| catalog.unsupported_officer_stat_ids(imported, now_unix_seconds))
    .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return None;
    }
    let examples = ids
        .iter()
        .take(4)
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{} active officer-stat buff(s) are omitted because their native conditions, timing or targets are unsupported (IDs: {examples}).",
        ids.len()
    ))
}

pub fn quarantined_officer_stat_warning_for_profile(profile_id: Option<&str>) -> Option<String> {
    use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, BUFFS_IMPORTED};
    let id = resolve_profile_id_for_api(profile_id);
    let buffs = crate::data::import::load_imported_buffs(
        &profile_path(&id, BUFFS_IMPORTED).to_string_lossy(),
    )?;
    quarantined_officer_stat_warning(&buffs, chrono::Utc::now().timestamp())
}

/// Bounded diagnostics for inventoried account combat effects that remain
/// quarantined. This deliberately excludes economy buffs and does not infer
/// missing account membership or an effect's scope from an owned buff ID.
pub fn quarantined_account_combat_warning(
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> Option<String> {
    let catalog = embedded_catalog();
    let ids: Vec<_> = active_highest_levels(imported, now_unix_seconds)
        .into_iter()
        .filter_map(|(bid, level)| {
            let spec = catalog.buffs.get(&bid)?;
            let unsupported_apex = spec.modifier_code == 67001
                && !is_exact_stella_archive_apex(spec, bid, level)
                && exact_monaveen_archive_apex_value(spec, bid, level).is_none();
            let unsupported_flat =
                bid == 4_041_662_025 && spec.modifier_code == 2 && spec.operation_code == 1;
            let unsupported_syndicate = matches!(
                bid,
                831_877_216 | 2_118_637_918 | 1_369_297_542 | 313_675_470 | 1_464_566_611
            );
            (unsupported_apex || unsupported_flat || unsupported_syndicate).then_some(bid)
        })
        .collect();
    if ids.is_empty() {
        return None;
    }
    let examples = ids
        .iter()
        .take(4)
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{} active account combat buff(s) are omitted because their native conditions, timing or flat-damage insertion order are unsupported (IDs: {examples}).",
        ids.len()
    ))
}

pub fn quarantined_account_combat_warning_for_profile(profile_id: Option<&str>) -> Option<String> {
    use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, BUFFS_IMPORTED};
    let id = resolve_profile_id_for_api(profile_id);
    let buffs = crate::data::import::load_imported_buffs(
        &profile_path(&id, BUFFS_IMPORTED).to_string_lossy(),
    )?;
    quarantined_account_combat_warning(&buffs, chrono::Utc::now().timestamp())
}

/// Combine independent native officer-stat and account-combat diagnostics while
/// reading one frozen imported-buff snapshot for this request.
pub fn quarantined_combat_warning_for_profile(profile_id: Option<&str>) -> Option<String> {
    use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, BUFFS_IMPORTED};
    let id = resolve_profile_id_for_api(profile_id);
    let buffs = crate::data::import::load_imported_buffs(
        &profile_path(&id, BUFFS_IMPORTED).to_string_lossy(),
    )?;
    let now = chrono::Utc::now().timestamp();
    let messages: Vec<_> = [
        quarantined_officer_stat_warning(&buffs, now),
        quarantined_account_combat_warning(&buffs, now),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!messages.is_empty()).then(|| messages.join(" "))
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

/// Native CombatStart all-piercing percentages for an already resolved opponent.
/// The caller must pass false for unknown opponents, players and Armada targets.
///
/// These exact predicates depend only on the fixed opponent category. There is
/// no duration or officer-controlled condition, so scenario construction can add
/// them to its in-memory accuracy/AP/SP pools before computing raw weapon stats.
/// This does not modify the saved profile or multiply completed launch stats.
pub fn combat_start_piercing_bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    opponent_is_non_armada_npc: bool,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    combat_start_piercing_bonuses_from_catalog(
        embedded_catalog(),
        imported,
        opponent_is_non_armada_npc,
        now_unix_seconds,
    )
}

fn combat_start_piercing_bonuses_from_catalog(
    catalog: &NativeConsumableBuffCatalog,
    imported: &[GlobalBuffEntry],
    opponent_is_non_armada_npc: bool,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    if catalog.schema_version != 1 || catalog.source_group != 116 || !opponent_is_non_armada_npc {
        return out;
    }
    for (bid, level) in active_highest_levels(imported, now_unix_seconds) {
        let Some(spec) = catalog.buffs.get(&bid) else {
            continue;
        };
        if spec.modifier_code != 74
            || spec.operation_code != 3
            || spec.condition_codes != [27, 131]
            || spec.num_rounds != 0
            || combat_start_condition(spec, 116, bid, None).is_none()
        {
            continue;
        }
        let rank = usize::try_from(level - 1).unwrap_or(usize::MAX);
        if rank >= spec.ranked_value_count || rank >= spec.ranked_chance_count {
            continue;
        }
        let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
        if value != 0.0 {
            for stat in ["accuracy", "armor_piercing", "shield_piercing"] {
                *out.entry(stat.to_string()).or_default() += value;
            }
        }
    }
    out
}

/// Exact source-bound account Apex effects. `owner_ship_id` is the resolved
/// canonical player hull ID, independent of which side is called the attacker
/// by the combat loop. The caller supplies ship-vs-ship PvP context explicitly.
///
/// Only the first Stella archive Apex milestone is bound here. Raw condition 17
/// has no general client predicate binding; its Eclipse scope is established by
/// this exact BuffSpec, native localization 95003 and the observed archive UI.
/// Other Apex predicates, including PvP/ship-station exclusions, stay unsupported.
pub fn combat_start_apex_seats_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    owner_ship_id: Option<&str>,
    opponent_is_player: bool,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    combat_start_apex_seats_from_catalog(
        embedded_catalog(),
        imported,
        owner_ship_id,
        opponent_is_player,
        now_unix_seconds,
    )
}

fn combat_start_apex_seats_from_catalog(
    catalog: &NativeConsumableBuffCatalog,
    imported: &[GlobalBuffEntry],
    owner_ship_id: Option<&str>,
    opponent_is_player: bool,
    now_unix_seconds: i64,
) -> Vec<CrewSeatContext> {
    if catalog.schema_version != 1
        || catalog.source_group != 116
        || owner_ship_id != Some("stella")
        || opponent_is_player
    {
        return Vec::new();
    }
    active_highest_levels(imported, now_unix_seconds)
        .into_iter()
        .filter_map(|(bid, level)| {
            let spec = catalog.buffs.get(&bid)?;
            is_exact_stella_archive_apex(spec, bid, level).then(|| CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    name: format!("native_116_{bid}"),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::ApexBarrierBonus(spec.ranked_values[0]),
                    condition: Some(AbilityCondition::And(vec![
                        AbilityCondition::DefenderHullFactionIdIs(1_750_120_904),
                        AbilityCondition::DefenderShipTypeIs(ShipType::Armada),
                    ])),
                    weapon_scope: Default::default(),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            })
        })
        .collect()
}

fn is_exact_stella_archive_apex(spec: &NativeBuffSpec, bid: i64, level: i64) -> bool {
    bid == 3_061_559_261
        && spec.sha256 == "b6e7cca49475cd554ce866576e725b9ab9f54ee15fa5ef1832f37f5b5cc09262"
        && spec.modifier_code == 67001
        && spec.operation_code == 1
        && spec.target_code == 1
        && spec.trigger_code == 25
        && spec.condition_codes == [129, 150, 17]
        && spec.attribute_fields == [8]
        && spec.faction_id == 1_750_120_904
        && spec.num_rounds == 0
        && spec.unknown_fields.is_empty()
        && spec.ranked_value_type == 0
        && spec.ranked_values == [100.0]
        && spec.ranked_value_count == 99
        && spec.ranked_chances == [1.0]
        && spec.ranked_chance_count == 99
        && level > 0
        && usize::try_from(level).is_ok_and(|rank| rank <= spec.ranked_value_count)
}

// Monaveen's twelve archive milestones, bound by native272/273 group116
// BuffSpec SHA256, group114 ConsumableSpec localization72023 and the observed
// mastered archive UI's twelve values [50,50,75,75,100,100,100,150,150,200,200,250].
// The native description declares all-ships PvP. Raw246 independently binds
// CondSelfOfficerTalNotOnBridge; raw28/137 retain their exact source context
// rather than receiving a general predicate interpretation. These are twelve
// owned sources, not a second inferred1500-point Archive level pool.
const MONAVEEN_ARCHIVE_APEX_SOURCES: [(i64, &str, f64); 12] = [
    (
        487851441,
        "5d1c6c9f5b688bfe3de638cdc8973a80270b672d2f31a5f2eba2429ca08c747d",
        150.0,
    ),
    (
        610243845,
        "c8e101130540c857151d4dc57adbec13a4ef18b8fec2ddb16211822c27dc8f5e",
        200.0,
    ),
    (
        1240186710,
        "c2e69aaef0dc6bdb191ae65c1fe4b4447543b7cd7a043f7d569541f3d8171575",
        200.0,
    ),
    (
        1721910014,
        "2676aa79ac5b7ec34a311f50fa0e1763805825d768fd3ed5eb0a2a573c146038",
        100.0,
    ),
    (
        1763915157,
        "9e34286efe49722ec9f2e4c79845bb0df407a970cde5dbdcdaae042612265b0d",
        100.0,
    ),
    (
        2199227326,
        "18b9a52108e043055db5c83d7510d505b6e2f1b206f55e5369adaaaeed2ba88d",
        50.0,
    ),
    (
        2280947091,
        "33072990613edc6dd533ab1fd3f101a669da41a6cd98048d41184740c6a5d60b",
        50.0,
    ),
    (
        2328144758,
        "2a65d71bc80beb2f4f4534cba7a75d6737b4c84678ace37e37593024ba048684",
        150.0,
    ),
    (
        2956393975,
        "20bf1e8d511f6743fa13be87bef6570d6dbbe90fa038cc2e3dee9ab28ef7e614",
        250.0,
    ),
    (
        3378417511,
        "d5d978d54a6185228be830767c510f9569e3881f0be930a86a4c6cb3724401a1",
        75.0,
    ),
    (
        3558267162,
        "99d657af3c783d9d8c5068f12f91b66317e0917789e3434b64d666495e0b3a4c",
        100.0,
    ),
    (
        3663693120,
        "a4cc4994975f92fe758bfb661ef8c2b7e0833dfdf038541540f812eae82145bc",
        75.0,
    ),
];

/// Exact Monaveen archive Apex points, kept under the existing conditional key.
/// Callers must evaluate ship-vs-ship PvP and the owning crew's Tal bridge gate;
/// this map must not become an unconditional launch bonus or a percentage.
pub fn conditional_apex_bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    conditional_apex_bonuses_from_catalog(embedded_catalog(), imported, now_unix_seconds)
}

fn conditional_apex_bonuses_from_catalog(
    catalog: &NativeConsumableBuffCatalog,
    imported: &[GlobalBuffEntry],
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    if catalog.schema_version != 1 || catalog.source_group != 116 {
        return out;
    }
    for (bid, level) in active_highest_levels(imported, now_unix_seconds) {
        let Some(spec) = catalog.buffs.get(&bid) else {
            continue;
        };
        if let Some(points) = exact_monaveen_archive_apex_value(spec, bid, level) {
            *out.entry("apex_barrier_vs_player_tal_not_on_bridge".to_string())
                .or_default() += points;
        }
    }
    out
}

fn exact_monaveen_archive_apex_value(spec: &NativeBuffSpec, bid: i64, level: i64) -> Option<f64> {
    let &(_, expected_hash, points) = MONAVEEN_ARCHIVE_APEX_SOURCES.iter().find(|r| r.0 == bid)?;
    let exact = spec.sha256 == expected_hash
        && spec.modifier_code == 67001
        && spec.operation_code == 1
        && spec.target_code == 1
        && spec.trigger_code == 25
        && spec.condition_codes == [28, 137, 246]
        && spec.attribute_fields.is_empty()
        && spec.unknown_fields.is_empty()
        && spec.num_rounds == 0
        && spec.ranked_value_type == 0
        && spec.ranked_values == [points]
        && spec.ranked_value_count == 99
        && spec.ranked_chances == [1.0]
        && spec.ranked_chance_count == 99
        && (1..=99).contains(&level);
    exact.then_some(points)
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

/// Native launch HP/defense percentages gated on the resolved owner's hull, minimum
/// grade or faction. These predicates are disjoint from the class/Syndicate
/// setup route, so a row cannot enter both pools. Active BID membership selects
/// the source once; project ownership and building levels never add another copy.
pub(crate) fn ship_gated_setup_bonuses_for_imported_buffs(
    imported: &[GlobalBuffEntry],
    ship_grade: Option<u32>,
    owner_faction: Option<&str>,
    ship_id: &str,
    ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    ship_gated_setup_bonuses_from_catalogs(
        &[
            embedded_catalog(),
            crate::data::native_building_buffs::embedded_catalog(),
            crate::data::native_research_buffs::embedded_catalog(),
        ],
        imported,
        ship_grade,
        owner_faction,
        ship_id,
        ship_class,
        now_unix_seconds,
    )
}

fn ship_gated_setup_bonuses_from_catalogs(
    catalogs: &[&NativeConsumableBuffCatalog],
    imported: &[GlobalBuffEntry],
    ship_grade: Option<u32>,
    owner_faction: Option<&str>,
    ship_id: &str,
    ship_class: Option<&str>,
    now_unix_seconds: i64,
) -> HashMap<String, f64> {
    let owner = owner_faction.map(|s| s.trim().to_ascii_lowercase());
    let mut out = HashMap::new();
    for (bid, level) in active_highest_levels(imported, now_unix_seconds) {
        let mut sources = catalogs.iter().filter_map(|catalog| {
            (catalog.schema_version == 1 && matches!(catalog.source_group, 66 | 68 | 116))
                .then(|| catalog.buffs.get(&bid))
                .flatten()
        });
        let Some(spec) = sources.next() else { continue };
        // An ambiguous source-group collision is not evidence for two bonuses.
        if sources.next().is_some() {
            continue;
        }
        let rank = usize::try_from(level - 1).unwrap_or(usize::MAX);
        if !matches!(spec.modifier_code, 11 | 12 | 13 | 60 | 61 | 73)
            || spec.operation_code != 3
            || spec.target_code != 1
            || spec.trigger_code != 24
            || spec.num_rounds != 0
            || spec.ranked_value_type != 0
            || spec.ranked_values.is_empty()
            || spec.ranked_values.len() > spec.ranked_value_count
            || rank >= spec.ranked_value_count
            || spec.ranked_values.iter().any(|v| !v.is_finite())
            || spec.ranked_chances.is_empty()
            || spec.ranked_chances.len() > spec.ranked_chance_count
            || rank >= spec.ranked_chance_count
            || spec.ranked_chances.iter().any(|v| *v != 1.0)
            || !spec.attribute_fields.is_empty()
            || !spec.unknown_fields.is_empty()
        {
            continue;
        }
        // Native BuffCondition.Values: Grade4/5 retain the minimum-grade
        // interpretation independently reviewed for Direct Hit. Owner faction
        // and class predicates are separate from the opponent's class/faction.
        let matches_scope = match spec.condition_codes.as_slice() {
            [123] => ship_grade.is_some_and(|grade| grade >= 4),
            [176] => ship_grade.is_some_and(|grade| grade >= 5),
            [124] => owner.as_deref() == Some("federation"),
            [125] => owner.as_deref() == Some("romulan"),
            [126] => owner.as_deref() == Some("klingon"),
            [240] => matches!(owner.as_deref(), Some("federation" | "klingon" | "romulan")),
            [217] => ship_id == "uss_enterprise_d",
            [class @ (0..=3), 123] => {
                ship_grade.is_some_and(|grade| grade >= 4)
                    && ship_class
                        == Some(match class {
                            0 => "explorer",
                            1 => "interceptor",
                            2 => "battleship",
                            _ => "survey",
                        })
            }
            _ => false,
        };
        if matches_scope {
            let stats: &[&str] = match spec.modifier_code {
                61 => &["hull_hp"],
                60 => &["shield_hp"],
                11 => &["dodge"],
                12 => &["armor"],
                13 => &["shield_deflection"],
                73 => &["armor", "shield_deflection", "dodge"],
                _ => unreachable!(),
            };
            let value = spec.ranked_values[rank.min(spec.ranked_values.len() - 1)];
            for stat in stats {
                *out.entry((*stat).to_owned()).or_insert(0.0) += value;
            }
        }
    }
    out
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
        let stats = if matches!(spec.modifier_code, 56..=59) {
            supported_officer_stat_sources(spec, expected_group == 66)
        } else if matches!(expected_group, 66 | 116) && matches!(spec.modifier_code, 6 | 7 | 8 | 74)
        {
            supported_launch_piercing_sources(spec, attacker_ship_class, ship_grade)
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

/// Deterministic launch multipliers on officer base stats. Research's native
/// selector binds Captain to slot 0, Bridge to slots 0/1/2, and BelowDeck to
/// every other crewed officer. Global/building inventories establish target 5.
fn supported_officer_stat_sources(
    spec: &NativeBuffSpec,
    allow_seat_scopes: bool,
) -> Option<&'static [&'static str]> {
    if spec.operation_code != 3
        || spec.trigger_code != 24
        || spec.num_rounds != 0
        || !spec.condition_codes.is_empty()
        || !spec.attribute_fields.is_empty()
        || !spec.unknown_fields.is_empty()
        || spec.ranked_value_type != 0
        || spec.ranked_values.is_empty()
        || spec.ranked_values.len() > spec.ranked_value_count
        || spec.ranked_values.iter().any(|value| !value.is_finite())
        || spec.ranked_chances.is_empty()
        || spec.ranked_chances.len() > spec.ranked_chance_count
        || spec.ranked_chances.iter().any(|chance| *chance != 1.0)
        || (spec.target_code != 5 && !allow_seat_scopes)
    {
        return None;
    }
    match (spec.target_code, spec.modifier_code) {
        (5, 56) => Some(&["officer_attack"]),
        (5, 57) => Some(&["officer_defense"]),
        (5, 58) => Some(&["officer_health"]),
        (5, 59) => Some(&["officer_attack", "officer_defense", "officer_health"]),
        (3, 56) => Some(&["officer_attack_captain"]),
        (3, 57) => Some(&["officer_defense_captain"]),
        (3, 58) => Some(&["officer_health_captain"]),
        (3, 59) => Some(&[
            "officer_attack_captain",
            "officer_defense_captain",
            "officer_health_captain",
        ]),
        (4, 56) => Some(&["officer_attack_bridge"]),
        (4, 57) => Some(&["officer_defense_bridge"]),
        (4, 58) => Some(&["officer_health_bridge"]),
        (4, 59) => Some(&[
            "officer_attack_bridge",
            "officer_defense_bridge",
            "officer_health_bridge",
        ]),
        (11, 56) => Some(&["officer_attack_below_decks"]),
        (11, 57) => Some(&["officer_defense_below_decks"]),
        (11, 58) => Some(&["officer_health_below_decks"]),
        (11, 59) => Some(&[
            "officer_attack_below_decks",
            "officer_defense_below_decks",
            "officer_health_below_decks",
        ]),
        _ => None,
    }
}

/// Reviewed research/global launch piercing families, distinct from damage-through
/// pierce and officer stats. Native source group does not change the stat's units.
fn supported_launch_piercing_sources(
    spec: &NativeBuffSpec,
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

    #[test]
    fn native_launch_hp_uses_resolved_minimum_grade_owner_and_hull_gates() {
        for (condition, grade, faction, hull, applies) in [
            (123, Some(4), None, "unknown", true),
            (123, Some(5), None, "unknown", true),
            (123, Some(3), None, "unknown", false),
            (123, None, None, "unknown", false),
            (176, Some(5), None, "unknown", true),
            (176, Some(4), None, "unknown", false),
            (124, None, Some(" FEDERATION "), "unknown", true),
            (124, None, Some("romulan"), "unknown", false),
            (125, None, Some("romulan"), "unknown", true),
            (125, None, Some("klingon"), "unknown", false),
            (126, None, Some("klingon"), "unknown", true),
            (126, None, Some("romulan"), "unknown", false),
            (240, None, Some("federation"), "unknown", true),
            (240, None, Some("klingon"), "unknown", true),
            (240, None, Some("romulan"), "unknown", true),
            (240, None, Some("independent"), "unknown", false),
            (240, None, None, "unknown", false),
            (217, None, None, "uss_enterprise_d", true),
            (217, None, None, "uss_enterprise", false),
        ] {
            let mut row = spec();
            row.condition_codes = vec![condition];
            let c = catalog(vec![(1, row)]);
            let out = ship_gated_setup_bonuses_from_catalogs(
                &[&c],
                &[entry(1, 2, None)],
                grade,
                faction,
                hull,
                None,
                100,
            );
            assert_eq!(
                out.get("hull_hp").copied(),
                applies.then_some(5.0),
                "condition {condition}"
            );
        }
    }

    #[test]
    fn native_launch_defense_preserves_axis_grade_owner_and_class_conjunctions() {
        for (modifier, expected) in [
            (11, vec!["dodge"]),
            (12, vec!["armor"]),
            (13, vec!["shield_deflection"]),
            (73, vec!["armor", "shield_deflection", "dodge"]),
        ] {
            let mut row = spec();
            row.modifier_code = modifier;
            row.condition_codes = vec![0, 123];
            let c = catalog(vec![(1, row)]);
            for (grade, class, applies) in [
                (Some(4), Some("explorer"), true),
                (Some(5), Some("explorer"), true),
                (Some(3), Some("explorer"), false),
                (None, Some("explorer"), false),
                (Some(5), Some("battleship"), false),
                (Some(5), None, false),
            ] {
                let out = ship_gated_setup_bonuses_from_catalogs(
                    &[&c],
                    &[entry(1, 2, None)],
                    grade,
                    Some("federation"),
                    "fixture",
                    class,
                    100,
                );
                assert_eq!(out.len(), if applies { expected.len() } else { 0 });
                if applies {
                    for axis in &expected {
                        assert_eq!(out[*axis], 5.0);
                    }
                }
            }
            assert!(
                bonuses_from_catalog(&c, &[entry(1, 2, None)], Some("explorer"), 100).is_empty(),
                "classANDgrade sources must not also enter the ordinary setup route"
            );
        }
        let mut fed = spec();
        fed.modifier_code = 73;
        fed.condition_codes = vec![124];
        let c = catalog(vec![(1, fed)]);
        assert_eq!(
            ship_gated_setup_bonuses_from_catalogs(
                &[&c],
                &[entry(1, 2, None)],
                Some(5),
                Some("federation"),
                "fixture",
                Some("explorer"),
                100
            )
            .len(),
            3
        );
        assert!(ship_gated_setup_bonuses_from_catalogs(
            &[&c],
            &[entry(1, 2, None)],
            Some(5),
            Some("klingon"),
            "fixture",
            Some("explorer"),
            100
        )
        .is_empty());
    }

    #[test]
    fn native_launch_defense_real_grade_and_federation_rows_retain_source_fractions() {
        let rows = [
            entry(1_019_365_535, 6, None),
            entry(3_249_974_457, 10, None),
            entry(2_880_082_268, 10, None),
            entry(1_728_837_131, 10, None),
            entry(2_580_220_592, 10, None),
            entry(1_024_053_940, 10, None),
        ];
        let out = ship_gated_setup_bonuses_for_imported_buffs(
            &rows,
            Some(5),
            Some("federation"),
            "uss_enterprise_d",
            Some("explorer"),
            100,
        );
        assert_eq!(out.len(), 3);
        assert!((out["armor"] - 3.65).abs() < 1e-12);
        assert!((out["dodge"] - 3.65).abs() < 1e-12);
        assert!((out["shield_deflection"] - 6.55).abs() < 1e-12);
    }

    #[test]
    fn native_launch_hp_deduplicates_active_ids_and_preserves_other_setup_families() {
        let ordinary = spec();
        let mut gated = spec();
        gated.modifier_code = 60;
        gated.condition_codes = vec![123];
        let c = catalog(vec![(1, ordinary), (2, gated)]);
        let imported = [
            entry(1, 1, None),
            entry(2, 1, None),
            entry(2, 2, None),
            entry(2, 3, Some(99)),
        ];
        let ordinary = bonuses_from_catalog(&c, &imported, Some("explorer"), 100);
        assert_eq!(ordinary.get("hull_hp"), Some(&0.5));
        assert!(!ordinary.contains_key("shield_hp"));
        let gated = ship_gated_setup_bonuses_from_catalogs(
            &[&c],
            &imported,
            Some(5),
            Some("federation"),
            "uss_enterprise_d",
            None,
            100,
        );
        assert_eq!(gated.len(), 1);
        assert_eq!(gated.get("shield_hp"), Some(&5.0));
        // A collision between catalog source groups is ambiguous, not additive.
        assert!(ship_gated_setup_bonuses_from_catalogs(
            &[&c, &c],
            &imported,
            Some(5),
            Some("federation"),
            "uss_enterprise_d",
            None,
            100,
        )
        .is_empty());
    }

    #[test]
    fn native_launch_hp_keeps_unknown_timing_attributes_and_ranks_quarantined() {
        for variant in 0..9 {
            let mut row = spec();
            row.condition_codes = vec![123];
            match variant {
                0 => row.trigger_code = 25,
                1 => row.condition_codes.push(999),
                2 => row.target_code = 6,
                3 => row.operation_code = 5,
                4 => row.attribute_fields.push(8),
                5 => row.unknown_fields.push(99),
                6 => row.num_rounds = 1,
                7 => row.ranked_chances = vec![0.5],
                _ => row.ranked_value_count = 1,
            }
            let c = catalog(vec![(1, row)]);
            assert!(
                ship_gated_setup_bonuses_from_catalogs(
                    &[&c],
                    &[entry(1, 2, None)],
                    Some(5),
                    Some("federation"),
                    "uss_enterprise_d",
                    None,
                    100,
                )
                .is_empty(),
                "variant {variant}"
            );
        }
    }

    #[test]
    fn combat_start_piercing_is_a_gated_additive_three_axis_pool() {
        let imported = [
            entry(1_404_701_608, 1, None),
            entry(1_404_701_608, 61, None),
        ];
        let out = combat_start_piercing_bonuses_for_imported_buffs(&imported, true, 100);
        assert_eq!(out.len(), 3);
        for key in ["accuracy", "armor_piercing", "shield_piercing"] {
            assert!((out[key] - 0.1).abs() < 1e-12);
        }
        assert!(!out.contains_key("pierce"));
        assert!(
            combat_start_piercing_bonuses_for_imported_buffs(&imported, false, 100).is_empty(),
            "players, Armadas and unresolved opponents fail the setup gate"
        );
        for class in ["explorer", "interceptor", "battleship", "survey"] {
            let mut profile = crate::data::profile::PlayerProfile::default();
            profile.bonuses.extend(bonuses_for_imported_buffs(
                &[entry(1_333_781_883, 61, None)],
                Some(class),
                100,
            ));
            for (key, value) in &out {
                *profile.bonuses.entry(key.clone()).or_default() += value;
            }
            let mut raw = crate::combat::AttackerStats {
                accuracy: 100.0,
                armor_piercing: 200.0,
                shield_piercing: 300.0,
            };
            crate::data::profile::apply_profile_piercing_to_attacker_stats(
                &mut raw, &profile, None,
            );
            assert!((raw.accuracy - 150.0).abs() < 1e-12);
            assert!((raw.armor_piercing - 300.0).abs() < 1e-12);
            assert!((raw.shield_piercing - 450.0).abs() < 1e-12);
        }
        assert!(
            bonuses_for_imported_buffs(&imported, Some("explorer"), 100).is_empty(),
            "conditional CombatStart sources never become unconditional launch buffs"
        );
    }

    #[test]
    fn combat_start_piercing_preserves_rank_expiry_and_all_nine_verified_rows() {
        let mut row = spec();
        row.modifier_code = 74;
        row.trigger_code = 25;
        row.condition_codes = vec![27, 131];
        row.ranked_values = vec![0.1, 0.2, 0.0];
        let c = catalog(vec![(1, row)]);
        let out = combat_start_piercing_bonuses_from_catalog(
            &c,
            &[entry(1, 1, None), entry(1, 2, None), entry(1, 3, Some(100))],
            true,
            100,
        );
        assert_eq!(out["accuracy"], 0.2);
        for imported in [
            vec![entry(1, 0, None)],
            vec![entry(1, 4, None)],
            vec![entry(1, 2, Some(100))],
        ] {
            assert!(
                combat_start_piercing_bonuses_from_catalog(&c, &imported, true, 100).is_empty()
            );
        }
        let rows: Vec<_> = [
            1277497908, 1404701608, 1610041024, 2363932725, 2886123067, 3504434200, 3714771770,
            3885426739, 3888142903,
        ]
        .into_iter()
        .map(|id| entry(id, 61, None))
        .collect();
        let total = combat_start_piercing_bonuses_for_imported_buffs(&rows, true, 100);
        for axis in ["accuracy", "armor_piercing", "shield_piercing"] {
            assert!((total[axis] - 3.0).abs() < 1e-12);
        }
    }

    #[test]
    fn combat_start_piercing_rejects_changed_native_gates_and_units() {
        let mut base = spec();
        base.modifier_code = 74;
        base.trigger_code = 25;
        base.condition_codes = vec![27, 131];
        let mut mutants = vec![base.clone(); 14];
        mutants[0].modifier_code = 7;
        mutants[1].operation_code = 1;
        mutants[2].trigger_code = 24;
        mutants[3].target_code = 5;
        mutants[4].num_rounds = 1;
        mutants[5].ranked_chances = vec![0.5];
        mutants[6].attribute_fields = vec![8];
        mutants[7].unknown_fields = vec![99];
        mutants[8].condition_codes = vec![27];
        mutants[9].condition_codes = vec![27, 131, 246];
        mutants[10].ranked_value_type = 1;
        mutants[11].ranked_values = vec![f64::INFINITY];
        mutants[12].ranked_chances.clear();
        mutants[13].ranked_chance_count = 0;
        for row in mutants {
            let c = catalog(vec![(1, row)]);
            assert!(combat_start_piercing_bonuses_from_catalog(
                &c,
                &[entry(1, 1, None)],
                true,
                100
            )
            .is_empty());
        }
    }

    #[test]
    fn exact_stella_archive_apex_uses_owner_hull_and_eclipse_armada_scope() {
        let imported = [
            entry(3_061_559_261, 1, None),
            entry(3_061_559_261, 99, None),
        ];
        let seats =
            combat_start_apex_seats_for_imported_buffs(&imported, Some("stella"), false, 100);
        assert_eq!(seats.len(), 1, "one active BID contributes once");
        assert_eq!(
            seats[0].ability.effect,
            AbilityEffect::ApexBarrierBonus(100.0)
        );
        assert_eq!(seats[0].ability.timing, TimingWindow::CombatBegin);
        assert!(!seats[0].ability.boostable);
        assert!(!seats[0].boosted);
        let condition = seats[0].ability.condition.as_ref().unwrap();
        let mut ctx = combat_context();
        ctx.defender_hull_faction_id = 1_750_120_904;
        ctx.defender_ship_type = ShipType::Armada;
        assert!(condition.evaluate(&ctx));
        // Owner scope was evaluated by the compiler, not by attacker-relative runtime context.
        ctx.attacker_ship_id = Arc::from("different_combat_loop_role");
        assert!(condition.evaluate(&ctx));
        ctx.defender_ship_type = ShipType::Battleship;
        assert!(!condition.evaluate(&ctx));
        ctx.defender_ship_type = ShipType::Armada;
        ctx.defender_hull_faction_id = 99;
        assert!(!condition.evaluate(&ctx));
        for owner in [None, Some("realta"), Some("monaveen")] {
            assert!(
                combat_start_apex_seats_for_imported_buffs(&imported, owner, false, 100).is_empty()
            );
        }
        assert!(
            combat_start_apex_seats_for_imported_buffs(&imported, Some("stella"), true, 100)
                .is_empty()
        );
        for level in [0, -1, 100] {
            assert!(combat_start_apex_seats_for_imported_buffs(
                &[entry(3_061_559_261, level, None)],
                Some("stella"),
                false,
                100
            )
            .is_empty());
        }
        assert!(combat_start_apex_seats_for_imported_buffs(
            &[entry(3_061_559_261, 1, Some(100))],
            Some("stella"),
            false,
            100
        )
        .is_empty());
        assert!(
            bonuses_for_imported_buffs(&imported, Some("battleship"), 100).is_empty(),
            "Apex must not also enter launch pools"
        );
    }

    #[test]
    fn exact_stella_archive_apex_rejects_changed_predicates_hash_rank_or_units() {
        let base = embedded_catalog().buffs[&3_061_559_261].clone();
        let mut mutants = vec![base.clone(); 14];
        mutants[0].sha256 = "unverified".into();
        mutants[1].condition_codes = vec![129, 150];
        mutants[2].faction_id = 99;
        mutants[3].attribute_fields = vec![];
        mutants[4].num_rounds = 1;
        mutants[5].unknown_fields = vec![99];
        mutants[6].operation_code = 3;
        mutants[7].target_code = 2;
        mutants[8].trigger_code = 24;
        mutants[9].ranked_value_type = 1;
        mutants[10].ranked_chances = vec![0.5];
        mutants[11].ranked_values = vec![0.1];
        mutants[12].ranked_value_count = 98;
        mutants[13].ranked_chance_count = 98;
        for row in mutants {
            let c = catalog(vec![(3_061_559_261, row)]);
            assert!(combat_start_apex_seats_from_catalog(
                &c,
                &[entry(3_061_559_261, 1, None)],
                Some("stella"),
                false,
                100
            )
            .is_empty());
        }
        let c = catalog(vec![(999, base)]);
        assert!(combat_start_apex_seats_from_catalog(
            &c,
            &[entry(999, 1, None)],
            Some("stella"),
            false,
            100
        )
        .is_empty());
        for bid in [1_369_297_542, 143_261_594, 313_675_470, 1_434_397_842] {
            assert!(
                combat_start_apex_seats_for_imported_buffs(
                    &[entry(bid, 1, None)],
                    Some("stella"),
                    false,
                    100
                )
                .is_empty(),
                "other source identities do not inherit this exact binding"
            );
        }
    }

    #[test]
    fn global_launch_piercing_maps_fractional_axes_without_damage_through_pierce() {
        let rows = [(6, 0.1), (7, 0.2), (8, 0.3), (74, 0.5)]
            .into_iter()
            .map(|(code, value)| {
                let mut row = spec();
                row.modifier_code = code;
                row.ranked_values = vec![value];
                (code, row)
            })
            .collect();
        let c = catalog(rows);
        let imported: Vec<_> = [6, 7, 8, 74]
            .into_iter()
            .map(|id| entry(id, 1, None))
            .collect();
        let out = bonuses_from_catalog(&c, &imported, None, 100);
        assert_eq!(out.len(), 3);
        assert!((out["accuracy"] - 0.6).abs() < 1e-12);
        assert!((out["armor_piercing"] - 0.7).abs() < 1e-12);
        assert!((out["shield_piercing"] - 0.8).abs() < 1e-12);
        assert!(!out.contains_key("pierce"));
    }

    #[test]
    fn global_launch_piercing_preserves_class_grade_rank_and_expiry_gates() {
        let mut row = spec();
        row.modifier_code = 74;
        row.condition_codes = vec![0];
        let mut graded = row.clone();
        graded.condition_codes = vec![123];
        let c = catalog(vec![(1, row), (2, graded)]);
        let imported = [entry(1, 1, None), entry(1, 2, None), entry(1, 3, Some(100))];
        for class in [None, Some("battleship"), Some("unknown")] {
            assert!(bonuses_from_catalog(&c, &imported, class, 100).is_empty());
        }
        let out = bonuses_from_catalog(&c, &imported, Some("explorer"), 100);
        assert_eq!(
            out["accuracy"], 5.0,
            "highest live one-based rank is applied once"
        );
        assert!(
            bonuses_from_catalog(&c, &[entry(1, 2, Some(100))], Some("explorer"), 100).is_empty()
        );
        for grade in [None, Some(3)] {
            assert!(bonuses_from_catalog_for_ship(
                &c,
                116,
                &[entry(2, 2, None)],
                None,
                None,
                grade,
                100
            )
            .is_empty());
        }
        let out =
            bonuses_from_catalog_for_ship(&c, 116, &[entry(2, 2, None)], None, None, Some(4), 100);
        assert_eq!(out["shield_piercing"], 5.0);
        assert!(
            bonuses_from_catalog(&c, &[entry(2, 2, None)], None, 100).is_empty(),
            "unknown grade cannot turn a minimum-grade source into an unconditional buff"
        );
    }

    #[test]
    fn global_launch_piercing_rejects_combat_start_and_unverified_native_facts() {
        let mut base = spec();
        base.modifier_code = 74;
        let mut rows = vec![base.clone(); 9];
        rows[0].operation_code = 1;
        rows[1].trigger_code = 25;
        rows[2].target_code = 5;
        rows[3].num_rounds = 2;
        rows[4].condition_codes = vec![27, 131];
        rows[5].condition_codes = vec![999];
        rows[6].attribute_fields = vec![8];
        rows[7].unknown_fields = vec![99];
        rows[8].ranked_chances = vec![0.5];
        let c = catalog(
            rows.into_iter()
                .enumerate()
                .map(|(i, row)| (i as i64 + 1, row))
                .collect(),
        );
        let imported: Vec<_> = (1..=9).map(|id| entry(id, 1, None)).collect();
        assert!(bonuses_from_catalog(&c, &imported, Some("explorer"), 100).is_empty());
    }

    #[test]
    fn embedded_global_piercing_keeps_six_launch_sources_and_nine_combat_start_rows_separate() {
        let launch = [
            609129565, 675636702, 1085339663, 1333781883, 2259348690, 3809461333,
        ];
        let imported: Vec<_> = launch.into_iter().map(|id| entry(id, 61, None)).collect();
        let out = bonuses_for_imported_buffs(&imported, Some("explorer"), 100);
        for axis in ["accuracy", "armor_piercing", "shield_piercing"] {
            assert!((out[axis] - 6.4).abs() < 1e-12);
        }
        assert_eq!(out.len(), 3);
        let combat_start = [
            1277497908, 1404701608, 1610041024, 2363932725, 2886123067, 3504434200, 3714771770,
            3885426739, 3888142903,
        ];
        let imported: Vec<_> = combat_start
            .into_iter()
            .map(|id| entry(id, 1, None))
            .collect();
        assert!(bonuses_for_imported_buffs(&imported, Some("explorer"), 100).is_empty());
    }

    #[test]
    fn officer_axis_and_all_stats_compose_across_native_source_groups() {
        for group in [66, 68, 116] {
            let mut defense = spec();
            defense.modifier_code = 57;
            defense.target_code = 5;
            defense.ranked_values = vec![0.25, 0.5];
            let mut all = defense.clone();
            all.modifier_code = 59;
            all.ranked_values = vec![0.1, 0.2];
            let mut c = catalog(vec![(101, defense), (102, all)]);
            c.source_group = group;
            let result = bonuses_from_catalog_for_group(
                &c,
                group,
                &[
                    entry(101, 1, None),
                    entry(101, 2, None),
                    entry(102, 2, None),
                ],
                None,
                None,
                100,
            );
            assert_eq!(result.len(), 3);
            assert!((result["officer_defense"] - 0.7).abs() < 1e-12);
            assert!((result["officer_attack"] - 0.2).abs() < 1e-12);
            assert!((result["officer_health"] - 0.2).abs() < 1e-12);
            assert!(!result.contains_key("armor"));
            assert!(!result.contains_key("pierce"));
        }
    }

    #[test]
    fn research_officer_seat_sources_keep_their_native_scope() {
        let mut rows = Vec::new();
        for target in [3, 4, 11] {
            let mut row = spec();
            row.modifier_code = 59;
            row.target_code = target;
            rows.push((i64::from(target), row));
        }
        let mut c = catalog(rows);
        c.source_group = 66;
        let active = [entry(3, 1, None), entry(4, 1, None), entry(11, 1, None)];
        let result = bonuses_from_catalog_for_group(&c, 66, &active, None, None, 100);
        assert_eq!(result.len(), 9);
        for scope in ["captain", "bridge", "below_decks"] {
            for axis in ["attack", "defense", "health"] {
                assert_eq!(result[&format!("officer_{axis}_{scope}")], 0.5);
            }
        }
        assert!(!result.contains_key("officer_defense"));
        // The building/global audit establishes only crew-wide officer targets.
        for group in [68, 116] {
            c.source_group = group;
            assert!(bonuses_from_catalog_for_group(&c, group, &active, None, None, 100).is_empty());
            assert_eq!(c.unsupported_officer_stat_ids(&active, 100), vec![3, 4, 11]);
        }
    }

    #[test]
    fn officer_stat_unknown_gates_and_expired_sources_are_not_applied() {
        let mut base = spec();
        base.modifier_code = 57;
        base.target_code = 5;
        let mut mutants = vec![base.clone(); 8];
        mutants[0].operation_code = 5;
        mutants[1].trigger_code = 25;
        mutants[2].num_rounds = 2;
        mutants[3].condition_codes = vec![231];
        mutants[4].attribute_fields = vec![9];
        mutants[5].unknown_fields = vec![99];
        mutants[6].ranked_chances = vec![0.5];
        mutants[7].target_code = 7;
        let mut rows = vec![(100, base)];
        rows.extend(
            mutants
                .into_iter()
                .enumerate()
                .map(|(i, row)| (i as i64 + 1, row)),
        );
        let c = catalog(rows);
        let mut active: Vec<_> = (1..=8).map(|bid| entry(bid, 1, None)).collect();
        active.push(entry(100, 1, Some(100)));
        assert!(bonuses_from_catalog(&c, &active, None, 100).is_empty());
        assert_eq!(
            c.unsupported_officer_stat_ids(&active, 100),
            (1..=8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn newly_inventoried_real_sources_preserve_rank_values_and_units() {
        let global = bonuses_for_imported_buffs(&[entry(476178618, 3, None)], None, 100);
        assert_eq!(global["officer_defense"], 0.15);
        let building = crate::data::native_building_buffs::bonuses_for_imported_buffs(
            &[entry(334143899, 2, None)],
            None,
            None,
            100,
        );
        for axis in ["attack", "defense", "health"] {
            assert_eq!(building[&format!("officer_{axis}")], 0.02);
        }
        let research = crate::data::native_research_buffs::research_bonuses_for_imported_buffs(
            &[entry(104229770, 1, None), entry(304385043, 2, None)],
            None,
            100,
        )
        .unwrap();
        assert_eq!(research["officer_attack"], 0.4);
        assert_eq!(research["officer_defense"], 0.19);
    }

    #[test]
    fn active_quarantined_officer_sources_are_reported_and_expired_ones_are_not() {
        let active = [entry(3988908241, 1, None), entry(4178575394, 1, None)];
        let warning = quarantined_officer_stat_warning(&active, 100).unwrap();
        assert!(warning.starts_with("2 active officer-stat buff(s)"));
        assert!(warning.contains("3988908241, 4178575394"));
        assert!(
            quarantined_officer_stat_warning(&[entry(3988908241, 1, Some(100))], 100).is_none()
        );
        assert!(quarantined_officer_stat_warning(&[entry(476178618, 3, None)], 100).is_none());
    }

    #[test]
    fn account_combat_warning_is_bounded_deduplicated_and_excludes_supported_effects() {
        let active = [
            entry(831_877_216, 1, None),
            entry(831_877_216, 61, None),
            entry(2_118_637_918, 61, None),
            entry(1_369_297_542, 61, None),
            entry(313_675_470, 61, None),
            entry(1_464_566_611, 61, None),
            entry(4_041_662_025, 61, None),
            entry(3_061_559_261, 99, None),
        ];
        let warning = quarantined_account_combat_warning(&active, 100).unwrap();
        assert!(warning.starts_with("6 active account combat buff(s)"));
        assert!(warning.contains("313675470, 831877216, 1369297542, 1464566611"));
        assert!(!warning.contains("2118637918"), "show at most four IDs");
        assert!(
            !warning.contains("3061559261"),
            "exact supported Stella source"
        );
        assert!(warning.contains("flat-damage insertion order"));
        for imported in [
            vec![entry(831_877_216, 1, Some(100))],
            vec![entry(831_877_216, 0, None)],
            vec![entry(3_061_559_261, 99, None)],
            vec![entry(1_404_701_608, 61, None)],
            vec![entry(1_333_781_883, 61, None)],
            vec![entry(476_178_618, 3, None)],
            vec![entry(1, 1, None)],
        ] {
            assert!(quarantined_account_combat_warning(&imported, 100).is_none());
        }
        assert!(
            quarantined_account_combat_warning(&[entry(1_157_216_071, 61, None)], 100).is_some(),
            "other Apex sources remain quarantined"
        );
        assert!(
            quarantined_account_combat_warning(&[entry(3_061_559_261, 100, None)], 100).is_some(),
            "a rank beyond the exact supported source remains unsupported"
        );
    }

    #[test]
    fn monaveen_archive_apex_sums_owned_milestones_as_conditional_raw_points() {
        const KEY: &str = "apex_barrier_vs_player_tal_not_on_bridge";
        let all: Vec<_> = MONAVEEN_ARCHIVE_APEX_SOURCES
            .iter()
            .map(|&(id, _, _)| entry(id, 61, None))
            .collect();
        let total = conditional_apex_bonuses_for_imported_buffs(&all, 100);
        assert_eq!(total.len(), 1);
        assert_eq!(total[KEY], 1500.0);
        assert!(quarantined_account_combat_warning(&all, 100).is_none());
        assert!(
            bonuses_for_imported_buffs(&all, Some("explorer"), 100).is_empty(),
            "conditional Apex never enters unconditional launch pools"
        );
        let partial = [
            entry(2_199_227_326, 1, None),
            entry(2_956_393_975, 99, None),
        ];
        assert_eq!(
            conditional_apex_bonuses_for_imported_buffs(&partial, 100)[KEY],
            300.0
        );
        let duplicated = [
            entry(2_199_227_326, 1, None),
            entry(2_199_227_326, 61, None),
            entry(2_199_227_326, 99, Some(100)),
            entry(2_956_393_975, 99, Some(100)),
        ];
        assert_eq!(
            conditional_apex_bonuses_for_imported_buffs(&duplicated, 100)[KEY],
            50.0
        );
        for rank in [-1, 0, 100] {
            assert!(conditional_apex_bonuses_for_imported_buffs(
                &[entry(2_199_227_326, rank, None)],
                100
            )
            .is_empty());
        }
        for id in [
            1_369_297_542,
            313_675_470,
            143_261_594,
            249_883_926,
            3_061_559_261,
        ] {
            assert!(
                conditional_apex_bonuses_for_imported_buffs(&[entry(id, 61, None)], 100).is_empty(),
                "Syndicate, consumables and other archive families remain separate"
            );
        }
    }

    #[test]
    fn monaveen_archive_apex_rejects_changed_hash_units_or_predicates() {
        let bid = 2_199_227_326;
        let base = embedded_catalog().buffs[&bid].clone();
        let mut mutants = vec![base.clone(); 15];
        mutants[0].sha256 = "unverified".into();
        mutants[1].modifier_code = 73;
        mutants[2].operation_code = 3;
        mutants[3].target_code = 5;
        mutants[4].trigger_code = 24;
        mutants[5].condition_codes = vec![28, 137];
        mutants[6].condition_codes = vec![28, 137, 246, 17];
        mutants[7].attribute_fields = vec![8];
        mutants[8].unknown_fields = vec![99];
        mutants[9].num_rounds = 1;
        mutants[10].ranked_value_type = 1;
        mutants[11].ranked_values = vec![0.5];
        mutants[12].ranked_value_count = 100;
        mutants[13].ranked_chances = vec![0.5];
        mutants[14].ranked_chance_count = 100;
        for row in mutants {
            let c = catalog(vec![(bid, row)]);
            assert!(
                conditional_apex_bonuses_from_catalog(&c, &[entry(bid, 61, None)], 100).is_empty()
            );
        }
        let unknown = catalog(vec![(1, base)]);
        assert!(
            conditional_apex_bonuses_from_catalog(&unknown, &[entry(1, 61, None)], 100).is_empty(),
            "matching-looking unknown IDs cannot inherit exact source support"
        );
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
        // Original 285 definitions plus 56 newly inventoried officer-stat sources.
        // The original 341 definitions are unchanged; the same frozen cache adds
        // fifteen ModAllPiercing74 rows and thirty-three inventoried Apex rows.
        // Apex inventory does not imply generic eligibility: only the exact
        // source-bound first Stella milestone has an admitted conditional seat.
        assert_eq!(c.buffs.len(), 389);
        assert_eq!(
            c.buffs
                .values()
                .filter(|spec| matches!(spec.modifier_code, 56..=59))
                .count(),
            56
        );
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
