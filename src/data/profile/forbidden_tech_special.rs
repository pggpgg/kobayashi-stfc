//! Forbidden-tech FIDs with non-flat combat routing (Borg Alcove, Operating Table,
//! Quantum Slipstream, ship-class torpedo family).

use std::collections::{HashMap, HashSet};

use crate::combat::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext, ShipType,
    TimingWindow, EPSILON, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::forbidden_chaos::ForbiddenChaosList;
use crate::data::import::ForbiddenTechEntry;
use crate::data::ship::ShipRecord;

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

/// FIDs whose catalog bonuses must not be merged as unconditional [`super::PlayerProfile::bonuses`].
#[inline]
pub(crate) fn skips_entire_flat_merge(fid: i64) -> bool {
    is_borg_alcove_forbidden_tech_fid(fid)
        || is_borg_operating_table_forbidden_tech_fid(fid)
        || is_ship_class_gated_torpedo_family_forbidden_tech_fid(fid)
}

/// Bonuses that exist in the catalog for sync/scaling but must not become unconditional profile modifiers.
#[inline]
pub(crate) fn skip_forbidden_tech_profile_bonus_for_fid(fid: i64, stat: &str) -> bool {
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
            _ if super::normalize_profile_combat_stat(stat).is_some() => {
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
    if super::normalize_profile_combat_stat(stat).is_some() {
        return Some(ForbiddenTechBonusCombatRoute::ProfileFlat);
    }
    None
}

pub(crate) fn forbidden_tech_bonus_value_for_imported_entry(
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
    super::scale_forbidden_tech_bonus_value_linear_by_level_tier(
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
                        weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
                        name: format!(
                            "forbidden_tech_borg_operating_table_hostile_crit_reduction_{idx}"
                        ),
                        class: AbilityClass::ShipAbility,
                        timing: TimingWindow::CombatBegin,
                        boostable: false,
                        effect: AbilityEffect::HostileCritDamageReduction {
                            reduction: v.clamp(0.0, 0.95),
                            duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
                            additive_percentage_points: false,
                            stacks: false,
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
            weapon_scope: Default::default(),
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
                    weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
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
                        weapon_scope: Default::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::forbidden_chaos::{BonusEntry, ForbiddenChaosList, ForbiddenChaosRecord};
    use crate::data::import::ForbiddenTechEntry;
    use crate::data::profile::{merge_tech_fids_into_profile_with_level_tier, PlayerProfile};
    use crate::data::ship::ShipRecord;

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
}
