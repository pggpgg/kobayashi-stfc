//! Player profile: effective_bonuses applied as pre-combat modifier layer (DESIGN §5).
//! Keys match engine/LCARS stats: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce,
//! accuracy (scales ship AttackerStats for dodge mitigation), apex_shred, apex_barrier, isolytic_*, etc.
//! Bonuses from equipped forbidden/chaos tech (by fid) are merged in when [merge_forbidden_tech_bonuses_into_profile] is used.
//! Each ship has at most one forbidden slot and one chaos slot; see `equipped_forbidden_fid` / `equipped_chaos_fid`.
//! Synced `forbidden_tech.imported.json` supplies tier/level for optional scaling; combat uses only equipped fids.
//! **Borg Alcove** ([`BORG_ALCOVE_FORBIDDEN_TECH_FID`]) is an exception: Voyager/NPC-gated combat stats use
//! [`forbidden_tech_derived_attack_phase_seats`] and [`borg_alcove_hull_hp_bonus_fraction`] instead of flat `bonuses`.
//! **Borg Operating Table** ([`BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID`]): Conqueror Borg–gated combat stats use
//! [`borg_operating_table_forbidden_tech_seats`] (not flat `bonuses`). Warp-speed rows remain out of combat scope.
//! **Quantum Slipstream Drive** ([`QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID`]): opponent cumulative shield-mitigation
//! debuff is [`AbilityEffect::CumulativeOpponentShieldMitigationDebuff`] via
//! [`quantum_slipstream_forbidden_tech_round_start_seats`]; catalog `shield_mitigation` is a **cap** source only
//! (skipped in [`merge_tech_fids_into_profile`] / [`merge_tech_fids_into_profile_with_level_tier`]).
//! **Ship-class + hostile-gated torpedo family** (S31 Battleship, Control Seeker Probes Explorer, Dual Photon
//! Warheads Interceptor): combat stats are not merged as unconditional [`PlayerProfile::bonuses`]; see
//! [`ship_class_gated_torpedo_family_derived_seats`], [`ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship`],
//! and scenario-side hostile shield / accuracy patches.
//! Bonuses from synced buildings (by bid) are merged in when [merge_building_bonuses_into_profile] is used.
//! Foundry / Science Lab / Engine Technology Lab `shield_hp` are gated by player hull class (BB /
//! Explorer / Interceptor) via building `conditions` and
//! [`crate::data::building::BuildingBonusContext::attacker_ship_type`] at merge time (not global).
//! Bonuses from synced research (by rid) are merged in when [merge_research_bonuses_into_profile] is used.
//! **Titan-A Fortify:** several Titan research `rid`s apply combat catalog stats only while the alliance
//! Fortify support buff is selected (`titan_a_fortification` / `titan_a_max_fortification`); see
//! [`TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS`] and [`merge_research_bonuses_into_profile`].

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, AttackerStats, Combatant, CrewSeat,
    CrewSeatContext, ShipType, TimingWindow, EPSILON, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::building::{self, BuildingBonusContext, BuildingIndex};
use crate::data::forbidden_chaos::{ForbiddenChaosList, ForbiddenChaosRecord};
use crate::data::import::{BuildingEntry, ForbiddenTechEntry, ResearchEntry};
use crate::data::research::{
    cumulative_research_bonuses, cumulative_research_owner_faction_bonuses, ResearchCatalog,
};
use crate::data::ship::ShipRecord;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: HashMap<String, f64>,
    /// Research catalog bonuses gated on **player ship** owner `faction` slug (e.g. `federation`).
    /// Outer key: lowercase trimmed slug; inner: combat stat → cumulative value (same merge as `bonuses`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub research_owner_faction_bonuses: HashMap<String, HashMap<String, f64>>,
    /// Selected alliance / ship support buff ids persisted with the profile UI state.
    /// Combat request handling still resolves ids against the support buff catalog per scenario.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support_buffs: Vec<String>,
    /// Optional Operations Center level override. When set, building bonus context uses this
    /// instead of inferring from synced buildings (ops_center level). Lets you simulate without sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops_level: Option<u32>,
    /// Legacy list overrides (ignored for combat resolution; use equipped slots). Kept for
    /// backward-compatible JSON round-trips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbidden_tech_override: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos_tech_override: Option<Vec<i64>>,
    /// STFC: one forbidden-tech slot on the ship. Omitted or unset = empty (no forbidden tech bonuses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped_forbidden_fid: Option<i64>,
    /// STFC: one chaos-tech slot. Omitted or unset = empty (no chaos tech bonuses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped_chaos_fid: Option<i64>,
}

/// Catalog row classifies as forbidden lane (includes legacy empty `tech_type`).
#[inline]
pub fn forbidden_chaos_record_is_forbidden_lane(r: &ForbiddenChaosRecord) -> bool {
    r.tech_type.is_empty() || r.tech_type.eq_ignore_ascii_case("forbidden")
}

/// Catalog row classifies as chaos lane.
#[inline]
pub fn forbidden_chaos_record_is_chaos_lane(r: &ForbiddenChaosRecord) -> bool {
    r.tech_type.eq_ignore_ascii_case("chaos")
}

pub const DEFAULT_PROFILE_PATH: &str = "data/profile.json";

fn validate_positive_unique_ids(issues: &mut Vec<String>, field: &str, ids: Option<&[i64]>) {
    let Some(ids) = ids else { return };
    let mut seen = HashSet::new();
    for &id in ids {
        if id <= 0 {
            issues.push(format!("{field} contains non-positive id {id}"));
        }
        if !seen.insert(id) {
            issues.push(format!("{field} contains duplicate id {id}"));
        }
    }
}

fn validate_and_canonicalize_support_buff_ids(
    issues: &mut Vec<String>,
    ids: &[String],
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut canonical = Vec::new();
    for raw_id in ids {
        let id = raw_id.trim();
        if id.is_empty() {
            issues.push("support_buffs contains an empty id".to_string());
            continue;
        }
        if id != raw_id {
            issues.push(format!(
                "support_buffs id `{raw_id}` must not contain surrounding whitespace"
            ));
            continue;
        }
        if seen.insert(id.to_string()) {
            canonical.push(id.to_string());
        }
    }
    canonical
}

/// Validate and canonicalize a profile payload before persisting it.
///
/// Profile bonus keys are the combat stat names consumed by [`apply_profile_to_attacker`] and related
/// scenario helpers. Aliases such as `armor_pierce` and `shield_pierce` are folded into `pierce` so
/// accepted payloads cannot be silently ignored later.
/// When `forbidden_catalog` is `Some`, equipped fids are checked against the catalog `tech_type` lane.
pub fn validate_player_profile_payload(
    profile: PlayerProfile,
    forbidden_catalog: Option<&ForbiddenChaosList>,
) -> Result<PlayerProfile, Vec<String>> {
    let mut issues = Vec::new();
    let mut canonical_bonuses: HashMap<String, f64> = HashMap::new();
    let support_buffs =
        validate_and_canonicalize_support_buff_ids(&mut issues, &profile.support_buffs);

    for (raw_key, value) in &profile.bonuses {
        let key = raw_key.trim();
        if key.is_empty() {
            issues.push("bonuses contains an empty stat key".to_string());
            continue;
        }
        if key != raw_key {
            issues.push(format!(
                "bonus key `{raw_key}` must not contain surrounding whitespace"
            ));
        }
        if !value.is_finite() {
            issues.push(format!("bonus `{key}` value must be finite"));
            continue;
        }
        let Some(canonical_key) = normalize_profile_combat_stat(key) else {
            issues.push(format!("bonus `{key}` is not a supported combat stat"));
            continue;
        };
        canonical_bonuses
            .entry(canonical_key.to_string())
            .and_modify(|existing| *existing += *value)
            .or_insert(*value);
    }

    if profile.ops_level == Some(0) {
        issues.push("ops_level must be greater than 0 when set".to_string());
    }
    validate_positive_unique_ids(
        &mut issues,
        "forbidden_tech_override",
        profile.forbidden_tech_override.as_deref(),
    );
    validate_positive_unique_ids(
        &mut issues,
        "chaos_tech_override",
        profile.chaos_tech_override.as_deref(),
    );

    if let Some(fid) = profile.equipped_forbidden_fid {
        if fid <= 0 {
            issues.push("equipped_forbidden_fid must be positive when set".to_string());
        }
    }
    if let Some(fid) = profile.equipped_chaos_fid {
        if fid <= 0 {
            issues.push("equipped_chaos_fid must be positive when set".to_string());
        }
    }
    if let (Some(a), Some(b)) = (profile.equipped_forbidden_fid, profile.equipped_chaos_fid) {
        if a > 0 && b > 0 && a == b {
            issues.push(
                "equipped_forbidden_fid and equipped_chaos_fid must not be the same fid"
                    .to_string(),
            );
        }
    }

    if let Some(cat) = forbidden_catalog {
        let by_fid: HashMap<i64, &ForbiddenChaosRecord> = cat
            .items
            .iter()
            .filter_map(|r| r.fid.map(|id| (id, r)))
            .collect();
        if let Some(fid) = profile.equipped_forbidden_fid {
            if fid > 0 {
                match by_fid.get(&fid) {
                    None => issues.push(format!(
                        "equipped_forbidden_fid {fid} is not present in the forbidden/chaos catalog"
                    )),
                    Some(rec) => {
                        if !forbidden_chaos_record_is_forbidden_lane(rec) {
                            issues.push(format!(
                                "equipped_forbidden_fid {fid} is not a forbidden-tech catalog row (tech_type={})",
                                rec.tech_type
                            ));
                        }
                    }
                }
            }
        }
        if let Some(fid) = profile.equipped_chaos_fid {
            if fid > 0 {
                match by_fid.get(&fid) {
                    None => issues.push(format!(
                        "equipped_chaos_fid {fid} is not present in the forbidden/chaos catalog"
                    )),
                    Some(rec) => {
                        if !forbidden_chaos_record_is_chaos_lane(rec) {
                            issues.push(format!(
                                "equipped_chaos_fid {fid} is not a chaos-tech catalog row (tech_type={})",
                                rec.tech_type
                            ));
                        }
                    }
                }
            }
        }
    }

    let mut canonical_owner: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (fac_raw, inner) in &profile.research_owner_faction_bonuses {
        let fac = fac_raw.trim();
        if fac.is_empty() {
            issues.push("research_owner_faction_bonuses contains an empty faction key".to_string());
            continue;
        }
        if fac != fac_raw {
            issues.push(format!(
                "research_owner_faction_bonuses faction key `{fac_raw}` must not contain surrounding whitespace"
            ));
        }
        let fac_key = fac.to_ascii_lowercase();
        let dest = canonical_owner.entry(fac_key).or_default();
        for (raw_stat, value) in inner {
            let st = raw_stat.trim();
            if st.is_empty() {
                issues.push(format!(
                    "research_owner_faction_bonuses[{fac}] contains an empty stat key"
                ));
                continue;
            }
            if !value.is_finite() {
                issues.push(format!(
                    "research_owner_faction_bonuses[{fac}][{st}] value must be finite"
                ));
                continue;
            }
            let Some(canonical_stat) = normalize_profile_combat_stat(st) else {
                issues.push(format!(
                    "research_owner_faction_bonuses[{fac}][{st}] is not a supported combat stat"
                ));
                continue;
            };
            dest.entry(canonical_stat.to_string())
                .and_modify(|e| *e += *value)
                .or_insert(*value);
        }
    }

    if !issues.is_empty() {
        return Err(issues);
    }

    Ok(PlayerProfile {
        bonuses: canonical_bonuses,
        research_owner_faction_bonuses: canonical_owner,
        support_buffs,
        ops_level: profile.ops_level,
        forbidden_tech_override: profile.forbidden_tech_override,
        chaos_tech_override: profile.chaos_tech_override,
        equipped_forbidden_fid: profile.equipped_forbidden_fid,
        equipped_chaos_fid: profile.equipped_chaos_fid,
    })
}

/// Applies one bonus to profile (add or mult). Mult: (1+current)*(1+value)-1; else additive.
fn accumulate_forbidden_tech_bonus(
    out: &mut HashMap<String, f64>,
    stat: &str,
    operator: &str,
    value: f64,
) {
    let current = out.get(stat).copied().unwrap_or(0.0);
    let is_mult = operator.eq_ignore_ascii_case("mult")
        || operator.eq_ignore_ascii_case("multiply")
        || operator.eq_ignore_ascii_case("mul");
    let new_value = if is_mult {
        (1.0 + current) * (1.0 + value) - 1.0
    } else {
        current + value
    };
    out.insert(stat.to_string(), new_value);
}

/// Merges bonuses from player's synced forbidden/chaos tech into `profile.bonuses`.
/// For each imported tech entry (by `fid`), looks up the catalog by `fid`; if the catalog
/// has a matching `fid`, applies that record's bonuses (additive for "add", multiplicative for "mult").
/// Catalog entries without `fid` are skipped for sync-based lookup.
pub fn merge_forbidden_tech_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_ft: &[ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
) {
    let fids: Vec<i64> = imported_ft.iter().map(|e| e.fid).collect();
    merge_tech_fids_into_profile(profile, &fids, catalog);
}

/// Resolves equipped forbidden/chaos tech fids for combat (merge + derived seats).
/// Only `profile.equipped_forbidden_fid` / `equipped_chaos_fid` apply; legacy `*_override` and the full
/// sync list are ignored for combat. `imported_ft` is retained for call-site compatibility (tier/level).
pub fn resolve_effective_tech_fids(
    profile: &PlayerProfile,
    _imported_ft: &[ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
) -> Vec<i64> {
    let by_fid: HashMap<i64, &ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();

    let mut out: Vec<i64> = Vec::new();

    if let Some(fid) = profile.equipped_forbidden_fid {
        if let Some(rec) = by_fid.get(&fid) {
            if forbidden_chaos_record_is_forbidden_lane(rec) {
                out.push(fid);
            }
        }
    }

    if let Some(fid) = profile.equipped_chaos_fid {
        if let Some(rec) = by_fid.get(&fid) {
            if forbidden_chaos_record_is_chaos_lane(rec) {
                out.push(fid);
            }
        }
    }

    out
}

/// Merges bonuses from tech catalog into profile for the given fids.
pub fn merge_tech_fids_into_profile(
    profile: &mut PlayerProfile,
    fids: &[i64],
    catalog: &ForbiddenChaosList,
) {
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    for &fid in fids {
        if is_borg_alcove_forbidden_tech_fid(fid)
            || is_borg_operating_table_forbidden_tech_fid(fid)
            || is_ship_class_gated_torpedo_family_forbidden_tech_fid(fid)
        {
            continue;
        }
        let Some(record) = by_fid.get(&fid) else {
            continue;
        };
        for bonus in &record.bonuses {
            if skip_forbidden_tech_profile_bonus_for_fid(fid, &bonus.stat) {
                continue;
            }
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };
            accumulate_forbidden_tech_bonus(&mut profile.bonuses, &bonus.stat, op, bonus.value);
        }
    }
}

/// Enable level/tier scaling for forbidden tech via environment.
///
/// This is intentionally opt-in because the exact in-game scaling is uncertain.
/// The current implementation assumes:
/// - Catalog `bonuses` represent max-level bonus values for the tech's `tier`.
/// - Player's synced `level` scales linearly within that tier.
///
/// Env var: `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` (also accepts `true/yes`).
pub fn forbidden_tech_level_tier_scaling_enabled_from_env() -> bool {
    let v = std::env::var("KOBAYASHI_FT_LEVEL_TIER_SCALING").ok();
    let Some(v) = v else { return false };
    let v = v.trim().to_ascii_lowercase();
    matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
}

fn max_forbidden_tech_level_for_tier(tier: u32) -> u32 {
    // Common STFC tier structure: tier 1 -> level 10, tier 2 -> 20, etc.
    // We use tier*10 as a pragmatic placeholder until we confirm a different formula.
    tier.saturating_mul(10)
}

fn scale_forbidden_tech_bonus_value_linear_by_level_tier(
    base_value: f64,
    record_tier: Option<u32>,
    imported_tier: i64,
    imported_level: i64,
) -> f64 {
    if imported_level <= 0 || imported_tier <= 0 {
        return 0.0;
    }

    let imported_tier_u32 = match u32::try_from(imported_tier) {
        Ok(v) => v,
        Err(_) => return 0.0,
    };

    let tier = record_tier.unwrap_or(imported_tier_u32);

    // Safety: when catalog tier is explicitly set and disagrees with the synced tech tier,
    // don't attempt to cross-scale; keep the catalog's value unchanged.
    if let Some(record_tier) = record_tier {
        if record_tier != imported_tier_u32 {
            return base_value;
        }
    }

    let max_level = max_forbidden_tech_level_for_tier(tier);
    if max_level == 0 {
        return 0.0;
    }

    let factor = (imported_level as f64) / (max_level as f64);
    let factor = factor.clamp(0.0, 1.0);
    base_value * factor
}

