//! Player profile: effective_bonuses applied as pre-combat modifier layer (DESIGN §5).
//! Keys match engine/LCARS stats: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce,
//! accuracy (scales ship AttackerStats for dodge mitigation), apex_shred, apex_barrier, isolytic_*, etc.
//! Bonuses from synced forbidden/chaos tech (by fid) are merged in when [merge_forbidden_tech_bonuses_into_profile] is used.
//! **Borg Alcove** ([`BORG_ALCOVE_FORBIDDEN_TECH_FID`]) is an exception: Voyager/NPC-gated combat stats use
//! [`forbidden_tech_derived_attack_phase_seats`] and [`borg_alcove_hull_hp_bonus_fraction`] instead of flat `bonuses`.
//! **Quantum Slipstream Drive** ([`QUANTUM_SLIPSTREAM_FORBIDDEN_TECH_FID`]): opponent cumulative shield-mitigation
//! debuff is [`AbilityEffect::CumulativeOpponentShieldMitigationDebuff`] via
//! [`quantum_slipstream_forbidden_tech_round_start_seats`]; catalog `shield_mitigation` is a **cap** source only
//! (skipped in [`merge_tech_fids_into_profile`] / [`merge_tech_fids_into_profile_with_level_tier`]).
//! **Ship-class + hostile-gated torpedo family** (S31 Battleship, Control Seeker Probes Explorer, Dual Photon
//! Warheads Interceptor): combat stats are not merged as unconditional [`PlayerProfile::bonuses`]; see
//! [`ship_class_gated_torpedo_family_derived_seats`], [`ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship`],
//! and scenario-side hostile shield / accuracy patches.
//! Bonuses from synced buildings (by bid) are merged in when [merge_building_bonuses_into_profile] is used.
//! Bonuses from synced research (by rid) are merged in when [merge_research_bonuses_into_profile] is used.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, AttackerStats, Combatant, CrewSeat,
    CrewSeatContext, ShipType, TimingWindow, EPSILON, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::combat::condition::ability_condition_from_research_bonus_key;
use crate::data::building::{self, BuildingBonusContext, BuildingIndex};
use crate::data::forbidden_chaos::ForbiddenChaosList;
use crate::data::import::{BuildingEntry, ForbiddenTechEntry, ResearchEntry};
use crate::data::research::{
    cumulative_conditional_research_bonuses, cumulative_research_bonuses, ResearchCatalog,
};
use crate::data::ship::ShipRecord;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: HashMap<String, f64>,
    /// Optional Operations Center level override. When set, building bonus context uses this
    /// instead of inferring from synced buildings (ops_center level). Lets you simulate without sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops_level: Option<u32>,
    /// When set and non-empty, these fids are used instead of synced forbidden_tech.imported.json
    /// to merge forbidden-tech bonuses. Enables UI to choose "Custom" tech set per profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbidden_tech_override: Option<Vec<i64>>,
    /// When set and non-empty, these fids are used instead of synced chaos tech from
    /// forbidden_tech.imported.json. Enables UI to choose "Custom" chaos tech set per profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos_tech_override: Option<Vec<i64>>,
}

pub const DEFAULT_PROFILE_PATH: &str = "data/profile.json";

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