/// Merge the selected forbidden/chaos tech catalog records into `profile.bonuses`.
///
/// Compared to [`merge_tech_fids_into_profile`], this variant can optionally scale bonuses
/// by the player's synced `tier` and `level`.
pub fn merge_tech_fids_into_profile_with_level_tier(
    profile: &mut PlayerProfile,
    fids: &[i64],
    imported_ft: &[crate::data::import::ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) {
    if fids.is_empty() || catalog.items.is_empty() {
        return;
    }

    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();

    // We need imported tier/level to scale bonuses when enabled.
    let imported_by_fid: HashMap<i64, &crate::data::import::ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();

    for &fid in fids {
        if is_borg_alcove_forbidden_tech_fid(fid)
            || is_borg_operating_table_forbidden_tech_fid(fid)
            || is_ship_class_gated_torpedo_family_forbidden_tech_fid(fid)
        {
            continue;
        }
        let Some(record) = by_fid.get(&fid) else {
            continue;
        };
        let imported = match imported_by_fid.get(&fid) {
            Some(v) => *v,
            None => continue,
        };

        for bonus in &record.bonuses {
            let op = if bonus.operator.is_empty() {
                "add"
            } else {
                bonus.operator.as_str()
            };

            if skip_forbidden_tech_profile_bonus_for_fid(fid, &bonus.stat) {
                continue;
            }

            let value = forbidden_tech_bonus_value_for_imported_entry(
                bonus,
                record,
                Some(imported),
                scale_by_level_tier,
            );

            if value == 0.0 {
                continue;
            }
            accumulate_forbidden_tech_bonus(&mut profile.bonuses, &bonus.stat, op, value);
        }
    }
}

/// Game fid for **Borg Alcove** forbidden tech. Combat bonuses are **not** applied as unconditional
/// [`PlayerProfile::bonuses`]; see [`forbidden_tech_derived_attack_phase_seats`] and
/// [`borg_alcove_hull_hp_bonus_fraction`].
pub const BORG_ALCOVE_FORBIDDEN_TECH_FID: i64 = 733381942;

/// **Borg Operating Table** (Update 89 prototype forbidden tech). Combat rows are **not** merged into
/// unconditional [`PlayerProfile::bonuses`]; see [`borg_operating_table_forbidden_tech_seats`].
pub const BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID: i64 = 3042210440;

/// Quantum Slipstream Drive — opponent mitigation debuff is modeled in combat, not as additive player
/// [`PlayerProfile::bonuses`] `shield_mitigation`.
pub const QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID: i64 = 2439729135;

/// S31 Torpedo Pods — upstream copy is **Battleship**-scoped; hostile-only lines use [`AbilityCondition`]
/// with [`AbilityCondition::DefenderIsNpcHostile`].
pub const S31_TORPEDO_PODS_FORBIDDEN_TECH_FID: i64 = 473132032;

/// Control Seeker Probes — **Explorer**-scoped torpedo family (same tier template as S31).
pub const CONTROL_SEEKER_PROBES_FORBIDDEN_TECH_FID: i64 = 2423550592;

/// Dual Photon Warheads — **Interceptor**-scoped torpedo family (same tier template as S31).
pub const DUAL_PHOTON_WARHEADS_FORBIDDEN_TECH_FID: i64 = 1364700249;

/// When catalog omits `shield_mitigation` (e.g. CSV regen maps opponent lines to null), cap debuff magnitude.
const QUANTUM_SLIPSTREAM_OPPONENT_MITIGATION_CAP_DEFAULT: f64 = 0.15;

/// Placeholder spread until in-game cumulative cadence is confirmed (cap reached over this many rounds).
const QUANTUM_SLIPSTREAM_MITIGATION_DEBUFF_ROUND_SPREAD: f64 = 3.0;

/// Data ship id for U.S.S. Voyager (`data/ships_extended/uss_voyager.json`).
pub const USS_VOYAGER_SHIP_ID: &str = "uss_voyager";

#[inline]
fn is_borg_alcove_forbidden_tech_fid(fid: i64) -> bool {
    fid == BORG_ALCOVE_FORBIDDEN_TECH_FID
}

#[inline]
fn is_borg_operating_table_forbidden_tech_fid(fid: i64) -> bool {
    fid == BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID
}

#[inline]
fn is_quantum_slipstream_forbidden_tech_fid(fid: i64) -> bool {
    fid == QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID
}

/// Hull-class + hostile-gated "torpedo family" forbidden tech: catalog bonuses apply only when the resolved
/// attacker ship matches the tech's hull class and the defender is an NPC hostile (see derived seats / scenario).
#[inline]
pub fn ship_class_gated_torpedo_family_attacker_ship_type(fid: i64) -> Option<ShipType> {
    match fid {
        S31_TORPEDO_PODS_FORBIDDEN_TECH_FID => Some(ShipType::Battleship),
        CONTROL_SEEKER_PROBES_FORBIDDEN_TECH_FID => Some(ShipType::Explorer),
        DUAL_PHOTON_WARHEADS_FORBIDDEN_TECH_FID => Some(ShipType::Interceptor),
        _ => None,
    }
}

#[inline]
fn is_ship_class_gated_torpedo_family_forbidden_tech_fid(fid: i64) -> bool {
    ship_class_gated_torpedo_family_attacker_ship_type(fid).is_some()
}

/// Bonuses that exist in the catalog for sync/scaling but must not become unconditional profile modifiers.
#[inline]
fn skip_forbidden_tech_profile_bonus_for_fid(fid: i64, stat: &str) -> bool {
    stat == "shield_mitigation" && is_quantum_slipstream_forbidden_tech_fid(fid)
}

/// How a forbidden/chaos catalog bonus row reaches combat (profile merge, timed seats, or scalars).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForbiddenTechBonusCombatRoute {
    /// Unconditional [`merge_tech_fids_into_profile`] when the fid is equipped.
    ProfileFlat,
    /// Attack/round-start seats from dedicated builders (Borg, Quantum, torpedo family, …).
    TimedSeat,
    /// Hull HP or hostile-accuracy scalars applied in scenario build (torpedo family, Borg Alcove hull).
    HullOrAccuracyScalar,
    /// Catalog row exists for sync/scaling but is intentionally not a flat profile modifier.
    SkippedIntentionally,
}

/// Returns `None` when the catalog `(fid, stat)` pair has no modeled combat consumer (mapping gap).
pub fn forbidden_tech_bonus_combat_route(
    fid: i64,
    stat: &str,
) -> Option<ForbiddenTechBonusCombatRoute> {
    if is_borg_alcove_forbidden_tech_fid(fid) {
        return match stat {
            "crit_chance" | "crit_damage" => Some(ForbiddenTechBonusCombatRoute::TimedSeat),
            "hull_hp" => Some(ForbiddenTechBonusCombatRoute::HullOrAccuracyScalar),
            _ => None,
        };
    }
    if is_borg_operating_table_forbidden_tech_fid(fid) {
        return match stat {
            "crit_damage" | "apex_shred" | "hostile_crit_damage_reduction" => {
                Some(ForbiddenTechBonusCombatRoute::TimedSeat)
            }
            _ => None,
        };
    }
    if is_quantum_slipstream_forbidden_tech_fid(fid) {
        return match stat {
            "shield_mitigation" => Some(ForbiddenTechBonusCombatRoute::TimedSeat),
            _ if normalize_profile_combat_stat(stat).is_some() => {
                Some(ForbiddenTechBonusCombatRoute::ProfileFlat)
            }
            _ => None,
        };
    }
    if is_ship_class_gated_torpedo_family_forbidden_tech_fid(fid) {
        return match stat {
            "armor" | "dodge" | "pierce" | "weapon_damage" => {
                Some(ForbiddenTechBonusCombatRoute::TimedSeat)
            }
            "hull_hp" | "shield_mitigation" | "accuracy" => {
                Some(ForbiddenTechBonusCombatRoute::HullOrAccuracyScalar)
            }
            _ => None,
        };
    }
    if skip_forbidden_tech_profile_bonus_for_fid(fid, stat) {
        return Some(ForbiddenTechBonusCombatRoute::SkippedIntentionally);
    }
    if normalize_profile_combat_stat(stat).is_some() {
        return Some(ForbiddenTechBonusCombatRoute::ProfileFlat);
    }
    None
}

fn forbidden_tech_bonus_value_for_imported_entry(
    bonus: &crate::data::forbidden_chaos::BonusEntry,
    record: &crate::data::forbidden_chaos::ForbiddenChaosRecord,
    imported: Option<&ForbiddenTechEntry>,
    scale_by_level_tier: bool,
) -> f64 {
    if !scale_by_level_tier {
        return bonus.value;
    }
    let Some(imported) = imported else {
        return bonus.value;
    };
    scale_forbidden_tech_bonus_value_linear_by_level_tier(
        bonus.value,
        record.tier,
        imported.tier,
        imported.level,
    )
}

/// Scaled `hull_hp` catalog bonus for Borg Alcove when that tech is active (apply only on
/// [`USS_VOYAGER_SHIP_ID`] in scenario build).
pub fn borg_alcove_hull_hp_bonus_fraction(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) -> Option<f64> {
    if !effective_fids
        .iter()
        .any(|&f| is_borg_alcove_forbidden_tech_fid(f))
    {
        return None;
    }
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let record = *by_fid.get(&BORG_ALCOVE_FORBIDDEN_TECH_FID)?;
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let imported = imported_by_fid
        .get(&BORG_ALCOVE_FORBIDDEN_TECH_FID)
        .copied();
    for bonus in &record.bonuses {
        if bonus.stat != "hull_hp" {
            continue;
        }
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if v.is_finite() && v != 0.0 {
            return Some(v);
        }
    }
    None
}

/// Borg Alcove: crit stats as attack-phase ship seats (Voyager + NPC for crit chance; NPC-only for
/// crit damage). **Delta Quadrant** scoping from in-game copy is not modeled (no regional hostile tag).
pub fn forbidden_tech_derived_attack_phase_seats(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) -> Vec<CrewSeatContext> {
    if !effective_fids
        .iter()
        .any(|&f| is_borg_alcove_forbidden_tech_fid(f))
    {
        return Vec::new();
    }
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let Some(record) = by_fid.get(&BORG_ALCOVE_FORBIDDEN_TECH_FID).copied() else {
        return Vec::new();
    };
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let imported = imported_by_fid
        .get(&BORG_ALCOVE_FORBIDDEN_TECH_FID)
        .copied();

    let voyager_and_npc = AbilityCondition::And(vec![
        AbilityCondition::AttackerShipIdIs(USS_VOYAGER_SHIP_ID.to_string()),
        AbilityCondition::DefenderIsNpcHostile,
    ]);
    let npc_only = AbilityCondition::DefenderIsNpcHostile;

    let mut out: Vec<CrewSeatContext> = Vec::new();
    let mut idx = 0u32;
    for bonus in &record.bonuses {
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if !v.is_finite() || v == 0.0 {
            continue;
        }
        match bonus.stat.as_str() {
            "crit_chance" => {
                idx = idx.saturating_add(1);
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!("forbidden_tech_borg_alcove_crit_chance_{idx}"),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::AttackPhase,
                        boostable: false,
                        effect: AbilityEffect::CritChanceBonus(
                            crate::data::ship_ability_resolve::normalize_probability(v),
                        ),
                        condition: Some(voyager_and_npc.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
            "crit_damage" => {
                idx = idx.saturating_add(1);
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!("forbidden_tech_borg_alcove_crit_damage_{idx}"),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::AttackPhase,
                        boostable: false,
                        effect: AbilityEffect::CritDamageMultiplier((1.0 + v).max(EPSILON)),
                        condition: Some(npc_only.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
            "hull_hp" => {}
            _ => {}
        }
    }
    out
}

/// Borg Operating Table: [`AbilityEffect`] seats gated on **Conqueror Borg** hostiles (tag
/// `conqueror_borg` on the defender) plus [`AbilityCondition::DefenderIsNpcHostile`].
///
/// Catalog magnitudes are maintained in [`crate::data::forbidden_chaos::ForbiddenChaosList`] (see
/// `data/import/forbidden_chaos_tech.csv`); they are **curated** from upstream
/// `data/upstream/data-stfc-space/forbidden_tech/3042210440.json` and Update 89 copy (crit damage /
/// apex shred vs Conqueror; reduction on incoming hostile crits). Refine when log-backed fights exist.
///
/// Supported catalog stats on this fid:
/// - `crit_damage` → [`AbilityEffect::CritDamageMultiplier`] at [`TimingWindow::AttackPhase`]
/// - `apex_shred` → [`AbilityEffect::ApexShredBonus`]
/// - `hostile_crit_damage_reduction` → [`AbilityEffect::HostileCritDamageReduction`] at [`TimingWindow::CombatBegin`]
pub fn borg_operating_table_forbidden_tech_seats(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) -> Vec<CrewSeatContext> {
    if !effective_fids
        .iter()
        .any(|&f| is_borg_operating_table_forbidden_tech_fid(f))
    {
        return Vec::new();
    }
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let Some(record) = by_fid
        .get(&BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID)
        .copied()
    else {
        return Vec::new();
    };
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let imported = imported_by_fid
        .get(&BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID)
        .copied();

    let conqueror_gate = AbilityCondition::And(vec![
        AbilityCondition::DefenderIsNpcHostile,
        AbilityCondition::DefenderHostileTagsAllPresent {
            required_mask: crate::combat::hostile_tags::HOSTILE_TAG_MASK_CONQUEROR_BORG,
        },
    ]);

    let mut out: Vec<CrewSeatContext> = Vec::new();
    let mut idx: u32 = 0;
    for bonus in &record.bonuses {
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if !v.is_finite() || v <= 0.0 {
            continue;
        }
        match bonus.stat.as_str() {
            "crit_damage" => {
                idx = idx.saturating_add(1);
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!("forbidden_tech_borg_operating_table_crit_damage_{idx}"),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::AttackPhase,
                        boostable: false,
                        effect: AbilityEffect::CritDamageMultiplier((1.0 + v).max(EPSILON)),
                        condition: Some(conqueror_gate.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
            "apex_shred" => {
                idx = idx.saturating_add(1);
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!("forbidden_tech_borg_operating_table_apex_shred_{idx}"),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::AttackPhase,
                        boostable: false,
                        effect: AbilityEffect::ApexShredBonus(v),
                        condition: Some(conqueror_gate.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
            "hostile_crit_damage_reduction" => {
                idx = idx.saturating_add(1);
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!(
                            "forbidden_tech_borg_operating_table_hostile_crit_reduction_{idx}"
                        ),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::CombatBegin,
                        boostable: false,
                        effect: AbilityEffect::HostileCritDamageReduction {
                            reduction: v.clamp(0.0, 0.95),
                            duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
                        },
                        condition: Some(conqueror_gate.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
            _ => {}
        }
    }
    out
}

/// Quantum Slipstream: cumulative debuff to **opponent** shield mitigation (NPC hostiles only).
/// Catalog `shield_mitigation` supplies the debuff cap when present (scaled like other bonuses); otherwise
/// a conservative default is used.
pub fn quantum_slipstream_forbidden_tech_round_start_seats(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) -> Vec<CrewSeatContext> {
    if !effective_fids
        .iter()
        .any(|&f| is_quantum_slipstream_forbidden_tech_fid(f))
    {
        return Vec::new();
    }
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let Some(record) = by_fid.get(&QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID).copied() else {
        return Vec::new();
    };
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let imported = imported_by_fid
        .get(&QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID)
        .copied();

    let mut cap: Option<f64> = None;
    for bonus in &record.bonuses {
        if bonus.stat != "shield_mitigation" {
            continue;
        }
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if v.is_finite() && v > 0.0 {
            cap = Some(v);
            break;
        }
    }
    let cap = cap.unwrap_or(QUANTUM_SLIPSTREAM_OPPONENT_MITIGATION_CAP_DEFAULT);
    if !cap.is_finite() || cap <= 0.0 {
        return Vec::new();
    }
    let per_round = (cap / QUANTUM_SLIPSTREAM_MITIGATION_DEBUFF_ROUND_SPREAD).max(0.0);
    if per_round <= 0.0 {
        return Vec::new();
    }

    vec![CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: "forbidden_tech_quantum_slipstream_opponent_shield_mitigation_debuff".to_string(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round, cap },
            condition: Some(AbilityCondition::DefenderIsNpcHostile),
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    }]
}

fn torpedo_family_hostile_accuracy_fraction_for_record(
    record: &crate::data::forbidden_chaos::ForbiddenChaosRecord,
    imported: Option<&ForbiddenTechEntry>,
    scale_by_level_tier: bool,
) -> Option<f64> {
    for bonus in &record.bonuses {
        if bonus.stat != "accuracy" {
            continue;
        }
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if v.is_finite() && v > 0.0 {
            return Some(v);
        }
    }
    for bonus in &record.bonuses {
        if bonus.stat != "pierce" {
            continue;
        }
        let v = forbidden_tech_bonus_value_for_imported_entry(
            bonus,
            record,
            imported,
            scale_by_level_tier,
        );
        if v.is_finite() && v > 0.0 {
            return Some(v);
        }
    }
    None
}

/// Sum of catalog `hull_hp` bonuses for torpedo-family techs whose hull gate matches `ship_rec`.
/// Returns [`None`] when the ship is unknown so hull is not scaled unconditionally.
pub fn ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
    ship_rec: Option<&ShipRecord>,
) -> Option<f64> {
    let sr = ship_rec?;
    let st = sr.ship_type();
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut sum = 0.0_f64;
    for &fid in effective_fids {
        if !seen.insert(fid) {
            continue;
        }
        if ship_class_gated_torpedo_family_attacker_ship_type(fid) != Some(st) {
            continue;
        }
        let Some(record) = by_fid.get(&fid).copied() else {
            continue;
        };
        let imported = imported_by_fid.get(&fid).copied();
        for bonus in &record.bonuses {
            if bonus.stat != "hull_hp" {
                continue;
            }
            let v = forbidden_tech_bonus_value_for_imported_entry(
                bonus,
                record,
                imported,
                scale_by_level_tier,
            );
            if v.is_finite() && v > 0.0 {
                sum += v;
            }
        }
    }
    (sum.is_finite() && sum > 0.0).then_some(sum)
}

/// Additive shield deflection for the player combatant; scenario applies vs hostiles only.
pub fn ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
    ship_rec: Option<&ShipRecord>,
) -> Option<f64> {
    let sr = ship_rec?;
    let st = sr.ship_type();
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut sum = 0.0_f64;
    for &fid in effective_fids {
        if !seen.insert(fid) {
            continue;
        }
        if ship_class_gated_torpedo_family_attacker_ship_type(fid) != Some(st) {
            continue;
        }
        let Some(record) = by_fid.get(&fid).copied() else {
            continue;
        };
        let imported = imported_by_fid.get(&fid).copied();
        for bonus in &record.bonuses {
            if bonus.stat != "shield_mitigation" {
                continue;
            }
            let v = forbidden_tech_bonus_value_for_imported_entry(
                bonus,
                record,
                imported,
                scale_by_level_tier,
            );
            if v.is_finite() && v > 0.0 {
                sum += v;
            }
        }
    }
    (sum.is_finite() && sum > 0.0).then_some(sum)
}

/// Sum of catalog hostile-accuracy fractions (`accuracy` row, else `pierce`) for matching-hull family techs.
/// Profile layer uses [`apply_profile_accuracy_to_attacker_stats`].
pub fn ship_class_gated_torpedo_family_hostile_accuracy_sum_for_resolved_ship(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
    ship_rec: Option<&ShipRecord>,
) -> Option<f64> {
    let sr = ship_rec?;
    let st = sr.ship_type();
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut sum = 0.0_f64;
    for &fid in effective_fids {
        if !seen.insert(fid) {
            continue;
        }
        if ship_class_gated_torpedo_family_attacker_ship_type(fid) != Some(st) {
            continue;
        }
        let Some(record) = by_fid.get(&fid).copied() else {
            continue;
        };
        let imported = imported_by_fid.get(&fid).copied();
        if let Some(a) = torpedo_family_hostile_accuracy_fraction_for_record(
            record,
            imported,
            scale_by_level_tier,
        ) {
            sum += a;
        }
    }
    (sum.is_finite() && sum > 0.0).then_some(sum)
}

/// Per equipped family fid: armor+dodge (return-fire mitigation), pierce, weapon damage — gated on hull class + hostile.
/// Shield mitigation and hull are applied in [`crate::optimizer::monte_carlo::scenario`].
pub fn ship_class_gated_torpedo_family_derived_seats(
    imported_ft: &[ForbiddenTechEntry],
    effective_fids: &[i64],
    catalog: &ForbiddenChaosList,
    scale_by_level_tier: bool,
) -> Vec<CrewSeatContext> {
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();
    let imported_by_fid: HashMap<i64, &ForbiddenTechEntry> =
        imported_ft.iter().map(|e| (e.fid, e)).collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut out: Vec<CrewSeatContext> = Vec::new();
    for &fid in effective_fids {
        if !seen.insert(fid) {
            continue;
        }
        let Some(ship_gate) = ship_class_gated_torpedo_family_attacker_ship_type(fid) else {
            continue;
        };
        let Some(record) = by_fid.get(&fid).copied() else {
            continue;
        };
        let imported = imported_by_fid.get(&fid).copied();
        let mut armor = 0.0_f64;
        let mut dodge = 0.0_f64;
        let mut pierce: Option<f64> = None;
        let mut weapon_damage: Option<f64> = None;
        for bonus in &record.bonuses {
            let v = forbidden_tech_bonus_value_for_imported_entry(
                bonus,
                record,
                imported,
                scale_by_level_tier,
            );
            if !v.is_finite() || v == 0.0 {
                continue;
            }
            match bonus.stat.as_str() {
                "armor" => armor = v,
                "dodge" => dodge = v,
                "pierce" => pierce = Some(v),
                "weapon_damage" => weapon_damage = Some(v),
                _ => {}
            }
        }
        let gated = AbilityCondition::And(vec![
            AbilityCondition::AttackerShipTypeIs(ship_gate),
            AbilityCondition::DefenderIsNpcHostile,
        ]);
        let mitigation_packed = armor + dodge;
        if mitigation_packed.is_finite() && mitigation_packed > 0.0 {
            out.push(CrewSeatContext {
                seat: CrewSeat::Ship,
                ability: Ability {
                    name: format!(
                        "forbidden_tech_ship_class_torpedo_family_{fid}_armor_dodge_return_fire"
                    ),
                    class: AbilityClass::ShipAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::MitigationAdditive(mitigation_packed),
                    condition: Some(gated.clone()),
                },
                boosted: false,
                officer_id: None,
                contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
            });
        }
        if let Some(p) = pierce {
            if p.is_finite() && p > 0.0 {
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!("forbidden_tech_ship_class_torpedo_family_{fid}_pierce"),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::CombatBegin,
                        boostable: false,
                        effect: AbilityEffect::PierceBonus(p),
                        condition: Some(gated.clone()),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
        }
        if let Some(wd) = weapon_damage {
            if wd.is_finite() && wd > 0.0 {
                out.push(CrewSeatContext {
                    seat: CrewSeat::Ship,
                    ability: Ability {
                        name: format!(
                            "forbidden_tech_ship_class_torpedo_family_{fid}_weapon_damage"
                        ),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::AttackPhase,
                        boostable: false,
                        effect: AbilityEffect::AttackMultiplier(wd),
                        condition: Some(gated),
                    },
                    boosted: false,
                    officer_id: None,
                    contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
                });
            }
        }
    }
    out
}

/// Research / building / forbidden-tech bonus `stat` strings are normalized here before merging into
/// [`PlayerProfile::bonuses`]. Keep in sync with `ALLOWED_COMBAT_STATS` in `scripts/import_stfcspace_research.mjs`
/// (same engine keys; importer also allows aliases that fold here, e.g. `armor_pierce` → `pierce`).
///
/// **Where each normalized key affects combat**
///
/// | Key | Flat merge into `profile.bonuses` | Applied via |
/// |-----|-----------------------------------|---------------|
/// | `weapon_damage`, `hull_hp`, `shield_hp`, `crit_chance`, `crit_damage`, `crit_damage_floor`, `pierce`, `shield_mitigation`, `armor`, `shield_deflection`, `dodge`, `damage_reduction`, `isolytic_damage`, `isolytic_defense`, `isolytic_cascade_damage` (alias `isolytic_cascade`), `apex_shred`, `apex_barrier` | yes (unless conditional `weapon_damage` / crit row; see below) | [`apply_profile_to_attacker`] on [`Combatant`] for most keys; `armor` / `shield_deflection` / `dodge` add into [`Combatant::mitigation`]; `crit_damage_floor` populates [`Combatant::crit_damage_floor`] (clamped at the per-shot crit resolution site after any attacker-outbound crit-damage reduction); `isolytic_cascade_damage` is merged into `profile.bonuses` but applied in Monte Carlo scenario build as an attack-phase `IsolyticCascadeDamageBonus` seat (with LCARS static buff keys of the same name), not as a [`Combatant`] field |
/// | `officer_attack`, `officer_health` | yes | Multiplicative with `weapon_damage` / `hull_hp`: attack × `(1+weapon_damage)×(1+officer_attack)`, hull × `(1+hull_hp)×(1+officer_health)` |
/// | `officer_defense` | yes | Additive with `shield_mitigation` into [`Combatant::shield_mitigation`] (same cap `[0,1]`) |
/// | `accuracy` | yes | [`apply_profile_accuracy_to_attacker_stats`] on [`AttackerStats`] (not `Combatant`) |
/// | `isolytic_damage` (conditional, e.g. `requires_morale`) | no | [`research_derived_attack_phase_seats`] → compiled seats (not `profile.bonuses` flat merge) |
/// | Conditional `weapon_damage`, `crit_chance` / `crit_damage` with [`crate::data::research::ResearchBonusConditionKey`] set | no (skipped from flat merge) | [`research_derived_attack_phase_seats`] → attack-phase seats |
///
/// Aliases: `armor_pierce`, `shield_pierce` → `pierce`.
pub(crate) fn normalize_profile_combat_stat(stat: &str) -> Option<&'static str> {
    match stat {
        // Officer Attack/Defense/Health (syndicate Officer_Stats columns, Command Center, etc.):
        // kept distinct from ship-level keys so [`apply_profile_to_attacker`] can compound them.
        "officer_attack" => Some("officer_attack"),
        "officer_defense" => Some("officer_defense"),
        "officer_health" => Some("officer_health"),
        "weapon_damage" => Some("weapon_damage"),
        "hull_hp" => Some("hull_hp"),
        "shield_hp" => Some("shield_hp"),
        // Morale-gated isolytic catalog rows use `stat: isolytic_damage` + `requires_morale` (compiled to seats).
        "isolytic_damage" => Some("isolytic_damage"),
        "isolytic_defense" => Some("isolytic_defense"),
        // Isolytic cascade stacks in the isolytic damage leg (see `mitigation::isolytic_damage`); applied
        // from profile/buildings/research/FT via scenario attack-phase seat, not as a Combatant scalar.
        "isolytic_cascade_damage" | "isolytic_cascade" => Some("isolytic_cascade_damage"),
        // Apex: same units as Combatant / engine (shred decimal; barrier pool vs hostile barrier formula).
        "apex_shred" => Some("apex_shred"),
        "apex_barrier" => Some("apex_barrier"),
        // Building-only conditional profile keys: resolved in scenario to conditional ship seats / gated scalars.
        "player_crit_damage_reduction" => Some("player_crit_damage_reduction"),
        "apex_barrier_vs_player_tal_not_on_bridge" => {
            Some("apex_barrier_vs_player_tal_not_on_bridge")
        }
        "crit_chance" => Some("crit_chance"),
        "crit_damage" => Some("crit_damage"),
        // Defensive clamp: ensures the attacker's effective crit multiplier never drops
        // below this floor after opponent-side crit-damage reductions are applied.
        // Populated by the 4 "Critical Damage Floor" research nodes (596446780,
        // 1336793796, 1727094437, 2601710565), additive across owned nodes.
        "crit_damage_floor" => Some("crit_damage_floor"),
        "pierce" | "armor_pierce" | "shield_pierce" => Some("pierce"),
        "shield_mitigation" => Some("shield_mitigation"),
        "armor" => Some("armor"),
        // Raw shield deflection (defender leg; same units as ship `DefenderStats::shield_deflection`).
        "shield_deflection" => Some("shield_deflection"),
        "dodge" => Some("dodge"),
        "damage_reduction" => Some("damage_reduction"),
        // Used with ship `AttackerStats.accuracy` for dodge leg of mitigation (see scenario.rs).
        // Catalog values are fractional (e.g. 0.1 = +10% effective accuracy vs base ship stat).
        "accuracy" => Some("accuracy"),
        _ => None,
    }
}

/// Merges combat stat bonuses from player's synced buildings into `profile.bonuses`.
/// Resolves bid → building id via `bid_to_id`, loads building records, takes **tier-snapshot**
/// bonuses at each synced module level (see [`crate::data::building::cumulative_building_bonuses_with_context`]),
/// and adds only combat keys (weapon_damage, hull_hp, officer_attack, …). armor_pierce and
/// shield_pierce are folded into pierce.
pub fn merge_building_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_buildings: &[BuildingEntry],
    bid_to_id: &HashMap<i64, String>,
    _building_index: &BuildingIndex,
    data_dir: &Path,
    context: &BuildingBonusContext,
) {
    if imported_buildings.is_empty() || bid_to_id.is_empty() {
        return;
    }

    let mut levels_by_id: HashMap<String, u32> = HashMap::new();
    for entry in imported_buildings {
        let Some(id) = bid_to_id.get(&entry.bid) else {
            continue;
        };
        let level = if entry.level >= 0 {
            entry.level.min(i64::from(u32::MAX)) as u32
        } else {
            0
        };
        levels_by_id.insert(id.clone(), level);
    }
    if levels_by_id.is_empty() {
        return;
    }

    let mut records: Vec<building::BuildingRecord> = Vec::new();
    for id in levels_by_id.keys() {
        if let Some(rec) = building::load_building_record(data_dir, id) {
            records.push(rec);
        }
    }
    if records.is_empty() {
        return;
    }

    let bonuses =
        building::cumulative_building_bonuses_with_context(&records, &levels_by_id, context);

    for (stat, value) in bonuses {
        let Some(key) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        let current = profile.bonuses.get(key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key.to_string(), current + value);
    }
}

/// Support buff ids for Titan-A **Fortify** / Fortified (exclusive group in `data/support_buffs.json`).
/// Keep in sync with `TITAN_A_FORTIFY_SUPPORT_BUFF_IDS` in `frontend/src/lib/supportBuffs.ts` (web UI checkboxes).
pub const TITAN_A_FORTIFY_SUPPORT_BUFF_IDS: &[&str] =
    &["titan_a_fortification", "titan_a_max_fortification"];

/// `data/support_buffs.json` key for Cerritos alliance support (web UI + API).
/// Keep in sync with `CERRITOS_SUPPORT_BUFF_ID` in `frontend/src/lib/supportBuffs.ts`.
pub const CERRITOS_SUPPORT_BUFF_ID: &str = "cerritos_support";

/// `data/support_buffs.json` key for Defiant reinforce (web UI + API).
/// Keep in sync with `DEFIANT_REINFORCE_BUFF_ID` in `frontend/src/lib/supportBuffs.ts`.
pub const DEFIANT_REINFORCE_BUFF_ID: &str = "defiant_reinforce";

/// Game research ids whose **catalog combat bonuses** apply only while a Titan-A Fortify support buff
/// is active (in-game **Fortified** from alliance Titan-A).
///
/// Names: Titan's Fist, Clash of Titan, Titan Shield (Fortified line), Titan Force, Titan's Bulwark,
/// Titan Power, Titan's Fortune.
pub const TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS: &[i64] = &[
    1_110_389_612,
    3_414_721_139,
    2_004_950_139,
    2_448_831_923,
    4_127_725_796,
    3_591_450_656,
    1_724_539_590,
];

/// **Max Fortified** copy (loca ids 53011, 53017, …); merged only when `titan_a_max_fortification` is selected.
pub const TITAN_MAX_FORTIFICATION_GATED_RESEARCH_RIDS: &[i64] = &[
    122_225_579,
    1_231_049_311,
    3_706_606_658,
    1_947_967_502,
    4_043_991_602,
    4_214_494_393,
];

/// **Supported by a Cerritos** / **supported by the Cerritos** (not dual Titan+Cerritos; see dual rid).
pub const CERRITOS_SUPPORT_GATED_RESEARCH_RIDS: &[i64] = &[
    360_952_209,
    614_188_988,
    641_841_437,
    677_345_161,
    896_198_526,
    1_753_145_287,
    1_819_818_771,
    1_985_436_799,
    2_077_375_366,
    2_800_647_811,
    2_812_467_102,
    2_848_942_876,
    3_149_755_773,
    3_281_870_661,
    4_114_549_617,
];

/// **Buffed by the Defiant** (solo/group Armada copy).
pub const DEFIANT_REINFORCE_GATED_RESEARCH_RIDS: &[i64] = &[
    79_182_414,
    207_675_799,
    242_592_436,
    258_755_994,
    380_673_310,
    397_271_235,
    646_659_704,
    753_025_502,
    1_488_569_245,
    1_841_846_761,
    2_303_614_878,
    2_462_026_539,
    2_509_162_890,
    3_159_432_452,
    3_421_161_870,
    3_716_895_003,
    3_747_707_279,
    4_259_319_033,
];

/// Titan Wrecker — **Cerritos and Fortified**; merged only when both Cerritos support and Titan Fortify buffs apply.
pub const TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID: i64 = 1_212_216_403;

/// True when this research `rid` merges only under an active support buff (Cerritos, Fortify, …).
pub fn is_support_buff_gated_research_rid(rid: i64) -> bool {
    is_support_buff_gated_combat_research_rid(rid)
}

#[inline]
fn is_support_buff_gated_combat_research_rid(rid: i64) -> bool {
    TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS.contains(&rid)
        || TITAN_MAX_FORTIFICATION_GATED_RESEARCH_RIDS.contains(&rid)
        || CERRITOS_SUPPORT_GATED_RESEARCH_RIDS.contains(&rid)
        || DEFIANT_REINFORCE_GATED_RESEARCH_RIDS.contains(&rid)
        || rid == TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID
}

/// Which alliance support buffs are active for this scenario (resolved ids after catalog + exclusive groups).
#[derive(Debug, Clone, Default)]
pub struct SupportBuffResearchGateState {
    /// `titan_a_fortification` or `titan_a_max_fortification`.
    pub titan_fortify: bool,
    /// `titan_a_max_fortification` only (adds Max Fortified research on top of Fortified research).
    pub titan_max_fortification: bool,
    pub cerritos_support: bool,
    pub defiant_reinforce: bool,
}

impl SupportBuffResearchGateState {
    pub fn from_resolved_support_buff_ids(resolved: &[String]) -> Self {
        Self {
            titan_fortify: resolved
                .iter()
                .any(|id| TITAN_A_FORTIFY_SUPPORT_BUFF_IDS.contains(&id.as_str())),
            titan_max_fortification: resolved
                .iter()
                .any(|id| id.as_str() == "titan_a_max_fortification"),
            cerritos_support: resolved
                .iter()
                .any(|id| id.as_str() == CERRITOS_SUPPORT_BUFF_ID),
            defiant_reinforce: resolved
                .iter()
                .any(|id| id.as_str() == DEFIANT_REINFORCE_BUFF_ID),
        }
    }
}

#[inline]
pub fn titan_a_fortify_active_in_resolved_support_buffs(resolved: &[String]) -> bool {
    SupportBuffResearchGateState::from_resolved_support_buff_ids(resolved).titan_fortify
}

/// Strips all support-buff–gated research: those rows merge into the corresponding static buff layer, not `profile.bonuses`.
pub(crate) fn research_entries_excluding_support_buff_gated<'a>(
    entries: &'a [ResearchEntry],
) -> Cow<'a, [ResearchEntry]> {
    let needs_filter = entries
        .iter()
        .any(|e| is_support_buff_gated_combat_research_rid(e.rid));
    if !needs_filter {
        return Cow::Borrowed(entries);
    }
    Cow::Owned(
        entries
            .iter()
            .filter(|e| !is_support_buff_gated_combat_research_rid(e.rid))
            .cloned()
            .collect(),
    )
}

/// Import slice for [`research_derived_attack_phase_seats`]: gated `rid`s only when their support buff is active.
pub(crate) fn research_entries_for_support_buff_gated_attack_phase<'a>(
    entries: &'a [ResearchEntry],
    gates: &SupportBuffResearchGateState,
) -> Cow<'a, [ResearchEntry]> {
    fn keep_rid(rid: i64, g: &SupportBuffResearchGateState) -> bool {
        if !is_support_buff_gated_combat_research_rid(rid) {
            return true;
        }
        if TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS.contains(&rid) {
            return g.titan_fortify;
        }
        if TITAN_MAX_FORTIFICATION_GATED_RESEARCH_RIDS.contains(&rid) {
            return g.titan_max_fortification;
        }
        if CERRITOS_SUPPORT_GATED_RESEARCH_RIDS.contains(&rid) {
            return g.cerritos_support;
        }
        if rid == TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID {
            return g.cerritos_support && g.titan_fortify;
        }
        if DEFIANT_REINFORCE_GATED_RESEARCH_RIDS.contains(&rid) {
            return g.defiant_reinforce;
        }
        true
    }
    if entries.iter().all(|e| keep_rid(e.rid, gates)) {
        return Cow::Borrowed(entries);
    }
    Cow::Owned(
        entries
            .iter()
            .filter(|e| keep_rid(e.rid, gates))
            .cloned()
            .collect(),
    )
}

/// Keys that use **absolute** multipliers in [`apply_static_buffs_to_combatant`] / static buff merge
/// (must match [`crate::data::support_buffs`] mult semantics for Fortify research fold-in).
const PROFILE_TO_STATIC_MULT_KEYS: &[&str] = &[
    "weapon_damage",
    "hull_hp",
    "shield_hp",
    "crit_damage",
    "accuracy_cb_mult",
];

/// Converts combat-only map as produced for [`PlayerProfile::bonuses`] (additive weapon/crit deltas)
/// into the static-buff map shape consumed by [`apply_static_buffs_to_combatant`].
/// Multiplier keys become `1 + bonus`; other stats copy as-is.
pub fn profile_combat_bonuses_to_static_style(map: &HashMap<String, f64>) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (k, &v) in map {
        if !v.is_finite() || v == 0.0 {
            continue;
        }
        if PROFILE_TO_STATIC_MULT_KEYS.contains(&k.as_str()) {
            out.insert(k.clone(), 1.0 + v);
        } else {
            out.insert(k.clone(), v);
        }
    }
    out
}

/// Combat bonuses from imported rows whose `rid` is in `rids` (catalog + synced levels).
pub fn combat_research_bonuses_for_rid_subset(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    rids: &[i64],
) -> HashMap<String, f64> {
    let only: Vec<ResearchEntry> = imported_research
        .iter()
        .filter(|e| rids.contains(&e.rid))
        .cloned()
        .collect();
    combat_research_bonuses_from_entries_slice(&only, catalog, None)
}

fn combat_research_bonuses_from_entries_slice(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> HashMap<String, f64> {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return HashMap::new();
    }

    let mut levels_by_rid = research_levels_by_rid_from_import(imported_research);
    if let Some(exc) = exclude_catalog_rids {
        levels_by_rid.retain(|rid, _| !exc.contains(rid));
    }
    if levels_by_rid.is_empty() {
        return HashMap::new();
    }

    let records: Vec<&crate::data::research::ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid))
        .collect();
    if records.is_empty() {
        return HashMap::new();
    }

    let bonuses = cumulative_research_bonuses(&records, &levels_by_rid);
    let mut out: HashMap<String, f64> = HashMap::new();
    for (stat, value) in bonuses {
        let Some(key) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        let current = out.get(key).copied().unwrap_or(0.0);
        out.insert(key.to_string(), current + value);
    }
    out
}