/// Resolves effective tech fids from profile overrides or imported entries, split by tech_type.
/// Forbidden: use forbidden_tech_override if set, else imported entries matching tech_type "forbidden".
/// Chaos: use chaos_tech_override if set, else imported entries matching tech_type "chaos".
/// Items with empty tech_type are treated as forbidden for backward compatibility.
pub fn resolve_effective_tech_fids(
    profile: &PlayerProfile,
    imported_ft: &[ForbiddenTechEntry],
    catalog: &ForbiddenChaosList,
) -> Vec<i64> {
    let by_fid: HashMap<i64, &crate::data::forbidden_chaos::ForbiddenChaosRecord> = catalog
        .items
        .iter()
        .filter_map(|r| r.fid.map(|id| (id, r)))
        .collect();

    let is_forbidden = |r: &&crate::data::forbidden_chaos::ForbiddenChaosRecord| {
        r.tech_type.is_empty() || r.tech_type.eq_ignore_ascii_case("forbidden")
    };
    let is_chaos = |r: &&crate::data::forbidden_chaos::ForbiddenChaosRecord| {
        r.tech_type.eq_ignore_ascii_case("chaos")
    };

    let forbidden_fids: Vec<i64> = if profile
        .forbidden_tech_override
        .as_ref()
        .is_some_and(|v| !v.is_empty())
    {
        profile.forbidden_tech_override.as_ref().unwrap().clone()
    } else {
        imported_ft
            .iter()
            .filter(|e| by_fid.get(&e.fid).is_some_and(is_forbidden))
            .map(|e| e.fid)
            .collect()
    };

    let chaos_fids: Vec<i64> = if profile
        .chaos_tech_override
        .as_ref()
        .is_some_and(|v| !v.is_empty())
    {
        profile.chaos_tech_override.as_ref().unwrap().clone()
    } else {
        imported_ft
            .iter()
            .filter(|e| by_fid.get(&e.fid).is_some_and(is_chaos))
            .map(|e| e.fid)
            .collect()
    };

    let mut out = forbidden_fids;
    out.extend(chaos_fids);
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
    let imported = imported_by_fid.get(&BORG_ALCOVE_FORBIDDEN_TECH_FID).copied();
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
    let imported = imported_by_fid.get(&BORG_ALCOVE_FORBIDDEN_TECH_FID).copied();

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
        if let Some(a) =
            torpedo_family_hostile_accuracy_fraction_for_record(record, imported, scale_by_level_tier)
        {
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
                    name: format!("forbidden_tech_ship_class_torpedo_family_{fid}_armor_dodge_return_fire"),
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
                        name: format!("forbidden_tech_ship_class_torpedo_family_{fid}_weapon_damage"),
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

fn normalize_profile_combat_stat(stat: &str) -> Option<&'static str> {
    match stat {
        "weapon_damage" => Some("weapon_damage"),
        "hull_hp" => Some("hull_hp"),
        "shield_hp" => Some("shield_hp"),
        "isolytic_damage" => Some("isolytic_damage"),
        // Morale-gated isolytic (research NS Morale Isolytic Damage, rid 4133019450): scenario injects a round-start seat.
        "isolytic_damage_morale" => Some("isolytic_damage_morale"),
        "isolytic_defense" => Some("isolytic_defense"),
        // Apex: same units as Combatant / engine (shred decimal; barrier pool vs hostile barrier formula).
        "apex_shred" => Some("apex_shred"),
        "apex_barrier" => Some("apex_barrier"),
        "crit_chance" => Some("crit_chance"),
        "crit_damage" => Some("crit_damage"),
        "pierce" | "armor_pierce" | "shield_pierce" => Some("pierce"),
        "shield_mitigation" => Some("shield_mitigation"),
        "armor" => Some("armor"),
        "dodge" => Some("dodge"),
        "damage_reduction" => Some("damage_reduction"),
        // Used with ship `AttackerStats.accuracy` for dodge leg of mitigation (see scenario.rs).
        // Catalog values are fractional (e.g. 0.1 = +10% effective accuracy vs base ship stat).
        "accuracy" => Some("accuracy"),
        _ => None,
    }
}

/// Merges combat stat bonuses from player's synced buildings into `profile.bonuses`.
/// Resolves bid → building id via `bid_to_id`, loads building records, computes cumulative
/// bonuses, and adds only combat keys (weapon_damage, hull_hp, etc.). armor_pierce and
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

/// Per-`rid` research level from sync import: duplicate rows use **max** level for that `rid`.
fn research_levels_by_rid_from_import(imported_research: &[ResearchEntry]) -> HashMap<i64, u32> {
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
/// `crit_damage` become attack-phase ship seats so crit rolls respect gates (see `research.rs`).
///
/// Unconditional `crit_*` rows stay in [`merge_research_bonuses_into_profile`] / `profile.bonuses`.
pub fn research_derived_attack_phase_seats(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
) -> Vec<CrewSeatContext> {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return Vec::new();
    }
    let levels_by_rid = research_levels_by_rid_from_import(imported_research);
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

    let conditional = cumulative_conditional_research_bonuses(&records, &levels_by_rid);
    let mut out: Vec<CrewSeatContext> = Vec::new();
    let mut idx = 0u32;
    for ((key, stat), value) in conditional {
        if !value.is_finite() || value == 0.0 {
            continue;
        }
        let Some(norm) = normalize_profile_combat_stat(&stat) else {
            continue;
        };
        if norm != "crit_chance" && norm != "crit_damage" {
            continue;
        }
        let Some(condition) = ability_condition_from_research_bonus_key(&key) else {
            continue;
        };
        let effect = match norm {
            "crit_chance" => AbilityEffect::CritChanceBonus(
                crate::data::ship_ability_resolve::normalize_probability(value),
            ),
            "crit_damage" => AbilityEffect::CritDamageMultiplier((1.0 + value).max(EPSILON)),
            _ => continue,
        };
        idx = idx.saturating_add(1);
        out.push(CrewSeatContext {
            seat: CrewSeat::Ship,
            ability: Ability {
                name: format!("research_{norm}_{idx}"),
                class: AbilityClass::ShipAbility,
                timing: TimingWindow::AttackPhase,
                boostable: false,
                effect,
                condition: Some(condition),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        });
    }
    out
}

/// Effective combat stat bonuses from synced research only (engine keys after normalization).
/// Duplicate `rid` rows use the **maximum** synced level for that `rid`.
/// Used by [`merge_research_bonuses_into_profile`] and [`crate::data::research_summary::research_combat_summary_for_profile`].
pub fn combat_research_bonuses_from_import(
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
) -> HashMap<String, f64> {
    if imported_research.is_empty() || catalog.items.is_empty() {
        return HashMap::new();
    }

    let levels_by_rid = research_levels_by_rid_from_import(imported_research);
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

/// Merges combat stat bonuses from player's synced research into `profile.bonuses`.
/// For each imported research entry (rid, level), looks up the catalog by rid and sums
/// cumulative bonuses for levels 1..=level. Only combat stats are applied (same keys as buildings).
/// Duplicate `rid` rows use the **maximum** synced level for that `rid`.
pub fn merge_research_bonuses_into_profile(
    profile: &mut PlayerProfile,
    imported_research: &[ResearchEntry],
    catalog: &ResearchCatalog,
) {
    let bonuses = combat_research_bonuses_from_import(imported_research, catalog);
    for (key, value) in bonuses {
        let current = profile.bonuses.get(&key).copied().unwrap_or(0.0);
        profile.bonuses.insert(key, current + value);
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

    Combatant {
        attack: combatant.attack * weapon_mult,
        hull_health: combatant.hull_health * hull_mult,
        shield_health: combatant.shield_health * shield_mult,
        pierce: (combatant.pierce + pierce_add).max(0.0),
        crit_chance: (combatant.crit_chance + crit_chance_add).clamp(0.0, 1.0),
        crit_multiplier: (combatant.crit_multiplier * crit_damage_mult).max(0.0),
        isolytic_damage: (combatant.isolytic_damage + isolytic_damage_add).max(0.0),
        isolytic_defense: (combatant.isolytic_defense + isolytic_defense_add).max(0.0),
        apex_shred: (combatant.apex_shred + apex_shred_add).max(0.0),
        apex_barrier: (combatant.apex_barrier + apex_barrier_add).max(0.0),
        shield_mitigation: (combatant.shield_mitigation + shield_mitigation_add).clamp(0.0, 1.0),
        mitigation: (combatant.mitigation + armor_add + damage_reduction_add + dodge_add)
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

/// Apply effective_bonuses to attacker Combatant (multipliers and additive bonuses).
/// Keys: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce (additive),
/// shield_mitigation (additive to base), armor/dodge/damage_reduction (additive to mitigation),
/// isolytic_damage / isolytic_defense, apex_shred / apex_barrier (additive; counter-attack uses player apex_barrier).
pub fn apply_profile_to_attacker(attacker: Combatant, profile: &PlayerProfile) -> Combatant {
    if profile.bonuses.is_empty() {
        return attacker;
    }
    let weapon = 1.0 + get_bonus(profile, "weapon_damage");
    let hull_hp = 1.0 + get_bonus(profile, "hull_hp");
    let shield_hp = 1.0 + get_bonus(profile, "shield_hp");
    let isolytic_damage_add = get_bonus(profile, "isolytic_damage");
    let isolytic_defense_add = get_bonus(profile, "isolytic_defense");
    let apex_shred_add = get_bonus(profile, "apex_shred");
    let apex_barrier_add = get_bonus(profile, "apex_barrier");
    let crit_chance_add = get_bonus(profile, "crit_chance");
    let crit_damage_mult = 1.0 + get_bonus(profile, "crit_damage");
    let pierce_add = get_bonus(profile, "pierce");
    let shield_mit_add = get_bonus(profile, "shield_mitigation");
    let mitigation_add = get_bonus(profile, "armor")
        + get_bonus(profile, "dodge")
        + get_bonus(profile, "damage_reduction");

    Combatant {
        attack: attacker.attack * weapon,
        hull_health: attacker.hull_health * hull_hp,
        shield_health: attacker.shield_health * shield_hp,
        crit_chance: (attacker.crit_chance + crit_chance_add).clamp(0.0, 1.0),
        crit_multiplier: (attacker.crit_multiplier * crit_damage_mult).max(0.0),
        pierce: (attacker.pierce + pierce_add).max(0.0),
        mitigation: (attacker.mitigation + mitigation_add).clamp(0.0, 1.0),
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

    use crate::combat::{AttackerStats, Combatant};
    use crate::data::building::{
        BuildingBonusContext, BuildingIndex, BuildingIndexEntry, BuildingMode,
    };
    use crate::data::forbidden_chaos::{BonusEntry, ForbiddenChaosList, ForbiddenChaosRecord};
    use crate::data::import::BuildingEntry;
    use crate::data::import::ForbiddenTechEntry;
    use crate::data::ship::ShipRecord;

    use super::*;

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
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog);
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
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog);
        assert_eq!(profile.bonuses.get("apex_shred"), Some(&0.25));
        assert_eq!(profile.bonuses.get("apex_barrier"), Some(&500.0));
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
        merge_research_bonuses_into_profile(&mut profile, &imported_research, &catalog);
        assert!(profile.bonuses.is_empty());
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
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
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
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
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
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("apex_shred".to_string(), 0.15);
        profile.bonuses.insert("apex_barrier".to_string(), 200.0);
        let out = apply_profile_to_attacker(attacker, &profile);
        assert!((out.apex_shred - 0.25).abs() < 1e-9);
        assert!((out.apex_barrier - 300.0).abs() < 1e-9);
    }

    #[test]
    fn apply_profile_to_attacker_applies_mitigation_stats() {
        let attacker = Combatant {
            id: "test".to_string(),
            attack: 100.0,
            mitigation: 0.10,
            pierce: 0.05,
            crit_chance: 0.10,
            crit_multiplier: 1.0,
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
        };
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("armor".to_string(), 0.04);
        profile.bonuses.insert("dodge".to_string(), 0.03);
        profile.bonuses.insert("damage_reduction".to_string(), 0.02);

        let out = apply_profile_to_attacker(attacker, &profile);
        assert!((out.mitigation - 0.19).abs() < 1e-9);
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
        assert!(profile.bonuses.get("crit_chance").is_none());
        assert!(profile.bonuses.get("hull_hp").is_none());
        assert!(profile.bonuses.get("crit_damage").is_none());

        let seats = super::forbidden_tech_derived_attack_phase_seats(
            &imported,
            &effective,
            &catalog,
            false,
        );
        assert_eq!(seats.len(), 2);

        let hull = super::borg_alcove_hull_hp_bonus_fraction(&imported, &effective, &catalog, false);
        assert!((hull.unwrap() - 0.12).abs() < 1e-12);
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
        assert!(profile.bonuses.get("shield_mitigation").is_none());

        let seats = super::quantum_slipstream_forbidden_tech_round_start_seats(
            &imported,
            &effective,
            &catalog,
            false,
        );
        assert_eq!(seats.len(), 1);
        match &seats[0].ability.effect {
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff {
                per_round,
                cap,
            } => {
                assert!((cap - 0.15).abs() < 1e-12);
                assert!((per_round - 0.05).abs() < 1e-12);
            }
            _ => panic!("expected cumulative opponent shield mitigation debuff"),
        }
    }

    #[test]
    fn ship_class_torpedo_family_s31_skips_flat_profile_and_exposes_gated_seats_and_hull_fraction() {
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

        assert!(super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            None,
        )
        .is_none());

        let ship_bb = ShipRecord {
            id: "t".into(),
            ship_name: "T".into(),
            ship_class: "battleship".into(),
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
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
        };
        let hull = super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_bb),
        );
        assert!((hull.unwrap() - 0.12).abs() < 1e-12);
        let sm = super::ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship(
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
            &imported,
            &effective,
            &catalog,
            false,
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
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
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
        };
        let ship_bb = ShipRecord {
            id: "b".into(),
            ship_name: "B".into(),
            ship_class: "battleship".into(),
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            accuracy: 100.0,
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
        };

        let h_ex = super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_explorer),
        );
        assert!((h_ex.unwrap() - 0.12).abs() < 1e-12);
        assert!(super::ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &imported,
            &effective,
            &catalog,
            false,
            Some(&ship_bb),
        )
        .is_none());
    }

    fn tiny_catalog(items: Vec<ForbiddenChaosRecord>) -> ForbiddenChaosList {
        ForbiddenChaosList {
            source: None,
            last_updated: None,
            items,
        }
    }

    #[test]
    fn resolve_effective_tech_fids_merges_forbidden_then_chaos_from_sync() {
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
        assert_eq!(fids, vec![10, 20]);
    }

    #[test]
    fn resolve_effective_tech_fids_skips_imported_fids_missing_from_catalog() {
        let catalog = tiny_catalog(vec![ForbiddenChaosRecord {
            fid: Some(1),
            name: "Only".into(),
            tech_type: "forbidden".into(),
            tier: None,
            bonuses: vec![],
        }]);
        let imported = vec![
            ForbiddenTechEntry {
                fid: 1,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
            ForbiddenTechEntry {
                fid: 999,
                tier: 1,
                level: 1,
                shard_count: 0,
            },
        ];
        let profile = PlayerProfile::default();
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![1]
        );
    }

    #[test]
    fn resolve_effective_tech_fids_empty_tech_type_is_forbidden() {
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
        let profile = PlayerProfile::default();
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![3]
        );
    }

    #[test]
    fn resolve_effective_tech_fids_forbidden_override_replaces_sync_forbidden_only() {
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
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![100, 2]
        );
    }

    #[test]
    fn resolve_effective_tech_fids_chaos_override_replaces_sync_chaos_only() {
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
            chaos_tech_override: Some(vec![200]),
            ..Default::default()
        };
        assert_eq!(
            resolve_effective_tech_fids(&profile, &imported, &catalog),
            vec![1, 200]
        );
    }
}