fn combat_research_owner_faction_bonuses_from_entries_slice(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> HashMap<String, HashMap<String, f64>> {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return HashMap::new();
    }

    let mut levels_by_rid = research_levels_by_rid_from_import(imported_research);
    if let Some(exc) = exclude_catalog_rids {
        levels_by_rid.retain(|rid, _| !exc.contains(rid));
    }
    if levels_by_rid.is_empty() {
        return HashMap::new();
    }

    let records: Vec<&crate::data::research::ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid))
        .collect();
    if records.is_empty() {
        return HashMap::new();
    }

    let nested = cumulative_research_owner_faction_bonuses(&records, &levels_by_rid);
    let mut out: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (faction, inner) in nested {
        let dest = out.entry(faction).or_default();
        for (stat, value) in inner {
            let Some(key) = normalize_profile_combat_stat(&stat) else {
                continue;
            };
            let current = dest.get(key).copied().unwrap_or(0.0);
            dest.insert(key.to_string(), current + value);
        }
    }
    out
}

fn combat_research_conditional_bonuses_from_entries_slice(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> Vec<(
    crate::data::research::ResearchBonusConditionKey,
    String,
    f64,
)> {
    use crate::data::research::cumulative_conditional_research_bonuses;

    if imported_research.is_empty() || catalog.items.is_empty() {
        return Vec::new();
    }

    let mut levels_by_rid = research_levels_by_rid_from_import(imported_research);
    if let Some(exc) = exclude_catalog_rids {
        levels_by_rid.retain(|rid, _| !exc.contains(rid));
    }
    if levels_by_rid.is_empty() {
        return Vec::new();
    }

    let records: Vec<&crate::data::research::ResearchRecord> = catalog
        .items
        .iter()
        .filter(|r| levels_by_rid.contains_key(&r.rid))
        .collect();
    if records.is_empty() {
        return Vec::new();
    }

    let merged = cumulative_conditional_research_bonuses(&records, &levels_by_rid);
    let mut out: Vec<(
        crate::data::research::ResearchBonusConditionKey,
        String,
        f64,
    )> = Vec::with_capacity(merged.len());
    for ((key, stat), value) in merged {
        let Some(norm) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        if value == 0.0 {
            continue;
        }
        out.push((key, norm.to_string(), value));
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.total_cmp(&b.2)));
    out
}

/// Conditional research bonuses (attack-phase seats), same import filter as [`combat_research_bonuses_from_import`].
pub fn combat_research_conditional_bonuses_from_import(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> Vec<(
    crate::data::research::ResearchBonusConditionKey,
    String,
    f64,
)> {
    let filtered = research_entries_excluding_support_buff_gated(imported_research);
    combat_research_conditional_bonuses_from_entries_slice(
        filtered.as_ref(),
        catalog,
        exclude_catalog_rids,
    )
}

/// Per-`rid` research level from sync import: duplicate rows use **max** level for that `rid`.
pub(crate) fn research_levels_by_rid_from_import(
    imported_research: &[ResearchEntry],
) -> HashMap<i64, u32> {
    let mut levels_by_rid: HashMap<i64, u32> = HashMap::new();
    for entry in imported_research {
        let level = if entry.level > 0 {
            entry.level.min(i64::from(u32::MAX)) as u32
        } else {
            0
        };
        if level > 0 {
            levels_by_rid
                .entry(entry.rid)
                .and_modify(|e| *e = (*e).max(level))
                .or_insert(level);
        }
    }
    levels_by_rid
}

/// Conditional research rows (hull class, faction, morale, burning, hull breach) for `crit_chance` /
/// `crit_damage`, and conditional `weapon_damage`, become attack-phase ship seats so outbound damage
/// and crit rolls respect gates (see `research.rs`).
///
/// Unconditional `crit_*` and unconditional `weapon_damage` rows stay in
/// [`merge_research_bonuses_into_profile`] / `profile.bonuses`.
pub fn research_derived_attack_phase_seats(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    gates: &SupportBuffResearchGateState,
    canonical_overrides: &std::collections::HashMap<
        i64,
        crate::data::research::ResearchCanonicalOverride,
    >,
) -> Vec<CrewSeatContext> {
    let filtered = research_entries_for_support_buff_gated_attack_phase(imported_research, gates);
    crate::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec(
        filtered.as_ref(),
        catalog,
        canonical_overrides,
    )
}

/// Effective combat stat bonuses from synced research only (engine keys after normalization).
/// Duplicate `rid` rows use the **maximum** synced level for that `rid`.
/// Used by [`merge_research_bonuses_into_profile`] and [`crate::data::research_summary::research_combat_summary_for_profile`].
/// Omits support-buff–gated `rid`s (folded into static buff layers when those buffs are active).
pub fn combat_research_bonuses_from_import(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> HashMap<String, f64> {
    let filtered = research_entries_excluding_support_buff_gated(imported_research);
    combat_research_bonuses_from_entries_slice(filtered.as_ref(), catalog, exclude_catalog_rids)
}

/// Owner-faction-gated research bonuses (same import filter as [`combat_research_bonuses_from_import`]).
pub fn combat_research_owner_faction_bonuses_from_import(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) -> HashMap<String, HashMap<String, f64>> {
    let filtered = research_entries_excluding_support_buff_gated(imported_research);
    combat_research_owner_faction_bonuses_from_entries_slice(
        filtered.as_ref(),
        catalog,
        exclude_catalog_rids,
    )
}

/// Merges combat stat bonuses from player's synced research into `profile.bonuses`.
/// For each imported research entry (rid, level), looks up the catalog by rid and sums
/// cumulative bonuses for levels 1..=level. Only combat stats are applied (same keys as buildings).
/// Duplicate `rid` rows use the **maximum** synced level for that `rid`.
///
/// Never merges support-buff–gated `rid`s (see [`crate::data::support_buffs::augment_static_buffs_with_support_gated_research`]).
///
/// `exclude_catalog_rids`: optional set of `rid`s whose catalog merge is skipped (canonical overrides).
pub fn merge_research_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
    exclude_catalog_rids: Option<&HashSet<i64>>,
) {
    let bonuses =
        combat_research_bonuses_from_import(imported_research, catalog, exclude_catalog_rids);
    for (key, value) in bonuses {
        let current = profile.bonuses.get(&key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key, current + value);
    }
    let owner = combat_research_owner_faction_bonuses_from_import(
        imported_research,
        catalog,
        exclude_catalog_rids,
    );
    for (faction, inner) in owner {
        let dest = profile
            .research_owner_faction_bonuses
            .entry(faction)
            .or_default();
        for (stat, value) in inner {
            let current = dest.get(&stat).copied().unwrap_or(0.0);
            dest.insert(stat, current + value);
        }
    }
}

/// Adds each `(stat, value)` from `raw` into `profile.bonuses` using the same combat-only
/// key normalization as research/building merges (e.g. `armor_pierce` → `pierce`).
pub fn accumulate_combat_only_bonuses_from_raw(
    profile: &mut PlayerProfile,
    raw: &HashMap<String, f64>,
) {
    for (stat, value) in raw {
        let Some(key) = normalize_profile_combat_stat(stat) else {
            continue;
        };
        let current = profile.bonuses.get(key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key.to_string(), current + value);
    }
}

/// Load profile from JSON file. Returns default (empty bonuses) if file missing or invalid.
pub fn load_profile(path: &str) -> PlayerProfile {
    let path = Path::new(path);
    if !path.exists() {
        return PlayerProfile::default();
    }
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        _ => return PlayerProfile::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn get_bonus(profile: &PlayerProfile, key: &str) -> f64 {
    profile.bonuses.get(key).copied().unwrap_or(0.0)
}

fn owner_faction_lookup_key(owner_faction_slug: Option<&str>) -> Option<String> {
    owner_faction_slug
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
}

/// Flat `profile.bonuses[key]` plus [`PlayerProfile::research_owner_faction_bonuses`] for the resolved ship `faction` slug.
pub fn get_bonus_with_owner_faction_research(
    profile: &PlayerProfile,
    key: &str,
    owner_faction_slug: Option<&str>,
) -> f64 {
    let mut v = get_bonus(profile, key);
    if let Some(fk) = owner_faction_lookup_key(owner_faction_slug) {
        if let Some(inner) = profile.research_owner_faction_bonuses.get(&fk) {
            v += inner.get(key).copied().unwrap_or(0.0);
        }
    }
    v
}

/// Apply LCARS/officer static buffs to a Combatant (e.g. from [BuffSet::static_buffs]).
/// Intended for use when building a Combatant from ship/hostile + crew where crew is resolved via
/// [crate::lcars::resolve_crew_to_buff_set]. Keys applied: isolytic_damage, isolytic_defense,
/// apex_shred, apex_barrier,
/// shield_mitigation (additive; shield_mitigation clamped to [0, 1]), weapon_damage (mult to attack),
/// hull_hp, shield_hp (mult), shield_pierce/armor_pierce (add to pierce), crit_chance (add), crit_damage (mult).
/// `accuracy` is **not** applied here: it is folded into [`AttackerStats`] when computing hostile
/// mitigation and pierce-through ([`crate::optimizer::monte_carlo::scenario::effective_attacker_stats_for_mitigation`]).
pub fn apply_static_buffs_to_combatant(
    combatant: Combatant,
    static_buffs: &HashMap<String, f64>,
) -> Combatant {
    if static_buffs.is_empty() {
        return combatant;
    }
    let isolytic_damage_add = static_buffs.get("isolytic_damage").copied().unwrap_or(0.0);
    let isolytic_defense_add = static_buffs.get("isolytic_defense").copied().unwrap_or(0.0);
    let apex_shred_add = static_buffs.get("apex_shred").copied().unwrap_or(0.0);
    let apex_barrier_add = static_buffs.get("apex_barrier").copied().unwrap_or(0.0);
    let shield_mitigation_add = static_buffs
        .get("shield_mitigation")
        .copied()
        .unwrap_or(0.0);
    let weapon_mult = static_buffs.get("weapon_damage").copied().unwrap_or(1.0);
    let hull_mult = static_buffs.get("hull_hp").copied().unwrap_or(1.0);
    let shield_mult = static_buffs.get("shield_hp").copied().unwrap_or(1.0);
    let pierce_add = static_buffs.get("shield_pierce").copied().unwrap_or(0.0)
        + static_buffs.get("armor_pierce").copied().unwrap_or(0.0);
    let crit_chance_add = static_buffs.get("crit_chance").copied().unwrap_or(0.0);
    let crit_damage_mult = static_buffs.get("crit_damage").copied().unwrap_or(1.0);
    let armor_add = static_buffs.get("armor").copied().unwrap_or(0.0);
    let damage_reduction_add = static_buffs.get("damage_reduction").copied().unwrap_or(0.0);
    let dodge_add = static_buffs.get("dodge").copied().unwrap_or(0.0);
    let shield_deflection_add = static_buffs
        .get("shield_deflection")
        .copied()
        .unwrap_or(0.0);

    Combatant {
        attack: combatant.attack * weapon_mult,
        hull_health: combatant.hull_health * hull_mult,
        shield_health: combatant.shield_health * shield_mult,
        pierce: (combatant.pierce + pierce_add).max(0.0),
        crit_chance: (combatant.crit_chance + crit_chance_add).clamp(0.0, 1.0),
        crit_multiplier: (combatant.crit_multiplier * crit_damage_mult).max(0.0),
        crit_damage_floor: 0.0,
        isolytic_damage: (combatant.isolytic_damage + isolytic_damage_add).max(0.0),
        isolytic_defense: (combatant.isolytic_defense + isolytic_defense_add).max(0.0),
        apex_shred: (combatant.apex_shred + apex_shred_add).max(0.0),
        apex_barrier: (combatant.apex_barrier + apex_barrier_add).max(0.0),
        shield_mitigation: (combatant.shield_mitigation + shield_mitigation_add).clamp(0.0, 1.0),
        mitigation: (combatant.mitigation
            + armor_add
            + damage_reduction_add
            + shield_deflection_add
            + dodge_add)
            .clamp(0.0, 1.0),
        ..combatant
    }
}

/// Scales ship-derived [`AttackerStats::accuracy`] by profile research/building/FT bonuses.
/// `profile.bonuses["accuracy"]` is a **fractional** increase (catalog convention: `0.1` ⇒ ×1.1), aligned with
/// `weapon_damage` semantics. Officer flat accuracy in `scenario_to_combat_input` is applied **after** this.
pub fn apply_profile_accuracy_to_attacker_stats(
    stats: &mut AttackerStats,
    profile: &PlayerProfile,
) {
    let b = profile.bonuses.get("accuracy").copied().unwrap_or(0.0);
    if b != 0.0 {
        stats.accuracy *= 1.0 + b.max(-0.999);
    }
}

/// Estimate effective officer Attack/Defense/Health after merged profile combat bonuses.
///
/// Compounds officer-stat bonuses with ship-level research/building bonuses:
/// - Attack: `base * (1 + weapon_damage) * (1 + officer_attack)`
/// - Defense: `base * (1 + shield_mitigation + officer_defense)` (additive mitigation fractions)
/// - Health: `base * (1 + hull_hp) * (1 + officer_health)`
pub fn estimate_officer_stats_with_profile_bonuses(
    base_attack: f64,
    base_defense: f64,
    base_health: f64,
    profile: &PlayerProfile,
) -> (f64, f64, f64) {
    let attack = (base_attack
        * (1.0 + get_bonus(profile, "weapon_damage"))
        * (1.0 + get_bonus(profile, "officer_attack")))
    .max(0.0);
    let defense = (base_defense
        * (1.0 + get_bonus(profile, "shield_mitigation") + get_bonus(profile, "officer_defense")))
    .max(0.0);
    let health = (base_health
        * (1.0 + get_bonus(profile, "hull_hp"))
        * (1.0 + get_bonus(profile, "officer_health")))
    .max(0.0);
    (attack, defense, health)
}

/// Precomputed officer-stat runtime contribution, derived once from
/// [`crate::lcars::resolver::BuffSet::officer_stat_totals`] + ship breakpoint table + profile
/// pre-aggregation multipliers, then handed to [`apply_profile_to_attacker`] for application.
///
/// `Default` = all zeros = no officer-stat contribution; safe for callers without crew context
/// (bare CLI `simulate`, tests that don't exercise the runtime path).
///
/// See `docs/OFFICER_STAT_FORMULA.md` for the empirical derivation (Sesha L15 / Chen / Ghrush L30
/// observations on the Cerritos, and the Realta T4 L20 + Ghrush experiment that pinned
/// `attack_bonus` as the sole attack-channel mechanism).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OfficerStatRuntimeBonus {
    /// Step-function bonus from `attack_rating`. `(1 + attack_bonus)` multiplies weapon damage.
    pub attack_bonus: f64,
    /// Step-function bonus from `health_rating`. `(1 + health_bonus)` multiplies hull AND shield HP.
    pub health_bonus: f64,
    /// Defense-channel additive contributions, already routed by ship class (§2c):
    /// battleship → armor; explorer → shield_deflection; interceptor → dodge;
    /// survey → ⅓ to each channel. At most one (or three for survey) is non-zero per call.
    pub defense_armor_add: f64,
    pub defense_shield_deflection_add: f64,
    pub defense_dodge_add: f64,
}

/// Fight-setup context used to evaluate static conditions on
/// [`crate::lcars::resolver::PendingOfficerStatContribution`] entries (§3 of
/// `docs/OFFICER_STAT_FORMULA.md`, Phase 4b). All fields default to "unknown"; conditions that
/// depend on an unknown field evaluate to `None` (undecidable) and the contribution is dropped.
///
/// Dynamic conditions (round state, morale procs, hull breach state, …) are intentionally
/// outside this struct's scope — they require per-round evaluation and are deferred to a
/// future runtime path.
#[derive(Debug, Clone, Default)]
pub struct OfficerStatConditionContext {
    /// Attacker's ship class slug (`battleship` / `explorer` / `interceptor` / `survey`).
    /// Powers `AttackerShipTypeIs`.
    pub attacker_ship_class: Option<String>,
    /// Attacker's canonical ship id (e.g. `uss_cerritos`). Powers `AttackerShipIdIs`.
    pub attacker_ship_id: Option<String>,
    /// Attacker hull owner faction slug (`federation` / `klingon` / `romulan`). Powers
    /// `AttackerOwnerFactionIs`.
    pub attacker_owner_faction: Option<String>,
    /// True iff the defender is a player ship (PvP). Powers `DefenderIsPlayerShip` and
    /// (inverted) `DefenderIsNpcHostile`.
    pub defender_is_player_ship: bool,
    /// Defender's ship class slug. Powers `DefenderShipTypeIs`.
    pub defender_ship_type: Option<String>,
    /// Defender hull faction id (numeric upstream id). Powers `DefenderHullFactionIdIs`.
    pub defender_faction_id: Option<i64>,
    /// Defender hull faction slug (`federation` / …). Powers `DefenderFactionIs`.
    pub defender_faction_slug: Option<String>,
    /// Engagement enemy-type tags resolved for this scenario (e.g. `solo_armadas`, `armada`).
    /// Powers `EngagementIncludes`.
    pub engagement_types: Vec<String>,
}

/// Evaluate an [`AbilityConditionSpec`] against fight-setup context.
///
/// Returns:
/// - `Some(true)` — condition holds at fight setup.
/// - `Some(false)` — condition definitely fails (e.g. `LiteralBool { false }`, ship-class
///   mismatch, PvE engagement when `DefenderIsPlayerShip` is required).
/// - `None` — undecidable (variant depends on dynamic state, or required context is missing).
///   Callers should treat `None` the same as `Some(false)`: drop the conditional bonus.
fn eval_static_condition(
    cond: &crate::data::combat_effect_spec::AbilityConditionSpec,
    ctx: &OfficerStatConditionContext,
) -> Option<bool> {
    use crate::data::combat_effect_spec::AbilityConditionSpec as C;
    match cond {
        C::LiteralBool { value } => Some(*value),
        C::AttackerShipTypeIs { ship_type } => ctx
            .attacker_ship_class
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(ship_type)),
        C::DefenderShipTypeIs { ship_type } => ctx
            .defender_ship_type
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(ship_type)),
        C::AttackerShipIdIs { ship_id } => ctx
            .attacker_ship_id
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(ship_id)),
        C::AttackerOwnerFactionIs { faction } => ctx
            .attacker_owner_faction
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(faction)),
        C::DefenderFactionIs { faction } => ctx
            .defender_faction_slug
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(faction)),
        C::DefenderHullFactionIdIs { faction_id } => {
            ctx.defender_faction_id.map(|id| id == *faction_id)
        }
        C::DefenderIsPlayerShip => Some(ctx.defender_is_player_ship),
        C::DefenderIsNpcHostile => Some(!ctx.defender_is_player_ship),
        C::EngagementIncludes { enemy_type } => Some(
            ctx.engagement_types
                .iter()
                .any(|t| t.eq_ignore_ascii_case(enemy_type)),
        ),
        C::Not { inner } => eval_static_condition(inner, ctx).map(|b| !b),
        C::And { all } => {
            // Short-circuit: any definitely-false wins; otherwise propagate any undecidable.
            let mut any_undecidable = false;
            for c in all {
                match eval_static_condition(c, ctx) {
                    Some(false) => return Some(false),
                    None => any_undecidable = true,
                    Some(true) => {}
                }
            }
            if any_undecidable {
                None
            } else {
                Some(true)
            }
        }
        C::Or { any } => {
            let mut any_undecidable = false;
            for c in any {
                match eval_static_condition(c, ctx) {
                    Some(true) => return Some(true),
                    None => any_undecidable = true,
                    Some(false) => {}
                }
            }
            if any_undecidable {
                None
            } else {
                Some(false)
            }
        }
        // Dynamic conditions (round state, morale procs, hull breach, …) cannot be evaluated
        // at fight setup; report as undecidable so the caller drops the conditional bonus
        // until Phase 4d adds per-round evaluation.
        C::MoraleActive
        | C::DefenderBurning
        | C::DefenderHullBreach
        | C::AttackerBurning
        | C::AttackerHullBreach
        | C::DefenderAssimilated
        | C::AttackerOfficerTalNotOnBridge
        | C::RoundRange { .. }
        | C::StatBelow { .. }
        | C::StatAbove { .. }
        | C::CombatBattleTypeAny { .. }
        | C::DefenderLevelAtMost { .. }
        | C::StfcCcToken { .. } => None,
    }
}

/// Compute the [`OfficerStatRuntimeBonus`] for a player ship + crew + profile combination.
/// Performs the full §2 pipeline: per-officer profile multiplier
/// (officer_attack/defense/health) and LCARS ability contributions (officerstatall /
/// officerstathealth from static_buffs) feed into the per-side rating, then a step-function
/// breakpoint lookup, then ship-class channel routing.
///
/// `static_buffs` is the BuffSet's static_buffs map (passive/permanent LCARS contributions).
/// Pass an empty map when no crew. The keys consumed here are `officer_attack` /
/// `officer_defense` / `officer_health` (single-axis officer-stat buffs, §3) and
/// `officer_stat_all` (the synthetic "all three axes" key produced by `officerstatall` tags).
///
#[derive(Default)]
struct OfficerStatAxisMults {
    crew_wide_attack: f64,
    crew_wide_defense: f64,
    crew_wide_health: f64,
    bridge_only_attack: f64,
    bridge_only_defense: f64,
    bridge_only_health: f64,
}

/// Apply pending officer-stat rows whose `target_attacker` matches `want_target_attacker`.
/// When `want_target_attacker` is false (Phase 4c: `target: enemy` debuffs on the opponent's
/// crewed officers), values are negated so positive LCARS magnitudes become rating penalties.
/// `target: enemy_bridge` rows only affect [`OfficerStatAxisMults::bridge_only_*`].
fn apply_pending_officer_stat_contributions(
    mults: &mut OfficerStatAxisMults,
    pending: &[crate::lcars::resolver::PendingOfficerStatContribution],
    want_target_attacker: bool,
    cond_ctx: &OfficerStatConditionContext,
) {
    use crate::lcars::resolver::OfficerStatOpponentScope;
    for c in pending {
        if c.target_attacker != want_target_attacker {
            continue;
        }
        let all_true = c
            .conditions
            .iter()
            .all(|cond| matches!(eval_static_condition(cond, cond_ctx), Some(true)));
        if !all_true {
            continue;
        }
        let v = if want_target_attacker {
            c.value
        } else {
            -c.value
        };
        let bridge_only =
            !want_target_attacker && c.opponent_scope == OfficerStatOpponentScope::BridgeOfficers;
        let add = |attack: &mut f64, defense: &mut f64, health: &mut f64| match c.stat_key.as_str()
        {
            "officer_attack" => *attack += v,
            "officer_defense" => *defense += v,
            "officer_health" => *health += v,
            "officer_stat_all" => {
                *attack += v;
                *defense += v;
                *health += v;
            }
            _ => {}
        };
        if bridge_only {
            add(
                &mut mults.bridge_only_attack,
                &mut mults.bridge_only_defense,
                &mut mults.bridge_only_health,
            );
        } else {
            add(
                &mut mults.crew_wide_attack,
                &mut mults.crew_wide_defense,
                &mut mults.crew_wide_health,
            );
        }
    }
}

fn axis_rating_from_parts(
    bridge_raw: f64,
    below_raw: f64,
    profile_mult: f64,
    crew_mult: f64,
    bridge_only_mult: f64,
) -> f64 {
    bridge_raw * (1.0 + profile_mult + crew_mult + bridge_only_mult)
        + below_raw * (1.0 + profile_mult + crew_mult)
}

/// Returns [`OfficerStatRuntimeBonus::default`] when the ship has no breakpoint table (legacy /
/// hostile-only records); callers do not need to special-case that path.
///
/// `self_pending_contributions`: `target: self` rows from this side's crew (Phase 4b).
/// `opponent_enemy_target_pending`: `target: enemy` rows from the **opponent's** crew (Phase 4c;
/// PvP only — debuffs this side's officer ratings when conditions pass).
///
/// `bridge_totals`: captain + bridge A/D/H subset of `totals`; required for
/// [`crate::lcars::resolver::OfficerStatOpponentScope::BridgeOfficers`] debuffs. When `bridge`
/// equals `totals` (no below-decks officers), bridge-only debuffs still behave correctly.
#[allow(clippy::too_many_arguments)]
pub fn compute_officer_stat_runtime_bonus(
    totals: crate::combat::CrewOfficerStatTotals,
    bridge_totals: crate::combat::CrewOfficerStatTotals,
    ship: &crate::data::ship::ShipRecord,
    profile: &PlayerProfile,
    owner_faction_slug: Option<&str>,
    static_buffs: &HashMap<String, f64>,
    self_pending_contributions: &[crate::lcars::resolver::PendingOfficerStatContribution],
    cond_ctx: &OfficerStatConditionContext,
    opponent_enemy_target_pending: &[crate::lcars::resolver::PendingOfficerStatContribution],
) -> OfficerStatRuntimeBonus {
    if ship.officer_bonus.is_empty() {
        return OfficerStatRuntimeBonus::default();
    }
    let gb = |k: &str| get_bonus_with_owner_faction_research(profile, k, owner_faction_slug);
    // §3 LCARS officerstat* ability contributions: passive/permanent effects with mapped
    // officer-rating axis. Per-axis tags add to their channel; `officer_stat_all` (from
    // `officerstatall` tags) adds to all three axes simultaneously.
    let stat_all = static_buffs.get("officer_stat_all").copied().unwrap_or(0.0);
    let mut mults = OfficerStatAxisMults {
        crew_wide_attack: static_buffs.get("officer_attack").copied().unwrap_or(0.0) + stat_all,
        crew_wide_defense: static_buffs.get("officer_defense").copied().unwrap_or(0.0) + stat_all,
        crew_wide_health: static_buffs.get("officer_health").copied().unwrap_or(0.0) + stat_all,
        ..OfficerStatAxisMults::default()
    };

    // Phase 4b: conditional `target: self` contributions from this crew (crew-wide).
    apply_pending_officer_stat_contributions(
        &mut mults,
        self_pending_contributions,
        true,
        cond_ctx,
    );
    // Phase 4c: `target: enemy` / `enemy_bridge` from the opponent's crew.
    apply_pending_officer_stat_contributions(
        &mut mults,
        opponent_enemy_target_pending,
        false,
        cond_ctx,
    );

    let below_decks = crate::combat::CrewOfficerStatTotals {
        attack: (totals.attack - bridge_totals.attack).max(0.0),
        defense: (totals.defense - bridge_totals.defense).max(0.0),
        health: (totals.health - bridge_totals.health).max(0.0),
    };

    // §2e + §3: per-officer multipliers, then sum. Bridge-only opponent debuffs hit captain +
    // bridge slots only; crew-wide buffs/debuffs hit below decks too.
    let attack_rating = axis_rating_from_parts(
        bridge_totals.attack,
        below_decks.attack,
        gb("officer_attack"),
        mults.crew_wide_attack,
        mults.bridge_only_attack,
    );
    let defense_rating = axis_rating_from_parts(
        bridge_totals.defense,
        below_decks.defense,
        gb("officer_defense"),
        mults.crew_wide_defense,
        mults.bridge_only_defense,
    );
    let health_rating = axis_rating_from_parts(
        bridge_totals.health,
        below_decks.health,
        gb("officer_health"),
        mults.crew_wide_health,
        mults.bridge_only_health,
    );

    let attack_bonus = ship.officer_bonus.attack_bonus(attack_rating);
    let defense_bonus = ship.officer_bonus.defense_bonus(defense_rating);
    let health_bonus = ship.officer_bonus.health_bonus(health_rating);

    // §2c: route defense_bonus × ship-channel-constant to the ship-class primary mitigation stat.
    let class = ship.ship_class.trim().to_ascii_lowercase();
    let (defense_armor_add, defense_shield_deflection_add, defense_dodge_add) = match class.as_str()
    {
        "battleship" => (ship.armor * defense_bonus, 0.0, 0.0),
        "explorer" => (0.0, ship.shield_deflection * defense_bonus, 0.0),
        "interceptor" => (0.0, 0.0, ship.dodge * defense_bonus),
        "survey" => (
            ship.armor * defense_bonus / 3.0,
            ship.shield_deflection * defense_bonus / 3.0,
            ship.dodge * defense_bonus / 3.0,
        ),
        _ => (0.0, 0.0, 0.0),
    };

    OfficerStatRuntimeBonus {
        attack_bonus,
        health_bonus,
        defense_armor_add,
        defense_shield_deflection_add,
        defense_dodge_add,
    }
}

/// Apply effective_bonuses to attacker Combatant (multipliers and additive bonuses).
///
/// Officer-stat contributions (§2 of `docs/OFFICER_STAT_FORMULA.md`) are passed in precomputed via
/// `officer_stat_runtime`; pass [`OfficerStatRuntimeBonus::default`] when there is no crew (bare
/// CLI simulate, tests without crew). The legacy `officer_attack` / `officer_health` /
/// `officer_defense` profile keys are consumed inside [`compute_officer_stat_runtime_bonus`] as
/// pre-aggregation multipliers on per-officer A/D/H, and are intentionally NOT applied here as
/// post-aggregation ship-stat multipliers (the §4 migration).
///
/// `owner_faction_slug`: lowercase-trimmed [`crate::data::ship::ShipRecord::faction`] when known
/// (e.g. `"federation"`); merges [`PlayerProfile::research_owner_faction_bonuses`] for matching
/// keys into the same stats as `profile.bonuses`.
pub fn apply_profile_to_attacker(
    attacker: Combatant,
    profile: &PlayerProfile,
    owner_faction_slug: Option<&str>,
    officer_stat_runtime: OfficerStatRuntimeBonus,
) -> Combatant {
    let osr = officer_stat_runtime;
    let osr_is_zero = osr == OfficerStatRuntimeBonus::default();
    if profile.bonuses.is_empty()
        && profile.research_owner_faction_bonuses.is_empty()
        && osr_is_zero
    {
        return attacker;
    }
    let gb = |k: &str| get_bonus_with_owner_faction_research(profile, k, owner_faction_slug);
    let weapon = 1.0 + gb("weapon_damage");
    let hull_hp = 1.0 + gb("hull_hp");
    let shield_hp = 1.0 + gb("shield_hp");
    let isolytic_damage_add = gb("isolytic_damage");
    let isolytic_defense_add = gb("isolytic_defense");
    let apex_shred_add = gb("apex_shred");
    let apex_barrier_add = gb("apex_barrier");
    let crit_chance_add = gb("crit_chance");
    let crit_damage_mult = 1.0 + gb("crit_damage");
    // Defensive clamp populated by the 4 "Critical Damage Floor" research nodes.
    // Additive across nodes per the upstream catalog operator. Consumed at the per-shot
    // crit-resolution site in `crit.rs` to enforce `effective ≥ floor` AFTER any
    // attacker-outbound crit-damage reduction and BEFORE hull-breach amplification.
    let crit_damage_floor_add = gb("crit_damage_floor");
    let pierce_add = gb("pierce");
    let shield_mit_add = gb("shield_mitigation");
    // §2c: officer Defense does NOT add to shield_mitigation. The previous
    // `shield_mitigation += officer_defense` line was incorrect and has been removed.
    //
    // Mitigation components are tracked separately post-resolution so the engine can apply
    // ship-type coefficients (c_armor, c_shield, c_dodge) in the inbound counter-fire path.
    // `damage_reduction` is a flat post-mitigation reduction not subject to ship-type weighting.
    // The aggregated `mitigation` scalar is preserved as a back-compat fallback for consumers
    // that read a single value (external serialized state, debug traces).
    let armor_add = gb("armor") + osr.defense_armor_add;
    let shield_deflection_add = gb("shield_deflection") + osr.defense_shield_deflection_add;
    let dodge_add = gb("dodge") + osr.defense_dodge_add;
    let damage_reduction_add = gb("damage_reduction");
    let mitigation_add = armor_add + shield_deflection_add + dodge_add + damage_reduction_add;

    // §2b/§2d: officer attack_bonus multiplies weapon damage; officer health_bonus multiplies
    // BOTH hull and shield HP (the §4-migrated semantics — was hull-only before).
    let attack_mult = 1.0 + osr.attack_bonus;
    let health_mult = 1.0 + osr.health_bonus;

    Combatant {
        attack: attacker.attack * weapon * attack_mult,
        hull_health: attacker.hull_health * hull_hp * health_mult,
        shield_health: attacker.shield_health * shield_hp * health_mult,
        crit_chance: (attacker.crit_chance + crit_chance_add).clamp(0.0, 1.0),
        crit_multiplier: (attacker.crit_multiplier * crit_damage_mult).max(0.0),
        crit_damage_floor: (attacker.crit_damage_floor + crit_damage_floor_add).max(0.0),
        pierce: (attacker.pierce + pierce_add).max(0.0),
        mitigation: (attacker.mitigation + mitigation_add).clamp(0.0, 1.0),
        armor: (attacker.armor + armor_add).max(0.0),
        shield_deflection: (attacker.shield_deflection + shield_deflection_add).max(0.0),
        dodge: (attacker.dodge + dodge_add).max(0.0),
        damage_reduction: (attacker.damage_reduction + damage_reduction_add).max(0.0),
        shield_mitigation: (attacker.shield_mitigation + shield_mit_add).clamp(0.0, 1.0),
        isolytic_damage: (attacker.isolytic_damage + isolytic_damage_add).max(0.0),
        isolytic_defense: (attacker.isolytic_defense + isolytic_defense_add).max(0.0),
        apex_shred: (attacker.apex_shred + apex_shred_add).max(0.0),
        apex_barrier: (attacker.apex_barrier + apex_barrier_add).max(0.0),
        ..attacker
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::combat::{AttackerStats, Combatant, ShipType};
    use crate::data::building::{
        BuildingBonusContext, BuildingIndex, BuildingIndexEntry, BuildingMode,
    };
    use crate::data::forbidden_chaos::{BonusEntry, ForbiddenChaosList, ForbiddenChaosRecord};
    use crate::data::import::BuildingEntry;
    use crate::data::import::ForbiddenTechEntry;
    use crate::data::ship::ShipRecord;

    use super::*;

    #[test]
    fn normalize_profile_combat_stat_maps_isolytic_cascade_aliases() {
        assert_eq!(
            normalize_profile_combat_stat("isolytic_cascade"),
            Some("isolytic_cascade_damage")
        );
        assert_eq!(
            normalize_profile_combat_stat("isolytic_cascade_damage"),
            Some("isolytic_cascade_damage")
        );
    }

    #[test]
    fn normalize_profile_combat_stat_keeps_officer_stats_distinct() {
        assert_eq!(
            normalize_profile_combat_stat("officer_attack"),
            Some("officer_attack")
        );
        assert_eq!(
            normalize_profile_combat_stat("officer_defense"),
            Some("officer_defense")
        );
        assert_eq!(
            normalize_profile_combat_stat("officer_health"),
            Some("officer_health")
        );
    }

    #[test]
    fn load_profile_round_trips_support_buffs() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("profile_support_buffs_{nanos}.json"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
                "bonuses": {"weapon_damage": 0.1},
                "support_buffs": ["cerritos_support", "defiant_reinforce"]
            }"#,
        )
        .unwrap();

        let profile = load_profile(path.to_string_lossy().as_ref());
        let serialized = serde_json::to_string(&profile).unwrap();
        let reloaded: PlayerProfile = serde_json::from_str(&serialized).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            reloaded.support_buffs,
            vec![
                "cerritos_support".to_string(),
                "defiant_reinforce".to_string()
            ]
        );
        assert_eq!(reloaded.bonuses.get("weapon_damage"), Some(&0.1));
    }

    #[test]
    fn forbidden_tech_bonus_combat_route_classifies_known_fids() {
        use ForbiddenTechBonusCombatRoute as R;

        assert_eq!(
            forbidden_tech_bonus_combat_route(BORG_ALCOVE_FORBIDDEN_TECH_FID, "crit_chance"),
            Some(R::TimedSeat)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(
                BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID,
                "hostile_crit_damage_reduction"
            ),
            Some(R::TimedSeat)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(
                QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID,
                "shield_mitigation"
            ),
            Some(R::TimedSeat)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(S31_TORPEDO_PODS_FORBIDDEN_TECH_FID, "weapon_damage"),
            Some(R::TimedSeat)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(S31_TORPEDO_PODS_FORBIDDEN_TECH_FID, "hull_hp"),
            Some(R::HullOrAccuracyScalar)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(999_999_999, "weapon_damage"),
            Some(R::ProfileFlat)
        );
        assert_eq!(
            forbidden_tech_bonus_combat_route(BORG_ALCOVE_FORBIDDEN_TECH_FID, "weapon_damage"),
            None
        );
    }

    #[test]
    fn merge_building_bonuses_into_profile_adds_only_combat_keys() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 1, level: 1 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(1i64, "test_weapon_building".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "test_weapon_building".to_string(),
                building_name: "Test".to_string(),
                file: None,
                bid: Some(1),
            }],
        };
        let data_dir = std::env::temp_dir().join("kobayashi_profile_building_test");
        let _ = std::fs::create_dir_all(&data_dir);
        let building_json = r#"{
            "id": "test_weapon_building",
            "building_name": "Test",
            "levels": [{
                "level": 1,
                "bonuses": [
                    {"stat": "weapon_damage", "value": 0.05, "operator": "add"},
                    {"stat": "buff_123", "value": 1.0, "operator": "add"}
                ]
            }]
        }"#;
        std::fs::write(data_dir.join("test_weapon_building.json"), building_json).unwrap();

        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext::default(),
        );

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.05));
        assert!(!profile.bonuses.contains_key("buff_123"));
    }

    #[test]
    fn merge_building_bonuses_into_profile_maps_pierce_and_mitigation_stats() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 1, level: 1 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(1i64, "test_weapon_building".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "test_weapon_building".to_string(),
                building_name: "Test".to_string(),
                file: None,
                bid: Some(1),
            }],
        };
        let data_dir = std::env::temp_dir().join("kobayashi_profile_building_test_stats");
        let _ = std::fs::create_dir_all(&data_dir);
        let building_json = r#"{
            "id": "test_weapon_building",
            "building_name": "Test",
            "levels": [{
                "level": 1,
                "bonuses": [
                    {"stat": "armor_pierce", "value": 0.07, "operator": "add"},
                    {"stat": "shield_pierce", "value": 0.03, "operator": "add"},
                    {"stat": "armor", "value": 0.04, "operator": "add"},
                    {"stat": "dodge", "value": 0.05, "operator": "add"},
                    {"stat": "damage_reduction", "value": 0.06, "operator": "add"}
                ]
            }]
        }"#;
        std::fs::write(data_dir.join("test_weapon_building.json"), building_json).unwrap();

        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext {
                ops_level: Some(30),
                mode: BuildingMode::ShipCombat,
                defender_opponent: crate::data::building::BuildingDefenderOpponent::Unknown,
                attacker_faction: crate::data::building::BuildingAttackerFaction::Unknown,
                attacker_tal_assigned_captain_or_bridge: false,
                attacker_ship_type: None,
            },
        );

        assert_eq!(profile.bonuses.get("pierce"), Some(&0.10));
        assert_eq!(profile.bonuses.get("armor"), Some(&0.04));
        assert_eq!(profile.bonuses.get("dodge"), Some(&0.05));
        assert_eq!(profile.bonuses.get("damage_reduction"), Some(&0.06));
    }

    #[test]
    fn merge_research_bonuses_into_profile_adds_only_combat_keys() {
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let mut profile = PlayerProfile::default();
        let imported_research = vec![ResearchEntry { rid: 1, level: 1 }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 1,
                name: Some("Combat I".to_string()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![
                        ResearchBonusEntry {
                            stat: "weapon_damage".to_string(),
                            value: 0.05,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        },
                        ResearchBonusEntry {
                            stat: "buff_unknown".to_string(),
                            value: 1.0,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        },
                    ],
                }],
            }],
        };
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog, None);
        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.05));
        assert!(!profile.bonuses.contains_key("buff_unknown"));
    }

    #[test]
    fn merge_research_bonuses_into_profile_merges_apex_stats() {
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let mut profile = PlayerProfile::default();
        let imported_research = vec![ResearchEntry { rid: 42, level: 1 }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 42,
                name: Some("Apex lab".to_string()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![
                        ResearchBonusEntry {
                            stat: "apex_shred".to_string(),
                            value: 0.25,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        },
                        ResearchBonusEntry {
                            stat: "apex_barrier".to_string(),
                            value: 500.0,
                            operator: "add".to_string(),
                            condition: Default::default(),
                        },
                    ],
                }],
            }],
        };
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog, None);
        assert_eq!(profile.bonuses.get("apex_shred"), Some(&0.25));
        assert_eq!(profile.bonuses.get("apex_barrier"), Some(&500.0));
    }

    #[test]
    fn merge_research_bonuses_into_profile_skips_conditional_morale_isolytic_for_flat_bonuses() {
        use crate::data::research::{
            ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel,
            ResearchRecord,
        };

        let imported_research = vec![ResearchEntry {
            rid: 4133019450,
            level: 1,
        }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 4133019450,
                name: Some("NS Morale Isolytic Damage".to_string()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "isolytic_damage".to_string(),
                        value: 0.05,
                        operator: "add".to_string(),
                        condition: ResearchBonusConditionKey {
                            requires_morale: true,
                            ..Default::default()
                        },
                    }],
                }],
            }],
        };
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog, None);
        assert!(
            !profile.bonuses.contains_key("isolytic_damage"),
            "conditional morale isolytic must not flat-merge into profile.bonuses"
        );
    }

    #[test]
    fn merge_research_then_apply_profile_carries_apex_to_combatant() {
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let imported_research = vec![ResearchEntry { rid: 7, level: 1 }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 7,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "apex_shred".to_string(),
                        value: 0.12,
                        operator: "add".to_string(),
                        condition: Default::default(),
                    }],
                }],
            }],
        };
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog, None);

        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.0,
            apex_barrier: 0.0,
            apex_shred: 0.03,
            weapons: vec![],
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        assert!((out.apex_shred - 0.15).abs() < 1e-9, "expected 0.03 + 0.12");
    }

    #[test]
    fn apply_profile_to_attacker_does_not_flat_merge_deprecated_morale_isolytic_profile_key() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.0,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            weapons: vec![],
            isolytic_damage: 2.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let mut profile = PlayerProfile::default();
        profile
            .bonuses
            .insert("isolytic_damage_morale".to_string(), 0.5);
        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        assert!(
            (out.isolytic_damage - 2.0).abs() < 1e-9,
            "morale-gated isolytic must not add to flat Combatant.isolytic_damage"
        );
        assert!((out.apex_shred - 0.0).abs() < 1e-9);
    }

    #[test]
    fn merge_research_bonuses_into_profile_skips_unknown_rid() {
        use crate::data::research::{ResearchCatalog, ResearchRecord};

        let mut profile = PlayerProfile::default();
        let imported_research = vec![ResearchEntry {
            rid: 99999,
            level: 5,
        }];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 1,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![],
            }],
        };
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog, None);
        assert!(profile.bonuses.is_empty());
    }

    #[test]
    fn merge_research_never_merges_titan_fortify_gated_into_profile() {
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };
        let rid = TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS[0];
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid,
                name: Some("Titan's Fist".into()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "weapon_damage".into(),
                        value: 0.07,
                        operator: "add".into(),
                        condition: Default::default(),
                    }],
                }],
            }],
        };
        let imported = vec![ResearchEntry { rid, level: 1 }];
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
        assert!(
            !profile.bonuses.contains_key("weapon_damage"),
            "gated rid must never merge into profile.bonuses"
        );
        let gated = combat_research_bonuses_for_rid_subset(
            &imported,
            &catalog,
            TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS,
        );
        assert_eq!(gated.get("weapon_damage"), Some(&0.07));
        let static_style = profile_combat_bonuses_to_static_style(&gated);
        assert_eq!(static_style.get("weapon_damage"), Some(&1.07));
    }

    #[test]
    fn merge_research_owner_faction_does_not_flatten_into_bonuses() {
        use crate::data::research::{
            ResearchBonusConditionKey, ResearchBonusEntry, ResearchCatalog, ResearchLevel,
            ResearchRecord,
        };
        let rid = 9002_i64;
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![ResearchRecord {
                rid,
                name: Some("Fed gate".into()),
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "shield_deflection".into(),
                        value: 0.06,
                        operator: "add".into(),
                        condition: ResearchBonusConditionKey {
                            attacker_faction: Some("federation".into()),
                            ..Default::default()
                        },
                    }],
                }],
            }],
        };
        let imported = vec![ResearchEntry { rid, level: 1 }];
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);
        assert!(
            !profile.bonuses.contains_key("shield_deflection"),
            "owner-faction-gated research must not merge into profile.bonuses"
        );
        assert_eq!(
            profile
                .research_owner_faction_bonuses
                .get("federation")
                .and_then(|m| m.get("shield_deflection"))
                .copied(),
            Some(0.06)
        );
    }

    #[test]
    fn apply_profile_owner_faction_shield_deflection_only_when_ship_faction_matches() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: vec![],
            hostile_mitigation_params: None,
        };
        let mut profile = PlayerProfile::default();
        profile
            .research_owner_faction_bonuses
            .entry("federation".to_string())
            .or_default()
            .insert("shield_deflection".to_string(), 0.05);

        let out_fed = apply_profile_to_attacker(
            attacker.clone(),
            &profile,
            Some("federation"),
            OfficerStatRuntimeBonus::default(),
        );
        assert!((out_fed.mitigation - 0.05).abs() < 1e-9);
        let out_klg = apply_profile_to_attacker(
            attacker.clone(),
            &profile,
            Some("klingon"),
            OfficerStatRuntimeBonus::default(),
        );
        assert!((out_klg.mitigation).abs() < 1e-9);
        let out_ci = apply_profile_to_attacker(
            attacker.clone(),
            &profile,
            Some("Federation"),
            OfficerStatRuntimeBonus::default(),
        );
        assert!((out_ci.mitigation - 0.05).abs() < 1e-9);
        let out_none =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        assert!((out_none.mitigation).abs() < 1e-9);
    }

    fn combatant_with(
        isolytic_damage: f64,
        isolytic_defense: f64,
        shield_mitigation: f64,
    ) -> Combatant {
        Combatant {
            id: "test".to_string(),
            attack: 0.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage,
            isolytic_defense,
            hostile_mitigation_params: None,
        }
    }

    #[test]
    fn apply_static_buffs_to_combatant_applies_and_clamps() {
        let c = combatant_with(0.0, 0.0, 0.5);
        let mut buffs = HashMap::new();
        buffs.insert("isolytic_damage".to_string(), 0.1);
        buffs.insert("isolytic_defense".to_string(), 10.0);
        buffs.insert("shield_mitigation".to_string(), 0.3);
        let out = apply_static_buffs_to_combatant(c, &buffs);
        assert_eq!(out.isolytic_damage, 0.1);
        assert_eq!(out.isolytic_defense, 10.0);
        assert_eq!(out.shield_mitigation, 0.8);

        let c2 = combatant_with(0.0, 0.0, 0.9);
        let mut buffs2 = HashMap::new();
        buffs2.insert("shield_mitigation".to_string(), 0.5);
        let out2 = apply_static_buffs_to_combatant(c2, &buffs2);
        assert_eq!(
            out2.shield_mitigation, 1.0,
            "shield_mitigation should clamp to 1.0"
        );
    }

    #[test]
    fn apply_profile_to_attacker_adds_apex_from_profile() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.8,
            apex_barrier: 100.0,
            apex_shred: 0.1,
            weapons: vec![],
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("apex_shred".to_string(), 0.15);
        profile.bonuses.insert("apex_barrier".to_string(), 200.0);
        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        assert!((out.apex_shred - 0.25).abs() < 1e-9);
        assert!((out.apex_barrier - 300.0).abs() < 1e-9);
    }

    #[test]
    fn apply_profile_to_attacker_applies_mitigation_stats() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.10,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.05,
            crit_chance: 0.10,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.2,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("armor".to_string(), 0.04);
        profile.bonuses.insert("dodge".to_string(), 0.03);
        profile.bonuses.insert("damage_reduction".to_string(), 0.02);

        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        // Aggregated scalar: attacker.mitigation (0.10 baseline) + sum of component adds (0.09).
        assert!((out.mitigation - 0.19).abs() < 1e-9);
        // Each component populated separately from its own profile key. The attacker started
        // with armor=0, shield_deflection=0, dodge=0, damage_reduction=0, so each post-resolution
        // value equals just the profile bonus add.
        assert!((out.armor - 0.04).abs() < 1e-9, "armor from profile");
        assert!(
            (out.shield_deflection - 0.0).abs() < 1e-9,
            "shield_deflection had no contributor"
        );
        assert!((out.dodge - 0.03).abs() < 1e-9, "dodge from profile");
        assert!(
            (out.damage_reduction - 0.02).abs() < 1e-9,
            "damage_reduction from profile"
        );
    }

    #[test]
    fn apply_profile_to_attacker_routes_components_with_officer_runtime() {
        // Officer Defense bonuses (defense_armor_add, defense_shield_deflection_add,
        // defense_dodge_add) add to the matching components, not just the aggregate.
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.2,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let profile = PlayerProfile::default();
        let osr = OfficerStatRuntimeBonus {
            defense_armor_add: 0.05,
            defense_shield_deflection_add: 0.04,
            defense_dodge_add: 0.03,
            ..OfficerStatRuntimeBonus::default()
        };
        let out = apply_profile_to_attacker(attacker, &profile, None, osr);
        assert!((out.armor - 0.05).abs() < 1e-9);
        assert!((out.shield_deflection - 0.04).abs() < 1e-9);
        assert!((out.dodge - 0.03).abs() < 1e-9);
        assert!((out.damage_reduction - 0.0).abs() < 1e-9);
        // Aggregate scalar tracks the sum.
        assert!((out.mitigation - 0.12).abs() < 1e-9);
    }

    #[test]
    fn apply_profile_to_attacker_routes_crit_damage_floor() {
        // 4 floor research nodes contribute additively to a single `crit_damage_floor`
        // profile key. Verify the resolved Combatant.crit_damage_floor reflects the sum.
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 2.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.0,
            apex_barrier: 0.0,
            weapons: vec![],
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };
        let mut profile = PlayerProfile::default();
        // Sum of two floor research nodes: +0.5 floor.
        profile.bonuses.insert("crit_damage_floor".to_string(), 0.5);
        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        assert!(
            (out.crit_damage_floor - 0.5).abs() < 1e-9,
            "crit_damage_floor: expected 0.5, got {}",
            out.crit_damage_floor
        );
        // `crit_damage` profile key is untouched (regression guard against accidental
        // routing of floor research to base crit damage).
        assert!(
            (out.crit_multiplier - 2.0).abs() < 1e-9,
            "crit_multiplier should not have been touched by crit_damage_floor"
        );
    }

    #[test]
    fn apply_profile_accuracy_to_attacker_stats_multiplies_base() {
        let mut stats = AttackerStats {
            armor_piercing: 100.0,
            shield_piercing: 100.0,
            accuracy: 200.0,
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("accuracy".to_string(), 0.1);
        apply_profile_accuracy_to_attacker_stats(&mut stats, &profile);
        assert!((stats.accuracy - 220.0).abs() < 1e-9);
        assert!((stats.armor_piercing - 100.0).abs() < 1e-9);
    }

    #[test]
    fn estimate_officer_stats_neelix_l30_t5_stays_below_upper_bounds_with_slack() {
        // User-provided live-game anchors (treated as practical ceilings due to possible profile lag).
        let provided_attack = 39_549.0;
        let provided_defense = 36_262.0;
        let provided_health = 35_473.0;
        let lower_bound_attack = 731.0;
        let lower_bound_defense = 658.0;
        let lower_bound_health = 658.0;

        // Conservative imported baseline (slightly stale) before profile combat bonuses.
        let base_attack = provided_attack * 0.94;
        let base_defense = provided_defense * 0.94;
        let base_health = provided_health * 0.94;

        let mut profile = PlayerProfile::default();
        let mut raw = HashMap::new();
        raw.insert("officer_attack".to_string(), 0.01);
        raw.insert("officer_defense".to_string(), 0.015);
        raw.insert("officer_health".to_string(), 0.02);
        accumulate_combat_only_bonuses_from_raw(&mut profile, &raw);

        let (modeled_attack, modeled_defense, modeled_health) =
            estimate_officer_stats_with_profile_bonuses(
                base_attack,
                base_defense,
                base_health,
                &profile,
            );

        assert!(modeled_attack < provided_attack);
        assert!(modeled_defense < provided_defense);
        assert!(modeled_health < provided_health);
        assert!(modeled_attack >= lower_bound_attack);
        assert!(modeled_defense >= lower_bound_defense);
        assert!(modeled_health >= lower_bound_health);

        // Keep calibration within a bounded "slightly lower" window to avoid brittle expectations.
        let tolerance = 0.10;
        assert!((provided_attack - modeled_attack) / provided_attack <= tolerance);
        assert!((provided_defense - modeled_defense) / provided_defense <= tolerance);
        assert!((provided_health - modeled_health) / provided_health <= tolerance);
    }

    #[test]
    fn officer_stat_profile_keys_are_no_op_without_crew_runtime() {
        // §4 of docs/OFFICER_STAT_FORMULA.md: officer_attack / officer_defense / officer_health
        // profile keys migrated from post-aggregation ship-stat multipliers (legacy) to
        // pre-aggregation per-officer multipliers (new). They are consumed inside
        // `compute_officer_stat_runtime_bonus` (which requires crew + ship breakpoint table),
        // and no longer applied here. With no crew (default OfficerStatRuntimeBonus) they
        // produce zero effect — only the non-officer profile keys (weapon_damage, hull_hp,
        // shield_mitigation) are applied. The previously incorrect
        // `shield_mitigation += officer_defense` line was also removed (§2c).
        let mut profile = PlayerProfile::default();
        let mut raw = HashMap::new();
        raw.insert("weapon_damage".to_string(), 0.10);
        raw.insert("officer_attack".to_string(), 0.10);
        raw.insert("shield_mitigation".to_string(), 0.05);
        raw.insert("officer_defense".to_string(), 0.05);
        raw.insert("hull_hp".to_string(), 0.20);
        raw.insert("officer_health".to_string(), 0.20);
        accumulate_combat_only_bonuses_from_raw(&mut profile, &raw);

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.10));
        assert_eq!(profile.bonuses.get("officer_attack"), Some(&0.10));
        assert_eq!(profile.bonuses.get("shield_mitigation"), Some(&0.05));
        assert_eq!(profile.bonuses.get("officer_defense"), Some(&0.05));
        assert_eq!(profile.bonuses.get("hull_hp"), Some(&0.20));
        assert_eq!(profile.bonuses.get("officer_health"), Some(&0.20));

        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.10,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.20,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: vec![],
            hostile_mitigation_params: None,
        };
        let out =
            apply_profile_to_attacker(attacker, &profile, None, OfficerStatRuntimeBonus::default());
        // attack: 100 × 1.10 (weapon_damage only; officer_attack no-op without crew) = 110
        // hull:   1000 × 1.20 (hull_hp only; officer_health no-op without crew)       = 1200
        // shield_mitigation: 0.20 + 0.05 (shield_mitigation only; officer_defense removed) = 0.25
        assert!(
            (out.attack - 110.0).abs() < 1e-9,
            "attack = {} (expected 110.0)",
            out.attack
        );
        assert!(
            (out.hull_health - 1200.0).abs() < 1e-9,
            "hull_health = {} (expected 1200.0)",
            out.hull_health
        );
        assert!(
            (out.shield_mitigation - 0.25).abs() < 1e-9,
            "shield_mitigation = {} (expected 0.25)",
            out.shield_mitigation
        );
    }

    #[test]
    fn officer_stat_runtime_bonus_compounds_into_attacker_math() {
        // Companion to the test above: when officer-stat runtime IS provided (i.e. crew + ship
        // breakpoint table are present), the attack/defense/health bonuses apply as multipliers
        // and channel-routed additives per §2 of docs/OFFICER_STAT_FORMULA.md.
        let mut profile = PlayerProfile::default();
        let mut raw = HashMap::new();
        raw.insert("weapon_damage".to_string(), 0.10);
        raw.insert("hull_hp".to_string(), 0.20);
        raw.insert("shield_mitigation".to_string(), 0.05);
        accumulate_combat_only_bonuses_from_raw(&mut profile, &raw);

        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.10,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 500.0,
            shield_mitigation: 0.20,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: vec![],
            hostile_mitigation_params: None,
        };
        let osr = OfficerStatRuntimeBonus {
            attack_bonus: 0.5, // 50% from a breakpoint
            health_bonus: 1.0, // 100% — multiplies both hull and shield (§2d)
            defense_armor_add: 0.0,
            defense_shield_deflection_add: 0.4, // explorer-routed defense additive
            defense_dodge_add: 0.0,
        };
        let out = apply_profile_to_attacker(attacker, &profile, None, osr);
        // attack: 100 × (1 + 0.10) × (1 + 0.50)                 = 165
        // hull:   1000 × (1 + 0.20) × (1 + 1.00)                = 2400
        // shield_hp (new — health_mult on shield too): 500 × (1 + 0) × (1 + 1.00) = 1000
        // mitigation: 0.10 + 0 (armor + add) + 0.40 (shield_defl add) + 0 (dodge add) = 0.50
        // shield_mitigation: 0.20 + 0.05 = 0.25 (officer_defense add removed)
        assert!((out.attack - 165.0).abs() < 1e-9, "attack = {}", out.attack);
        assert!(
            (out.hull_health - 2400.0).abs() < 1e-9,
            "hull_health = {}",
            out.hull_health
        );
        assert!(
            (out.shield_health - 1000.0).abs() < 1e-9,
            "shield_health = {}",
            out.shield_health
        );
        assert!(
            (out.mitigation - 0.50).abs() < 1e-9,
            "mitigation = {}",
            out.mitigation
        );
        assert!(
            (out.shield_mitigation - 0.25).abs() < 1e-9,
            "shield_mitigation = {}",
            out.shield_mitigation
        );
    }

    #[test]
    fn compute_runtime_bonus_consumes_officer_stat_all_static_buff() {
        // §3: a passive `officerstatall +X` LCARS effect produces a static_buffs entry under
        // `officer_stat_all`. `compute_officer_stat_runtime_bonus` reads this and adds it to
        // EACH per-axis multiplier (attack / defense / health), boosting the rating before the
        // breakpoint lookup.
        use crate::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
        let ship = ShipRecord {
            ship_class: "explorer".to_string(),
            armor: 100.0,
            shield_deflection: 1000.0,
            dodge: 100.0,
            officer_bonus: OfficerBonusTable {
                attack: vec![
                    OfficerBonusBreakpoint {
                        value: 1000.0,
                        bonus: 0.5,
                    },
                    OfficerBonusBreakpoint {
                        value: 2000.0,
                        bonus: 1.0,
                    },
                ],
                defense: vec![
                    OfficerBonusBreakpoint {
                        value: 1000.0,
                        bonus: 0.5,
                    },
                    OfficerBonusBreakpoint {
                        value: 2000.0,
                        bonus: 1.0,
                    },
                ],
                health: vec![
                    OfficerBonusBreakpoint {
                        value: 1000.0,
                        bonus: 0.5,
                    },
                    OfficerBonusBreakpoint {
                        value: 2000.0,
                        bonus: 1.0,
                    },
                ],
            },
            ..Default::default()
        };
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 1800.0,
            health: 1800.0,
        };
        // Without ability buff: rating = 1800, attack_bonus = 0.5 (first breakpoint).
        let static_buffs_none = HashMap::<String, f64>::new();
        let pending_none: Vec<crate::lcars::resolver::PendingOfficerStatContribution> = Vec::new();
        let ctx_default = OfficerStatConditionContext::default();
        let osr_no_ability = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs_none,
            &pending_none,
            &ctx_default,
            &[],
        );
        assert!(
            (osr_no_ability.attack_bonus - 0.5).abs() < 1e-9,
            "no-ability attack_bonus = {}",
            osr_no_ability.attack_bonus
        );

        // With a passive `officerstatall +20%`: each axis multiplier = (1 + 0.20) = 1.2 →
        // rating = 1800 × 1.2 = 2160 → attack_bonus = 1.0 (second breakpoint reached).
        let mut static_buffs_with_all = HashMap::new();
        static_buffs_with_all.insert("officer_stat_all".to_string(), 0.20);
        let osr_with_all = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs_with_all,
            &pending_none,
            &ctx_default,
            &[],
        );
        assert!(
            (osr_with_all.attack_bonus - 1.0).abs() < 1e-9,
            "with-officerstatall attack_bonus = {}",
            osr_with_all.attack_bonus
        );
        assert!(
            (osr_with_all.health_bonus - 1.0).abs() < 1e-9,
            "with-officerstatall health_bonus = {}",
            osr_with_all.health_bonus
        );
        assert!(
            (osr_with_all.defense_shield_deflection_add - 1000.0 * 1.0).abs() < 1e-9,
            "explorer defense channel additive = {}",
            osr_with_all.defense_shield_deflection_add
        );
    }

    #[test]
    fn compute_runtime_bonus_consumes_officer_stathealth_single_axis() {
        // §3: a passive `officerstathealth +X` LCARS effect should only boost the health rating,
        // leaving attack/defense unaffected.
        use crate::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
        let ship = ShipRecord {
            ship_class: "explorer".to_string(),
            officer_bonus: OfficerBonusTable {
                attack: vec![OfficerBonusBreakpoint {
                    value: 1000.0,
                    bonus: 0.5,
                }],
                defense: vec![OfficerBonusBreakpoint {
                    value: 1000.0,
                    bonus: 0.5,
                }],
                health: vec![
                    OfficerBonusBreakpoint {
                        value: 1000.0,
                        bonus: 0.5,
                    },
                    OfficerBonusBreakpoint {
                        value: 2000.0,
                        bonus: 1.0,
                    },
                ],
            },
            ..Default::default()
        };
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 1800.0,
            health: 1800.0,
        };
        let mut static_buffs = HashMap::new();
        static_buffs.insert("officer_health".to_string(), 0.20);
        let pending_none: Vec<crate::lcars::resolver::PendingOfficerStatContribution> = Vec::new();
        let ctx_default = OfficerStatConditionContext::default();
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending_none,
            &ctx_default,
            &[],
        );
        // Health rating: 1800 × 1.20 = 2160 → health_bonus = 1.0.
        assert!(
            (osr.health_bonus - 1.0).abs() < 1e-9,
            "health_bonus = {}",
            osr.health_bonus
        );
        // Attack & defense ratings: 1800 (no boost) → bonus = 0.5.
        assert!(
            (osr.attack_bonus - 0.5).abs() < 1e-9,
            "attack_bonus = {}",
            osr.attack_bonus
        );
    }

    fn ship_with_three_bp_table(ship_class: &str) -> crate::data::ship::ShipRecord {
        use crate::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
        ShipRecord {
            ship_class: ship_class.to_string(),
            officer_bonus: OfficerBonusTable {
                attack: vec![
                    OfficerBonusBreakpoint {
                        value: 1000.0,
                        bonus: 0.5,
                    },
                    OfficerBonusBreakpoint {
                        value: 2000.0,
                        bonus: 1.0,
                    },
                ],
                defense: vec![OfficerBonusBreakpoint {
                    value: 1000.0,
                    bonus: 0.5,
                }],
                health: vec![OfficerBonusBreakpoint {
                    value: 1000.0,
                    bonus: 0.5,
                }],
            },
            ..Default::default()
        }
    }

    #[test]
    fn phase4b_pending_contribution_applies_when_attacker_ship_type_matches() {
        // TOS McCoy pattern: passive + attacker_ship_type_is(explorer), target:self.
        // When attacker is an explorer, the +20% officerstatall fires; otherwise it's dropped.
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        let ship = ship_with_three_bp_table("explorer");
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 0.0,
            health: 0.0,
        };
        let static_buffs = HashMap::<String, f64>::new();
        let pending = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.20,
            target_attacker: true,
            conditions: vec![AbilityConditionSpec::AttackerShipTypeIs {
                ship_type: "explorer".to_string(),
            }],
            opponent_scope: crate::lcars::resolver::OfficerStatOpponentScope::default(),
        }];
        // Explorer attacker → +20% → rating 1800 × 1.20 = 2160 → attack_bonus = 1.0
        let ctx_explorer = OfficerStatConditionContext {
            attacker_ship_class: Some("explorer".to_string()),
            ..Default::default()
        };
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending,
            &ctx_explorer,
            &[],
        );
        assert!(
            (osr.attack_bonus - 1.0).abs() < 1e-9,
            "explorer: {}",
            osr.attack_bonus
        );
        // Battleship attacker → condition false → no boost → rating stays 1800 → attack_bonus = 0.5
        let ctx_bship = OfficerStatConditionContext {
            attacker_ship_class: Some("battleship".to_string()),
            ..Default::default()
        };
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending,
            &ctx_bship,
            &[],
        );
        assert!(
            (osr.attack_bonus - 0.5).abs() < 1e-9,
            "battleship: {}",
            osr.attack_bonus
        );
    }

    #[test]
    fn phase4b_pending_contribution_target_enemy_does_not_buff_attacker() {
        // Kras pattern: target:enemy debuff with defender_is_player_ship. In PvE the defender
        // is a hostile, so the debuff is no-op on attacker compute (and would land on the
        // defender via the deferred Phase 4c plumbing in PvP). Verify the attacker compute
        // never picks up this target:enemy entry regardless of PvP/PvE.
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        let ship = ship_with_three_bp_table("explorer");
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 0.0,
            health: 0.0,
        };
        let static_buffs = HashMap::<String, f64>::new();
        let pending = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.20,
            target_attacker: false, // target:enemy
            conditions: vec![AbilityConditionSpec::DefenderIsPlayerShip],
            opponent_scope: crate::lcars::resolver::OfficerStatOpponentScope::default(),
        }];
        for defender_is_player_ship in [false, true] {
            let ctx = OfficerStatConditionContext {
                defender_is_player_ship,
                ..Default::default()
            };
            let osr = compute_officer_stat_runtime_bonus(
                totals,
                totals,
                &ship,
                &profile,
                None,
                &static_buffs,
                &pending,
                &ctx,
                &[],
            );
            assert!(
                (osr.attack_bonus - 0.5).abs() < 1e-9,
                "target:enemy must not buff attacker (defender_is_player_ship={defender_is_player_ship}): {}",
                osr.attack_bonus
            );
        }
    }

    #[test]
    fn phase4c_opponent_enemy_target_pending_debuffs_defender_ratings() {
        // Kras "Know Your Enemy": `EnemyBridge` debuffs captain + bridge only, not below decks.
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        use crate::lcars::resolver::OfficerStatOpponentScope;
        let ship = ship_with_three_bp_table("explorer");
        let profile = PlayerProfile::default();
        let bridge_totals = crate::combat::CrewOfficerStatTotals {
            attack: 2000.0,
            defense: 0.0,
            health: 0.0,
        };
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 3000.0,
            defense: 0.0,
            health: 0.0,
        };
        let static_buffs = HashMap::<String, f64>::new();
        let attacker_kras = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.20,
            target_attacker: false,
            conditions: vec![AbilityConditionSpec::DefenderIsPlayerShip],
            opponent_scope: OfficerStatOpponentScope::BridgeOfficers,
        }];
        let ctx_pvp = OfficerStatConditionContext {
            defender_is_player_ship: true,
            ..Default::default()
        };
        let osr_baseline = compute_officer_stat_runtime_bonus(
            totals,
            bridge_totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &[],
            &ctx_pvp,
            &[],
        );
        assert!(
            (osr_baseline.attack_bonus - 1.0).abs() < 1e-9,
            "baseline: {}",
            osr_baseline.attack_bonus
        );
        let osr_bridge_debuff = compute_officer_stat_runtime_bonus(
            totals,
            bridge_totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &[],
            &ctx_pvp,
            &attacker_kras,
        );
        // 2000×0.8 + 1000 = 2600 → still second breakpoint (attack_bonus 1.0).
        assert!(
            (osr_bridge_debuff.attack_bonus - 1.0).abs() < 1e-9,
            "bridge-only -20% leaves below decks untouched: {}",
            osr_bridge_debuff.attack_bonus
        );
        let attacker_kras_all_crew = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.20,
            target_attacker: false,
            conditions: vec![AbilityConditionSpec::DefenderIsPlayerShip],
            opponent_scope: OfficerStatOpponentScope::AllCrewed,
        }];
        let osr_full_crew_debuff = compute_officer_stat_runtime_bonus(
            totals,
            bridge_totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &[],
            &ctx_pvp,
            &attacker_kras_all_crew,
        );
        // Crew-wide -20% hits below decks too (3000×0.8 = 2400) — weaker total rating than
        // bridge-only debuff (2600) at this breakpoint table.
        assert!(
            osr_full_crew_debuff.attack_bonus <= osr_bridge_debuff.attack_bonus,
            "crew-wide debuff must not be milder than bridge-only: full={} bridge={}",
            osr_full_crew_debuff.attack_bonus,
            osr_bridge_debuff.attack_bonus
        );
        let bridge_only_crew = crate::combat::CrewOfficerStatTotals {
            attack: 2000.0,
            defense: 0.0,
            health: 0.0,
        };
        let osr_no_below = compute_officer_stat_runtime_bonus(
            bridge_only_crew,
            bridge_only_crew,
            &ship,
            &profile,
            None,
            &static_buffs,
            &[],
            &ctx_pvp,
            &attacker_kras,
        );
        assert!(
            (osr_no_below.attack_bonus - 0.5).abs() < 1e-9,
            "bridge-only crew with -20%: {}",
            osr_no_below.attack_bonus
        );
    }

    #[test]
    fn phase4b_and_composite_evaluates_all_branches() {
        // Strike Team Una pattern: AND(defender_ship_type_is(explorer), defender_is_player_ship).
        // Only fires when both branches are true.
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        let ship = ship_with_three_bp_table("explorer");
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 0.0,
            health: 0.0,
        };
        let static_buffs = HashMap::<String, f64>::new();
        let pending = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.20,
            target_attacker: true,
            conditions: vec![AbilityConditionSpec::And {
                all: vec![
                    AbilityConditionSpec::DefenderShipTypeIs {
                        ship_type: "explorer".to_string(),
                    },
                    AbilityConditionSpec::DefenderIsPlayerShip,
                ],
            }],
            opponent_scope: crate::lcars::resolver::OfficerStatOpponentScope::default(),
        }];

        // Both true → bonus applies (rating 1800 × 1.20 = 2160 → attack_bonus = 1.0).
        let ctx_pvp = OfficerStatConditionContext {
            defender_is_player_ship: true,
            defender_ship_type: Some("explorer".to_string()),
            ..Default::default()
        };
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending,
            &ctx_pvp,
            &[],
        );
        assert!(
            (osr.attack_bonus - 1.0).abs() < 1e-9,
            "both true: {}",
            osr.attack_bonus
        );

        // One branch false → no bonus.
        let ctx_pve = OfficerStatConditionContext {
            defender_is_player_ship: false,
            defender_ship_type: Some("explorer".to_string()),
            ..Default::default()
        };
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending,
            &ctx_pve,
            &[],
        );
        assert!(
            (osr.attack_bonus - 0.5).abs() < 1e-9,
            "pve no fire: {}",
            osr.attack_bonus
        );
    }

    #[test]
    fn phase4b_dynamic_condition_drops_the_contribution() {
        // Kirk-1323b6 pattern: on_round_start + morale_active. Dynamic condition can't be
        // evaluated at fight setup, so the contribution is dropped (Phase 4d will revisit).
        use crate::data::combat_effect_spec::AbilityConditionSpec;
        let ship = ship_with_three_bp_table("explorer");
        let profile = PlayerProfile::default();
        let totals = crate::combat::CrewOfficerStatTotals {
            attack: 1800.0,
            defense: 0.0,
            health: 0.0,
        };
        let static_buffs = HashMap::<String, f64>::new();
        let pending = vec![crate::lcars::resolver::PendingOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.40,
            target_attacker: true,
            conditions: vec![AbilityConditionSpec::MoraleActive],
            opponent_scope: crate::lcars::resolver::OfficerStatOpponentScope::default(),
        }];
        let ctx = OfficerStatConditionContext::default();
        let osr = compute_officer_stat_runtime_bonus(
            totals,
            totals,
            &ship,
            &profile,
            None,
            &static_buffs,
            &pending,
            &ctx,
            &[],
        );
        assert!(
            (osr.attack_bonus - 0.5).abs() < 1e-9,
            "dynamic morale_active must not apply: {}",
            osr.attack_bonus
        );
    }

    #[test]
    fn merge_building_bonuses_into_profile_routes_officer_stats_to_officer_bucket() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 1, level: 1 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(1i64, "test_officer_stat_building".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "test_officer_stat_building".to_string(),
                building_name: "Test".to_string(),
                file: None,
                bid: Some(1),
            }],
        };
        let data_dir = std::env::temp_dir().join("kobayashi_profile_officer_stat_building_test");
        let _ = std::fs::create_dir_all(&data_dir);
        let building_json = r#"{
            "id": "test_officer_stat_building",
            "building_name": "Test",
            "levels": [{
                "level": 1,
                "bonuses": [
                    {"stat": "officer_attack", "value": 0.05, "operator": "add"},
                    {"stat": "weapon_damage", "value": 0.03, "operator": "add"}
                ]
            }]
        }"#;
        std::fs::write(
            data_dir.join("test_officer_stat_building.json"),
            building_json,
        )
        .unwrap();

        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext::default(),
        );

        assert_eq!(profile.bonuses.get("officer_attack"), Some(&0.05));
        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.03));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn merge_building_bonuses_into_profile_merges_holodeck_pierce_not_opaque_buff() {
        let mut profile = PlayerProfile::default();
        let imported_buildings = vec![BuildingEntry { bid: 69, level: 5 }];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(69i64, "building_69".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![BuildingIndexEntry {
                id: "building_69".to_string(),
                building_name: "Holodeck".to_string(),
                file: Some("69_holodeck.json".to_string()),
                bid: Some(69),
            }],
        };
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/buildings");
        merge_building_bonuses_into_profile(
            &mut profile,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext::default(),
        );
        assert!(
            profile.bonuses.get("pierce").copied().unwrap_or(0.0) > 0.0,
            "holodeck level 5 should contribute pierce via normalized stat"
        );
        assert!(!profile.bonuses.contains_key("buff_473361651"));
    }

    /// Spot-check triangle buildings at L60: Foundry / Science Lab / Engine Technology Lab shield_hp by hull class.
    #[test]
    fn merge_building_bonuses_higgsbozo_holodeck_foundry_science_lab_levels() {
        let imported_buildings = vec![
            BuildingEntry { bid: 69, level: 20 },
            BuildingEntry { bid: 43, level: 60 },
            BuildingEntry { bid: 29, level: 60 },
            BuildingEntry { bid: 14, level: 60 },
        ];
        let mut bid_to_id = HashMap::new();
        bid_to_id.insert(69i64, "building_69".to_string());
        bid_to_id.insert(43i64, "building_43".to_string());
        bid_to_id.insert(29i64, "building_29".to_string());
        bid_to_id.insert(14i64, "engine_technology_lab".to_string());
        let building_index = BuildingIndex {
            data_version: None,
            source_note: None,
            buildings: vec![
                BuildingIndexEntry {
                    id: "building_69".to_string(),
                    building_name: "Holodeck".to_string(),
                    file: Some("69_holodeck".to_string()),
                    bid: Some(69),
                },
                BuildingIndexEntry {
                    id: "building_43".to_string(),
                    building_name: "Foundry".to_string(),
                    file: Some("43_foundry".to_string()),
                    bid: Some(43),
                },
                BuildingIndexEntry {
                    id: "building_29".to_string(),
                    building_name: "Science Lab".to_string(),
                    file: Some("29_science_lab".to_string()),
                    bid: Some(29),
                },
                BuildingIndexEntry {
                    id: "engine_technology_lab".to_string(),
                    building_name: "Engine Technology Lab".to_string(),
                    file: Some("14_engine_technology_lab".to_string()),
                    bid: Some(14),
                },
            ],
        };
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/buildings");

        let mut profile_bb = PlayerProfile::default();
        merge_building_bonuses_into_profile(
            &mut profile_bb,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext {
                mode: BuildingMode::ShipCombat,
                attacker_ship_type: Some(ShipType::Battleship),
                ..BuildingBonusContext::default()
            },
        );
        assert_eq!(
            profile_bb.bonuses.get("pierce"),
            Some(&5.0),
            "Holodeck 20 pierce"
        );
        assert_eq!(
            profile_bb.bonuses.get("armor"),
            Some(&1.4),
            "Foundry 60 armor"
        );
        assert_eq!(
            profile_bb.bonuses.get("shield_deflection"),
            Some(&1.4),
            "Science Lab 60 shield deflection (global)"
        );
        assert_eq!(
            profile_bb.bonuses.get("shield_hp"),
            Some(&1.8),
            "Battleship: Foundry shield_hp only at L60"
        );

        let mut profile_ex = PlayerProfile::default();
        merge_building_bonuses_into_profile(
            &mut profile_ex,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext {
                mode: BuildingMode::ShipCombat,
                attacker_ship_type: Some(ShipType::Explorer),
                ..BuildingBonusContext::default()
            },
        );
        assert_eq!(
            profile_ex.bonuses.get("shield_hp"),
            Some(&1.8),
            "Explorer: Science Lab shield_hp only at L60"
        );

        let mut profile_int = PlayerProfile::default();
        merge_building_bonuses_into_profile(
            &mut profile_int,
            &imported_buildings,
            &bid_to_id,
            &building_index,
            data_dir.as_path(),
            &BuildingBonusContext {
                mode: BuildingMode::ShipCombat,
                attacker_ship_type: Some(ShipType::Interceptor),
                ..BuildingBonusContext::default()
            },
        );
        assert_eq!(
            profile_int.bonuses.get("dodge"),
            Some(&1.4),
            "Engine Technology Lab 60 dodge (global)"
        );
        assert_eq!(
            profile_int.bonuses.get("shield_hp"),
            Some(&1.8),
            "Interceptor: Engine Technology Lab shield_hp only at L60"
        );
        assert!(!profile_bb.bonuses.contains_key("buff_2151753795"));
        assert!(!profile_bb.bonuses.contains_key("buff_1593096695"));
    }

    #[test]
    fn merge_forbidden_tech_fids_scales_additive_by_level_tier_when_enabled() {
        let mut profile = PlayerProfile::default();

        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(1),
                name: "Ablative Armor".to_string(),
                tech_type: "forbidden".to_string(),
                tier: Some(1),
                bonuses: vec![BonusEntry {
                    stat: "weapon_damage".to_string(),
                    value: 0.1,
                    operator: "add".to_string(),
                }],
            }],
        };

        let imported = vec![ForbiddenTechEntry {
            fid: 1,
            tier: 1,
            level: 5, // tier 1 max level assumed 10 => 0.5 factor
            shard_count: 0,
        }];

        merge_tech_fids_into_profile_with_level_tier(&mut profile, &[1], &imported, &catalog, true);

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.05));
    }

    #[test]
    fn merge_forbidden_tech_fids_does_not_scale_when_disabled() {
        let mut profile = PlayerProfile::default();

        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(1),
                name: "Ablative Armor".to_string(),
                tech_type: "forbidden".to_string(),
                tier: Some(1),
                bonuses: vec![BonusEntry {
                    stat: "weapon_damage".to_string(),
                    value: 0.1,
                    operator: "add".to_string(),
                }],
            }],
        };

        let imported = vec![ForbiddenTechEntry {
            fid: 1,
            tier: 1,
            level: 5,
            shard_count: 0,
        }];

        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &[1],
            &imported,
            &catalog,
            false,
        );

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.1));
    }

    #[test]
    fn merge_forbidden_tech_fids_does_not_cross_scale_catalog_tier() {
        let mut profile = PlayerProfile::default();

        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(1),
                name: "Ablative Armor".to_string(),
                tech_type: "forbidden".to_string(),
                tier: Some(1),
                bonuses: vec![BonusEntry {
                    stat: "weapon_damage".to_string(),
                    value: 0.1,
                    operator: "add".to_string(),
                }],
            }],
        };

        let imported = vec![ForbiddenTechEntry {
            fid: 1,
            tier: 2, // disagrees with catalog.tier => no scaling
            level: 5,
            shard_count: 0,
        }];

        merge_tech_fids_into_profile_with_level_tier(&mut profile, &[1], &imported, &catalog, true);

        assert_eq!(profile.bonuses.get("weapon_damage"), Some(&0.1));
    }

    #[test]
    fn borg_alcove_skips_flat_profile_and_exposes_hull_fraction_and_crit_seats() {
        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(super::BORG_ALCOVE_FORBIDDEN_TECH_FID),
                name: "Borg Alcove".into(),
                tech_type: "forbidden".into(),
                tier: Some(12),
                bonuses: vec![
                    BonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.2,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.12,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "crit_damage".into(),
                        value: 0.85,
                        operator: "add".into(),
                    },
                ],
            }],
        };
        let imported = vec![ForbiddenTechEntry {
            fid: super::BORG_ALCOVE_FORBIDDEN_TECH_FID,
            tier: 12,
            level: 60,
            shard_count: 0,
        }];
        let effective = vec![super::BORG_ALCOVE_FORBIDDEN_TECH_FID];

        let mut profile = PlayerProfile::default();
        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &effective,
            &imported,
            &catalog,
            false,
        );
        assert!(!profile.bonuses.contains_key("crit_chance"));
        assert!(!profile.bonuses.contains_key("hull_hp"));
        assert!(!profile.bonuses.contains_key("crit_damage"));

        let seats = super::forbidden_tech_derived_attack_phase_seats(
            &imported, &effective, &catalog, false,
        );
        assert_eq!(seats.len(), 2);

        let hull =
            super::borg_alcove_hull_hp_bonus_fraction(&imported, &effective, &catalog, false);
        assert!((hull.unwrap() - 0.12).abs() < 1e-12);
    }

    #[test]
    fn borg_operating_table_skips_flat_profile_and_emits_conqueror_gated_seats() {
        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(super::BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID),
                name: "Borg Operating Table".into(),
                tech_type: "forbidden".into(),
                tier: Some(12),
                bonuses: vec![
                    BonusEntry {
                        stat: "crit_damage".into(),
                        value: 1.0,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "apex_shred".into(),
                        value: 0.1,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "hostile_crit_damage_reduction".into(),
                        value: 0.05,
                        operator: "add".into(),
                    },
                ],
            }],
        };
        let imported = vec![ForbiddenTechEntry {
            fid: super::BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID,
            tier: 12,
            level: 60,
            shard_count: 0,
        }];
        let effective = vec![super::BORG_OPERATING_TABLE_FORBIDDEN_TECH_FID];

        let mut profile = PlayerProfile::default();
        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &effective,
            &imported,
            &catalog,
            false,
        );
        assert!(!profile.bonuses.contains_key("crit_damage"));
        assert!(!profile.bonuses.contains_key("apex_shred"));

        let seats = super::borg_operating_table_forbidden_tech_seats(
            &imported, &effective, &catalog, false,
        );
        assert_eq!(seats.len(), 3);
        assert!(seats.iter().all(|s| s.ability.condition.is_some()));
    }

    #[test]
    fn quantum_slipstream_skips_profile_shield_mitigation_and_emits_debuff_seat() {
        use crate::combat::AbilityEffect;

        let fid = super::QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID;
        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(fid),
                name: "Quantum Slipstream Drive".into(),
                tech_type: "forbidden".into(),
                tier: Some(12),
                bonuses: vec![
                    BonusEntry {
                        stat: "crit_chance".into(),
                        value: 0.16,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "shield_mitigation".into(),
                        value: 0.15,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "shield_hp".into(),
                        value: 0.195,
                        operator: "add".into(),
                    },
                ],
            }],
        };
        let imported = vec![ForbiddenTechEntry {
            fid,
            tier: 12,
            level: 60,
            shard_count: 0,
        }];
        let effective = vec![fid];

        let mut profile = PlayerProfile::default();
        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &effective,
            &imported,
            &catalog,
            false,
        );
        assert_eq!(profile.bonuses.get("crit_chance"), Some(&0.16));
        assert_eq!(profile.bonuses.get("shield_hp"), Some(&0.195));
        assert!(!profile.bonuses.contains_key("shield_mitigation"));

        let seats = super::quantum_slipstream_forbidden_tech_round_start_seats(
            &imported, &effective, &catalog, false,
        );
        assert_eq!(seats.len(), 1);
        match &seats[0].ability.effect {
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round, cap } => {
                assert!((cap - 0.15).abs() < 1e-12);
                assert!((per_round - 0.05).abs() < 1e-12);
            }
            _ => panic!("expected cumulative opponent shield mitigation debuff"),
        }
    }

    #[test]
    fn ship_class_torpedo_family_s31_skips_flat_profile_and_exposes_gated_seats_and_hull_fraction()
    {
        use crate::combat::AbilityCondition;
        use crate::combat::ShipType;

        let fid = super::S31_TORPEDO_PODS_FORBIDDEN_TECH_FID;
        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(fid),
                name: "S31 Torpedo Pods".into(),
                tech_type: "forbidden".into(),
                tier: Some(12),
                bonuses: vec![
                    BonusEntry {
                        stat: "armor".into(),
                        value: 0.08,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "shield_mitigation".into(),
                        value: 0.08,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "dodge".into(),
                        value: 0.08,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.12,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "pierce".into(),
                        value: 0.06,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "weapon_damage".into(),
                        value: 0.155,
                        operator: "add".into(),
                    },
                ],
            }],
        };
        let imported = vec![ForbiddenTechEntry {
            fid,
            tier: 12,
            level: 60,
            shard_count: 0,
        }];
        let effective = vec![fid];

        let mut profile = PlayerProfile::default();
        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &effective,
            &imported,
            &catalog,
            false,
        );
        assert!(profile.bonuses.is_empty());

        assert!(
            super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
                &imported, &effective, &catalog, false, None,
            )
            .is_none()
        );

        let ship_bb = ShipRecord {
            id: "t".into(),
            ship_name: "T".into(),
            ship_class: "battleship".into(),
            faction: None,
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 1.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: None,
            ..Default::default()
        };
        let hull = super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_bb),
        );
        assert!((hull.unwrap() - 0.12).abs() < 1e-12);
        let sm =
            super::ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship(
                &imported,
                &effective,
                &catalog,
                false,
                Some(&ship_bb),
            );
        assert!((sm.unwrap() - 0.08).abs() < 1e-12);
        let acc = super::ship_class_gated_torpedo_family_hostile_accuracy_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_bb),
        );
        assert!((acc.unwrap() - 0.06).abs() < 1e-12);

        let seats = super::ship_class_gated_torpedo_family_derived_seats(
            &imported, &effective, &catalog, false,
        );
        assert_eq!(seats.len(), 3);
        let expected_gate = AbilityCondition::And(vec![
            AbilityCondition::AttackerShipTypeIs(ShipType::Battleship),
            AbilityCondition::DefenderIsNpcHostile,
        ]);
        for s in &seats {
            assert_eq!(s.ability.condition.as_ref(), Some(&expected_gate));
        }
    }

    #[test]
    fn ship_class_torpedo_family_control_seeker_matches_explorer_hull_only() {
        let fid = super::CONTROL_SEEKER_PROBES_FORBIDDEN_TECH_FID;
        let catalog = ForbiddenChaosList {
            source: None,
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(fid),
                name: "Control Seeker Probes".into(),
                tech_type: "forbidden".into(),
                tier: Some(12),
                bonuses: vec![
                    BonusEntry {
                        stat: "hull_hp".into(),
                        value: 0.12,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "shield_mitigation".into(),
                        value: 0.08,
                        operator: "add".into(),
                    },
                    BonusEntry {
                        stat: "accuracy".into(),
                        value: 0.06,
                        operator: "add".into(),
                    },
                ],
            }],
        };
        let imported = vec![ForbiddenTechEntry {
            fid,
            tier: 12,
            level: 60,
            shard_count: 0,
        }];
        let effective = vec![fid];

        let ship_explorer = ShipRecord {
            id: "e".into(),
            ship_name: "E".into(),
            ship_class: "explorer".into(),
            faction: None,
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 1.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: None,
            ..Default::default()
        };
        let ship_bb = ShipRecord {
            id: "b".into(),
            ship_name: "B".into(),
            ship_class: "battleship".into(),
            faction: None,
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 1.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: None,
            ..Default::default()
        };

        let h_ex = super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_explorer),
        );
        assert!((h_ex.unwrap() - 0.12).abs() < 1e-12);
        assert!(
            super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
                &imported,
                &effective,
                &catalog,
                false,
                Some(&ship_bb),
            )
            .is_none()
        );
    }

    fn tiny_catalog(items: Vec<ForbiddenChaosRecord>) -> ForbiddenChaosList {
        ForbiddenChaosList {
            source: None,
            last_updated: None,
            items,
        }
    }

    #[test]
    fn resolve_effective_tech_fids_empty_equip_means_no_fids() {
        let catalog = tiny_catalog(vec![
            ForbiddenChaosRecord {
                fid: Some(10),
                name: "F".into(),
                tech_type: "forbidden".into(),
                tier: None,
                bonuses: vec![],
            },
            ForbiddenChaosRecord {
                fid: Some(20),
                name: "C".into(),
                tech_type: "chaos".into(),
                tier: None,
                bonuses: vec![],
            },
        ]);
        let imported = vec![
            ForbiddenTechEntry {
                fid: 10,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
            ForbiddenTechEntry {
                fid: 20,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
        ];
        let profile = PlayerProfile::default();
        let fids = resolve_effective_tech_fids(&profile, &imported, &catalog);
        assert!(fids.is_empty());
    }

    #[test]
    fn resolve_effective_tech_fids_respects_equipped_slots() {
        let catalog = tiny_catalog(vec![
            ForbiddenChaosRecord {
                fid: Some(10),
                name: "F".into(),
                tech_type: "forbidden".into(),
                tier: None,
                bonuses: vec![],
            },
            ForbiddenChaosRecord {
                fid: Some(20),
                name: "C".into(),
                tech_type: "chaos".into(),
                tier: None,
                bonuses: vec![],
            },
        ]);
        let imported = vec![ForbiddenTechEntry {
            fid: 10,
            tier: 1,
            level: 1,
            shard_count: 0,
        }];
        let profile = PlayerProfile {
            equipped_forbidden_fid: Some(10),
            equipped_chaos_fid: Some(20),
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![10, 20]
        );
    }

    #[test]
    fn resolve_effective_tech_fids_skips_unknown_fids() {
        let catalog = tiny_catalog(vec![ForbiddenChaosRecord {
            fid: Some(1),
            name: "Only".into(),
            tech_type: "forbidden".into(),
            tier: None,
            bonuses: vec![],
        }]);
        let imported = vec![ForbiddenTechEntry {
            fid: 1,
            tier: 1,
            level: 1,
            shard_count: 0,
        }];
        let profile = PlayerProfile {
            equipped_forbidden_fid: Some(999),
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn resolve_effective_tech_fids_empty_tech_type_is_forbidden_lane() {
        let catalog = tiny_catalog(vec![ForbiddenChaosRecord {
            fid: Some(3),
            name: "Legacy".into(),
            tech_type: "".into(),
            tier: None,
            bonuses: vec![],
        }]);
        let imported = vec![ForbiddenTechEntry {
            fid: 3,
            tier: 1,
            level: 1,
            shard_count: 0,
        }];
        let profile = PlayerProfile {
            equipped_forbidden_fid: Some(3),
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![3]
        );
    }

    #[test]
    fn resolve_effective_tech_fids_ignores_legacy_overrides() {
        let catalog = tiny_catalog(vec![
            ForbiddenChaosRecord {
                fid: Some(1),
                name: "F".into(),
                tech_type: "forbidden".into(),
                tier: None,
                bonuses: vec![],
            },
            ForbiddenChaosRecord {
                fid: Some(2),
                name: "C".into(),
                tech_type: "chaos".into(),
                tier: None,
                bonuses: vec![],
            },
        ]);
        let imported = vec![
            ForbiddenTechEntry {
                fid: 1,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
            ForbiddenTechEntry {
                fid: 2,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
        ];
        let profile = PlayerProfile {
            forbidden_tech_override: Some(vec![100]),
            chaos_tech_override: Some(vec![200]),
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn validate_player_profile_rejects_wrong_lane_fids() {
        let catalog = tiny_catalog(vec![
            ForbiddenChaosRecord {
                fid: Some(1),
                name: "F".into(),
                tech_type: "forbidden".into(),
                tier: None,
                bonuses: vec![],
            },
            ForbiddenChaosRecord {
                fid: Some(2),
                name: "C".into(),
                tech_type: "chaos".into(),
                tier: None,
                bonuses: vec![],
            },
        ]);
        let bad = PlayerProfile {
            equipped_forbidden_fid: Some(2),
            ..Default::default()
        };
        assert!(validate_player_profile_payload(bad, Some(&catalog)).is_err());
    }

    #[test]
    fn research_derived_attack_phase_seats_matches_spec_adapter() {
        use crate::data::import::ResearchEntry;
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let record = ResearchRecord {
            rid: 5001,
            name: None,
            data_version: None,
            source_note: None,
            levels: vec![ResearchLevel {
                level: 1,
                bonuses: vec![ResearchBonusEntry {
                    stat: "weapon_damage".to_string(),
                    value: 0.02,
                    operator: "add".to_string(),
                    condition: crate::data::research::ResearchBonusConditionKey {
                        requires_defender_burning: true,
                        ..Default::default()
                    },
                }],
            }],
        };
        let catalog = ResearchCatalog {
            source: None,
            last_updated: None,
            items: vec![record],
        };
        let imported = vec![ResearchEntry {
            rid: 5001,
            level: 1,
        }];
        let gates = SupportBuffResearchGateState::default();
        let via_public = research_derived_attack_phase_seats(
            &imported,
            &catalog,
            &gates,
            &std::collections::HashMap::new(),
        );
        let via_spec =
            crate::data::research_effect_spec_adapter::research_derived_attack_phase_seats_from_spec(
                &imported,
                &catalog,
                &std::collections::HashMap::new(),
            );
        assert_eq!(via_public, via_spec);
    }
}
