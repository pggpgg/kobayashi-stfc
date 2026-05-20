//! Scenario and candidate → combat input: SharedScenarioData, scenario_to_combat_input, build_crew_and_buffs.

use std::collections::{HashMap, HashSet};

use crate::combat::{
    attacker_crew_tal_assigned_captain_or_bridge, mitigation, mitigation_for_hostile,
    pierce_damage_through_bonus, Ability, AbilityClass, AbilityEffect, AttackerStats, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats, EnemyTypes,
    HostileMitigationParams, OpponentFactionTag, ShipType, TimingWindow, MITIGATION_CEILING,
    MITIGATION_FLOOR, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::building::{
    self, BuildingAttackerFaction, BuildingBonusContext, BuildingDefenderOpponent, BuildingMode,
    DEFAULT_BUILDINGS_INDEX_PATH,
};
use crate::data::building_bid_resolver::{
    load_bid_to_building_id, DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
};
use crate::data::forbidden_chaos;
use crate::data::hostile::{ship_class_to_type, HostileRecord};
use crate::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, load_hostile_ability_catalog,
    DEFAULT_HOSTILE_ABILITY_CATALOG_PATH,
};
use crate::data::import;
use crate::data::loader::{resolve_hostile, resolve_ship};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::profile::{
    apply_profile_accuracy_to_attacker_stats, apply_profile_to_attacker,
    apply_static_buffs_to_combatant, borg_alcove_hull_hp_bonus_fraction,
    borg_operating_table_forbidden_tech_seats, forbidden_tech_derived_attack_phase_seats,
    forbidden_tech_level_tier_scaling_enabled_from_env, load_profile,
    merge_building_bonuses_into_profile, merge_research_bonuses_into_profile,
    merge_tech_fids_into_profile_with_level_tier,
    quantum_slipstream_forbidden_tech_round_start_seats, research_derived_attack_phase_seats,
    research_levels_by_rid_from_import, resolve_effective_tech_fids,
    ship_class_gated_torpedo_family_derived_seats,
    ship_class_gated_torpedo_family_hostile_accuracy_sum_for_resolved_ship,
    ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship,
    ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship, PlayerProfile,
    SupportBuffResearchGateState, USS_VOYAGER_SHIP_ID,
};
use crate::data::profile_index::{
    self, profile_path, BUILDINGS_IMPORTED, FORBIDDEN_TECH_IMPORTED, PROFILE_JSON,
    RESEARCH_IMPORTED, ROSTER_IMPORTED,
};
use crate::data::research::{
    cumulative_dual_gate_hull_shield_research_fractions, load_research_canonical_overrides,
    load_research_catalog, ResearchRecord, DEFAULT_RESEARCH_CANONICAL_PATH,
    DEFAULT_RESEARCH_CATALOG_PATH,
};
use crate::data::research_effect_spec_adapter::incoming_shield_mitigation_for_combat;
use crate::data::ship::ShipRecord;
use crate::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;
use crate::data::support_buffs::{self, AppliedSupportBuffTrace, SupportBuffCatalog};
use crate::lcars::{
    index_lcars_officers_by_id, load_lcars_dir, resolve_crew_to_buff_set, ResolveOptions,
};
use crate::optimizer::crew_generator::{
    CrewCandidate, BRIDGE_SLOTS, MAX_BELOW_DECKS_SLOTS, MIN_BELOW_DECKS_SLOTS,
};
use std::path::Path;

use super::crew_resolution::{
    build_crew_seats, hash_identifier, index_officers_by_name, normalize_lookup_key,
    roster_officer_ids_from_candidate, split_name_and_tier,
};

const DEFAULT_LCARS_OFFICERS_DIR_STANDALONE: &str = "data/officers";

/// Who the defending combatant represents for canonical opponent-category conditions (`EnemyHostile` / `EnemyPlayer`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefenderOpponent {
    /// NPC hostile (default ship-vs-hostile optimizer); `EnemyHostile` gates pass.
    #[default]
    Hostile,
    /// Player ship (“PvP-shaped” toggle); `EnemyPlayer` gates pass. Stats may still come from a hostile id until real PvP symmetry exists.
    Player,
}

impl DefenderOpponent {
    pub fn defender_is_npc_hostile(self) -> bool {
        matches!(self, Self::Hostile)
    }

    pub fn defender_is_player_ship(self) -> bool {
        matches!(self, Self::Player)
    }
}

/// Ship-class torpedo family: add summed catalog accuracy fractions to [`PlayerProfile::bonuses`] when the
/// scenario defender is a hostile (per-fid values are resolved in profile helpers using `ship_rec`).
fn apply_class_gated_torpedo_family_hostile_accuracy_to_profile(
    profile: &mut PlayerProfile,
    defender_opponent: DefenderOpponent,
    acc_frac_sum: Option<f64>,
) {
    let Some(a) = acc_frac_sum else {
        return;
    };
    if defender_opponent != DefenderOpponent::Hostile {
        return;
    }
    if !a.is_finite() || a <= 0.0 {
        return;
    }
    let cur = profile.bonuses.get("accuracy").copied().unwrap_or(0.0);
    profile.bonuses.insert("accuracy".to_string(), cur + a);
}

/// Append the ship [`ShipRecord`]'s optional `abilities` as [`CrewSeatContext`] rows
/// (supported timing/`effect_type` only; see [`crate::data::ship_ability_resolve`]).
/// Called after officer seats so hull abilities use the same engine phases as DESIGN.md §3.6.
fn extend_crew_with_ship_abilities(
    seats: &mut Vec<CrewSeatContext>,
    ship_rec: Option<&ShipRecord>,
) {
    let Some(rec) = ship_rec else {
        return;
    };
    seats.extend(ship_abilities_to_crew_seat_contexts(
        rec.abilities.as_deref().unwrap_or(&[]),
    ));
}

/// Pull isolytic cascade fractions from merged LCARS/support static buffs so they are not dropped by
/// [`apply_static_buffs_to_combatant`] (which has no [`Combatant`] field for cascade).
fn take_isolytic_cascade_static_bonus(static_buffs: &mut HashMap<String, f64>) -> f64 {
    let a = static_buffs
        .remove("isolytic_cascade_damage")
        .unwrap_or(0.0);
    let b = static_buffs.remove("isolytic_cascade").unwrap_or(0.0);
    let s = a + b;
    if s.is_finite() {
        s.max(0.0)
    } else {
        0.0
    }
}

/// Profile + static isolytic cascade bonuses as one always-on attack-phase seat (additive with officer cascade).
fn extend_crew_with_isolytic_cascade_profile_and_static(
    seats: &mut Vec<CrewSeatContext>,
    profile: &PlayerProfile,
    static_cascade: f64,
) {
    let p = profile
        .bonuses
        .get("isolytic_cascade_damage")
        .copied()
        .unwrap_or(0.0);
    let v = p + static_cascade;
    if !v.is_finite() || v <= 0.0 {
        return;
    }
    seats.push(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: "profile_isolytic_cascade_damage".to_string(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::AttackPhase,
            boostable: false,
            effect: AbilityEffect::IsolyticCascadeDamageBonus(v),
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
}

fn extend_crew_with_research_derived_attack_phase_seats(
    seats: &mut Vec<CrewSeatContext>,
    derived: &[CrewSeatContext],
) {
    if derived.is_empty() {
        return;
    }
    seats.extend(derived.iter().cloned());
}

/// Building/profile conditional scalar: Apex Barrier only in PvP scenarios and only when Tal is
/// **not** assigned on Captain/Bridge (DTI Headquarters copy).
fn apply_profile_player_apex_barrier_tal_gate(
    attacker: &mut Combatant,
    profile: &PlayerProfile,
    defender_opponent: DefenderOpponent,
    attacker_tal_assigned_captain_or_bridge: bool,
) {
    if defender_opponent != DefenderOpponent::Player || attacker_tal_assigned_captain_or_bridge {
        return;
    }
    let v = profile
        .bonuses
        .get("apex_barrier_vs_player_tal_not_on_bridge")
        .copied()
        .unwrap_or(0.0);
    if v.is_finite() && v != 0.0 {
        attacker.apex_barrier += v;
    }
}

/// Building/profile conditional effect: reduce incoming crit damage from opponent player ships.
/// Implemented as an always-on combat-begin seat in PvP scenarios.
fn extend_crew_with_player_crit_damage_reduction_profile_bonus(
    seats: &mut Vec<CrewSeatContext>,
    profile: &PlayerProfile,
    defender_opponent: DefenderOpponent,
) {
    if defender_opponent != DefenderOpponent::Player {
        return;
    }
    let v = profile
        .bonuses
        .get("player_crit_damage_reduction")
        .copied()
        .unwrap_or(0.0);
    if !v.is_finite() || v <= 0.0 {
        return;
    }
    seats.push(CrewSeatContext {
        seat: CrewSeat::Ship,
        ability: Ability {
            name: "profile_player_crit_damage_reduction".to_string(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::CombatBegin,
            boostable: false,
            effect: AbilityEffect::HostileCritDamageReduction {
                reduction: v.clamp(0.0, 0.95),
                duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
            },
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
}

/// [`AttackerStats`] for hostile mitigation and player pierce-through: ship components, profile
/// accuracy multiplier, LCARS static keys (`accuracy`, `accuracy_cb_mult` from passive or combat-begin
/// `stat_modify`), and combat-begin hull [`ShipAbility`] accuracy.
pub(crate) fn effective_attacker_stats_for_mitigation(
    ship_rec: &ShipRecord,
    profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
    defender_ship_type: ShipType,
) -> AttackerStats {
    let mut s = ship_rec.to_attacker_stats();
    apply_profile_accuracy_to_attacker_stats(&mut s, profile);
    s.accuracy += static_buffs.get("accuracy").copied().unwrap_or(0.0);
    s.accuracy += crate::data::ship_ability_resolve::sum_combat_begin_accuracy_from_ship_abilities(
        ship_rec.abilities.as_deref().unwrap_or(&[]),
        defender_ship_type,
    );
    let cbm = static_buffs.get("accuracy_cb_mult").copied().unwrap_or(1.0);
    if cbm.is_finite() && cbm > 0.0 {
        s.accuracy *= cbm;
    }
    s
}

/// Merge optional per-weapon piercing/accuracy from [`crate::data::ship::WeaponRecord`] with tier
/// aggregates, then apply the same accuracy bonuses as [`effective_attacker_stats_for_mitigation`].
fn effective_attacker_stats_for_ship_weapon_row(
    ship_rec: &ShipRecord,
    row: Option<&crate::data::ship::WeaponRecord>,
    profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
    defender_ship_type: ShipType,
) -> AttackerStats {
    let base = ship_rec.to_attacker_stats();
    let mut s = if let Some(w) = row {
        AttackerStats {
            armor_piercing: w.armor_piercing.unwrap_or(base.armor_piercing),
            shield_piercing: w.shield_piercing.unwrap_or(base.shield_piercing),
            accuracy: w.accuracy.unwrap_or(base.accuracy),
        }
    } else {
        base
    };
    apply_profile_accuracy_to_attacker_stats(&mut s, profile);
    s.accuracy += static_buffs.get("accuracy").copied().unwrap_or(0.0);
    s.accuracy += crate::data::ship_ability_resolve::sum_combat_begin_accuracy_from_ship_abilities(
        ship_rec.abilities.as_deref().unwrap_or(&[]),
        defender_ship_type,
    );
    let cbm = static_buffs.get("accuracy_cb_mult").copied().unwrap_or(1.0);
    if cbm.is_finite() && cbm > 0.0 {
        s.accuracy *= cbm;
    }
    s
}

/// When normalized weapon rows include per-component piercing or accuracy, set each
/// [`WeaponStats::pierce`] to the toolbox damage-through term for that row vs this hostile.
/// Mitigation for the fight still uses tier-averaged [`ShipRecord::to_attacker_stats`] (unchanged).
pub(crate) fn ship_weapons_with_resolved_pierce_through(
    ship_rec: &ShipRecord,
    hostile_rec: &HostileRecord,
    profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
) -> Vec<crate::combat::WeaponStats> {
    let mut weapons = ship_rec.to_weapons();
    let Some(rows) = ship_rec.weapons.as_ref() else {
        return weapons;
    };
    if rows.is_empty() || weapons.is_empty() {
        return weapons;
    }
    let any = rows
        .iter()
        .any(|w| w.armor_piercing.is_some() || w.shield_piercing.is_some() || w.accuracy.is_some());
    if !any {
        return weapons;
    }
    let ship_type = hostile_rec.ship_type_for_combat();
    let defender = hostile_rec.to_defender_stats();
    for (i, w) in weapons.iter_mut().enumerate() {
        let row = rows.get(i);
        let has_row = row.is_some_and(|r| {
            r.armor_piercing.is_some() || r.shield_piercing.is_some() || r.accuracy.is_some()
        });
        if !has_row {
            continue;
        }
        let stats = effective_attacker_stats_for_ship_weapon_row(
            ship_rec,
            row,
            profile,
            static_buffs,
            ship_type,
        );
        w.pierce = Some(pierce_damage_through_bonus(defender, stats, ship_type));
    }
    weapons
}

/// Attacker outbound weapons with per-row pierce vs a player defender ship.
pub(crate) fn ship_weapons_with_pierce_vs_defender_ship(
    ship_rec: &ShipRecord,
    defender_ship: &ShipRecord,
    profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
) -> Vec<crate::combat::WeaponStats> {
    let mut weapons = ship_rec.to_weapons();
    let defender_stats = defender_ship.to_defender_stats();
    let ship_type = defender_ship.ship_type();
    let Some(rows) = ship_rec.weapons.as_ref() else {
        return weapons;
    };
    if rows.is_empty() || weapons.is_empty() {
        return weapons;
    }
    let any = rows
        .iter()
        .any(|w| w.armor_piercing.is_some() || w.shield_piercing.is_some() || w.accuracy.is_some());
    if !any {
        return weapons;
    }
    for (i, w) in weapons.iter_mut().enumerate() {
        let row = rows.get(i);
        let has_row = row.is_some_and(|r| {
            r.armor_piercing.is_some() || r.shield_piercing.is_some() || r.accuracy.is_some()
        });
        if !has_row {
            continue;
        }
        let stats = effective_attacker_stats_for_ship_weapon_row(
            ship_rec,
            row,
            profile,
            static_buffs,
            ship_type,
        );
        w.pierce = Some(pierce_damage_through_bonus(defender_stats, stats, ship_type));
    }
    weapons
}

pub(crate) fn mitigation_and_pierce_for_player_vs_hostile(
    ship_rec: &ShipRecord,
    hostile_rec: &HostileRecord,
    profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
) -> (f64, f64) {
    let ship_type = hostile_rec.ship_type_for_combat();
    let attacker_stats =
        effective_attacker_stats_for_mitigation(ship_rec, profile, static_buffs, ship_type);
    let defender_stats = hostile_rec.to_defender_stats();
    let defender_mitigation = mitigation_for_hostile(
        defender_stats,
        attacker_stats,
        ship_type,
        hostile_rec.mystery_mitigation_factor.unwrap_or(0.0),
        hostile_rec.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
        hostile_rec.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
    );
    let pierce = pierce_damage_through_bonus(defender_stats, attacker_stats, ship_type);
    (defender_mitigation, pierce)
}

/// Outbound mitigation and pierce for player attacker vs player defender ship (no hostile mystery factor).
pub fn mitigation_and_pierce_for_player_vs_player(
    attacker_ship: &ShipRecord,
    defender_ship: &ShipRecord,
    attacker_profile: &PlayerProfile,
    static_buffs: &HashMap<String, f64>,
) -> (f64, f64) {
    let ship_type = defender_ship.ship_type();
    let attacker_stats = effective_attacker_stats_for_mitigation(
        attacker_ship,
        attacker_profile,
        static_buffs,
        ship_type,
    );
    let defender_stats = defender_ship.to_defender_stats();
    let defender_mitigation = mitigation_for_hostile(
        defender_stats,
        attacker_stats,
        ship_type,
        0.0,
        MITIGATION_FLOOR,
        MITIGATION_CEILING,
    );
    let pierce = pierce_damage_through_bonus(defender_stats, attacker_stats, ship_type);
    (defender_mitigation, pierce)
}

/// Per-weapon pierce for defender ship counter-fire vs attacker hull class.
pub(crate) fn ship_weapons_with_pierce_vs_player_defender(
    defender_ship: &ShipRecord,
    defender_profile: &PlayerProfile,
    defender_static_buffs: &HashMap<String, f64>,
    attacker_ship_type: ShipType,
    attacker_defender_stats: DefenderStats,
) -> Vec<crate::combat::WeaponStats> {
    let mut weapons = defender_ship.to_weapons();
    let Some(rows) = defender_ship.weapons.as_ref() else {
        let stats = effective_attacker_stats_for_mitigation(
            defender_ship,
            defender_profile,
            defender_static_buffs,
            attacker_ship_type,
        );
        let pierce = pierce_damage_through_bonus(attacker_defender_stats, stats, attacker_ship_type);
        for w in &mut weapons {
            if w.pierce.is_none() {
                w.pierce = Some(pierce);
            }
        }
        return weapons;
    };
    if rows.is_empty() || weapons.is_empty() {
        return weapons;
    }
    for (i, w) in weapons.iter_mut().enumerate() {
        let row = rows.get(i);
        let stats = effective_attacker_stats_for_ship_weapon_row(
            defender_ship,
            row,
            defender_profile,
            defender_static_buffs,
            attacker_ship_type,
        );
        w.pierce = Some(pierce_damage_through_bonus(
            attacker_defender_stats,
            stats,
            attacker_ship_type,
        ));
    }
    weapons
}

/// Ship-vs-ship defender [`Combatant`] (counter-fire uses defender weapons vs attacker `to_defender_stats`).
fn defender_combatant_from_ship_record(
    defender_lookup_id: &str,
    defender_ship: &ShipRecord,
    defender_mitigation: f64,
    attacker_ship_type: ShipType,
    attacker_defender_stats: DefenderStats,
    defender_profile: &PlayerProfile,
    defender_static_buffs: &HashMap<String, f64>,
) -> Combatant {
    let weapons = ship_weapons_with_pierce_vs_player_defender(
        defender_ship,
        defender_profile,
        defender_static_buffs,
        attacker_ship_type,
        attacker_defender_stats,
    );
    let attack = if weapons.is_empty() {
        defender_ship.attack
    } else {
        0.0
    };
    let counter_stats = effective_attacker_stats_for_mitigation(
        defender_ship,
        defender_profile,
        defender_static_buffs,
        attacker_ship_type,
    );
    let pierce = pierce_damage_through_bonus(
        attacker_defender_stats,
        counter_stats,
        attacker_ship_type,
    );
    Combatant {
        id: defender_lookup_id.to_string(),
        attack,
        mitigation: defender_mitigation,
        pierce,
        crit_chance: defender_ship.crit_chance,
        crit_multiplier: defender_ship.crit_damage,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: defender_ship.hull_health,
        shield_health: defender_ship.shield_health,
        shield_mitigation: defender_ship.shield_mitigation.unwrap_or(0.8),
        apex_barrier: 0.0,
        apex_shred: defender_ship.apex_shred,
        isolytic_damage: defender_ship.isolytic_damage,
        isolytic_defense: 0.0,
        weapons,
        hostile_mitigation_params: None,
    }
}

/// PvP scenario: fixed defender player ship + opponent profile (optimize searches attacker crews only).
#[derive(Debug, Clone)]
pub struct PvpScenarioParams {
    pub defender_ship: String,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    pub defender_profile_id: String,
}

/// Build PvP params when API validation has already ensured `defender_profile_id` is present.
pub fn pvp_scenario_params_from_api_fields(
    defender_ship: Option<&str>,
    defender_ship_tier: Option<u32>,
    defender_ship_level: Option<u32>,
    defender_profile_id: Option<&str>,
) -> Option<PvpScenarioParams> {
    let ds = defender_ship?.trim();
    if ds.is_empty() {
        return None;
    }
    let pid = defender_profile_id?.trim();
    if pid.is_empty() {
        return None;
    }
    Some(PvpScenarioParams {
        defender_ship: ds.to_string(),
        defender_ship_tier,
        defender_ship_level,
        defender_profile_id: pid.to_string(),
    })
}

/// Defender [`Combatant`] from [`HostileRecord`], including weapons and pierce/crit used on the
/// engine’s counter-attack path (see `engine.rs` — hostile fire vs player).
///
/// `player_defender` is sourced from [`crate::data::ShipRecord::to_defender_stats`] and feeds
/// hostile→player pierce-through (per-weapon and scalar). Until upstream ship data fills raw
/// armor/shield/dodge values these are zero and the formula collapses to the historical constant;
/// once they are populated, hostile `accuracy` and piercing actually move counter-fire pierce-through.
fn defender_combatant_from_hostile_record(
    hostile_lookup_id: &str,
    hostile_rec: &HostileRecord,
    defender_mitigation: f64,
    player_ship_type: ShipType,
    player_defender: DefenderStats,
    hostile_mitigation_params: Option<HostileMitigationParams>,
) -> Combatant {
    let weapons = hostile_rec.weapons_for_counter_attack(player_ship_type, player_defender);
    let attack = if weapons.is_empty() {
        hostile_rec.scalar_attack_fallback()
    } else {
        0.0
    };
    let pierce = hostile_rec.counter_pierce_damage_through_bonus(player_ship_type, player_defender);
    let crit_chance = hostile_rec.crit_chance.clamp(0.0, 1.0);
    let crit_multiplier = if hostile_rec.crit_damage.is_finite() && hostile_rec.crit_damage > 0.0 {
        hostile_rec.crit_damage
    } else {
        1.0
    };
    Combatant {
        id: hostile_lookup_id.to_string(),
        attack,
        mitigation: defender_mitigation,
        pierce,
        crit_chance,
        crit_multiplier,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: hostile_rec.hull_health,
        shield_health: hostile_rec.shield_health,
        shield_mitigation: hostile_rec.shield_mitigation.unwrap_or(0.8),
        apex_barrier: hostile_rec.apex_barrier,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: hostile_rec.isolytic_defense,
        weapons,
        hostile_mitigation_params,
    }
}

fn use_lcars_officer_source_standalone() -> bool {
    std::env::var("KOBAYASHI_OFFICER_SOURCE")
        .map(|v| v.eq_ignore_ascii_case("lcars"))
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub(crate) struct LcarsOfficerData {
    pub by_id: HashMap<String, crate::lcars::LcarsOfficer>,
    /// Normalized lookup → canonical LCARS id. Keys include both officer [`LcarsOfficer::name`] and
    /// [`LcarsOfficer::id`] so API/optimizer candidates can use either display names or slugs.
    pub name_to_id: HashMap<String, String>,
}

fn lcars_officer_data_from_officers(officers: Vec<crate::lcars::LcarsOfficer>) -> LcarsOfficerData {
    let by_id = index_lcars_officers_by_id(officers);
    let mut name_to_id: HashMap<String, String> = HashMap::new();
    for o in by_id.values() {
        name_to_id.insert(normalize_lookup_key(&o.name), o.id.clone());
        name_to_id.insert(normalize_lookup_key(&o.id), o.id.clone());
    }
    LcarsOfficerData { by_id, name_to_id }
}

/// Pre-resolved data for (ship, hostile) shared across all candidates in one Monte Carlo run.
#[derive(Clone)]
pub(crate) struct SharedScenarioData {
    pub ship: String,
    pub hostile: String,
    pub officer_index: HashMap<String, Officer>,
    pub profile: PlayerProfile,
    pub lcars_data: Option<LcarsOfficerData>,
    pub resolve_options: ResolveOptions,
    pub ship_rec: Option<ShipRecord>,
    #[allow(dead_code)]
    pub hostile_rec: Option<HostileRecord>,
    pub cached_defender: Option<Combatant>,
    pub cached_rounds: Option<u32>,
    pub cached_defender_hull: Option<f64>,
    pub cached_pierce: Option<f64>,
    #[allow(dead_code)]
    pub cached_defender_mitigation: Option<f64>,
    /// True when ship or hostile did not resolve from data and [`scenario_to_combat_input_from_shared`]
    /// uses hashed placeholder combatants instead of registry-backed stats.
    pub using_placeholder_combatants: bool,
    /// Resolved support buff ids (after exclusive-group rules).
    #[allow(dead_code)]
    pub resolved_support_buffs: Vec<String>,
    /// Display/debug metadata for resolved support buffs, included in trace replay output.
    pub applied_support_buffs: Vec<AppliedSupportBuffTrace>,
    /// Static combat keys from support buff definitions; merged with crew LCARS static buffs on the attacker.
    pub support_static_buffs: HashMap<String, f64>,
    /// Support buff `static_bonuses` that apply to the defender only when [`Self::defender_opponent`] is [`DefenderOpponent::Player`].
    pub support_defender_static_buffs: HashMap<String, f64>,
    /// Request ids not present in the support buff catalog (for API warnings).
    #[allow(dead_code)]
    pub unknown_support_buff_ids: Vec<String>,
    /// Conditional research (`crit_chance` / `crit_damage` with hull/faction/morale/etc. gates).
    pub research_derived_seats: Vec<CrewSeatContext>,
    /// Borg Alcove attack-phase crit seats, Quantum Slipstream round-start debuff, ship-class torpedo family
    /// (S31 / Control Seeker / Dual Photon) combat-begin / attack-phase seats; hull extras via
    /// [`Self::borg_alcove_hull_hp_bonus`] and [`Self::class_gated_torpedo_family_hull_hp_bonus`].
    pub forbidden_tech_derived_seats: Vec<CrewSeatContext>,
    /// Borg Alcove hull catalog bonus (fraction) when tech is equipped; multiply Voyager hull only in scenario build.
    pub borg_alcove_hull_hp_bonus: Option<f64>,
    /// Sum of hull catalog bonuses for equipped torpedo-family techs matching the resolved ship hull class.
    pub class_gated_torpedo_family_hull_hp_bonus: Option<f64>,
    /// Sum of shield deflection adds for matching-hull family techs; applied vs **hostile** (`defender_opponent`).
    pub class_gated_torpedo_family_hostile_shield_mitigation_sum: Option<f64>,
    /// Canonical `EnemyHostile` / `EnemyPlayer` condition context for the defending side.
    pub defender_opponent: DefenderOpponent,
    /// Resolved player hull owner faction for research / analytical gates (`ShipRecord::faction`).
    pub attacker_owner_faction: OpponentFactionTag,
    /// Extra `hull_hp` fraction from research gated on **both** owner faction and `defender_faction` (scenario-only).
    pub dual_gate_research_hull_hp: f64,
    /// Extra `shield_hp` fraction from dual-gated research (same semantics as [`Self::dual_gate_research_hull_hp`]).
    pub dual_gate_research_shield_hp: f64,
    /// STFC engagement tags for [`crate::combat::SimulationConfig::engagement_enemy_types`] (armada solo/group, …).
    pub engagement_enemy_types: EnemyTypes,
    /// Optional hostile level for canonical `TargetMaxLevel`.
    pub defender_level: Option<u32>,
    /// Incoming (counter-fire) shield mitigation from canonical research (e.g. KSG early-round SM).
    pub incoming_shield_mitigation_bonus: f64,
    pub incoming_shield_mitigation_bonus_rounds: u32,
    /// Optional defender-side officer seats (LCARS); merged after hostile ship abilities in combat input.
    pub player_defender_officer_seats: Vec<CrewSeatContext>,
    /// Static buff map from optional defender officer LCARS; merged onto defender [`Combatant`] at scenario build.
    pub player_defender_static_buffs: HashMap<String, f64>,
    /// Summed A/D/H from optional defender officer crew (PvP defender-side officer-stat pipeline).
    pub defender_officer_stat_totals: crate::combat::CrewOfficerStatTotals,
    /// Captain + bridge subset of [`Self::defender_officer_stat_totals`].
    pub defender_bridge_officer_stat_totals: crate::combat::CrewOfficerStatTotals,
    /// Conditional / `target: enemy` officer-stat rows from defender crew (Phase 4b/4c).
    pub defender_pending_officer_stat_contributions: Vec<crate::lcars::PendingOfficerStatContribution>,
    /// When set, defender stats come from [`Self::defender_ship_rec`] + [`Self::defender_profile`] (ship-vs-ship PvP).
    pub pvp: Option<PvpScenarioParams>,
    pub defender_ship_rec: Option<ShipRecord>,
    pub defender_profile: Option<PlayerProfile>,
    /// Incoming shield mitigation on the **attacker** from counter-fire (defender profile research).
    pub defender_incoming_shield_mitigation_bonus: f64,
    pub defender_incoming_shield_mitigation_bonus_rounds: u32,
}

fn attacker_owner_faction_from_ship(ship_rec: Option<&ShipRecord>) -> OpponentFactionTag {
    ship_rec
        .and_then(|s| s.faction.as_deref())
        .and_then(OpponentFactionTag::from_data_slug)
        .unwrap_or(OpponentFactionTag::Unknown)
}

fn dual_gate_hull_shield_for_scenario(
    catalog: Option<&crate::data::research::ResearchCatalog>,
    imported_research: &[import::ResearchEntry],
    exclude_canonical_rids: &HashSet<i64>,
    ship_rec: Option<&ShipRecord>,
    defender_faction: OpponentFactionTag,
) -> (f64, f64) {
    let Some(cat) = catalog else {
        return (0.0, 0.0);
    };
    let mut levels = research_levels_by_rid_from_import(imported_research);
    levels.retain(|rid, _| !exclude_canonical_rids.contains(rid));
    let records: Vec<&ResearchRecord> = cat
        .items
        .iter()
        .filter(|r| levels.contains_key(&r.rid))
        .collect();
    cumulative_dual_gate_hull_shield_research_fractions(
        &records,
        &levels,
        ship_rec.and_then(|s| s.faction.as_deref()),
        defender_faction,
    )
}

fn defender_faction_tag_for_scenario(
    hostile_rec: Option<&HostileRecord>,
    defender_ship_rec: Option<&ShipRecord>,
) -> OpponentFactionTag {
    if let Some(h) = hostile_rec {
        return h.opponent_faction_tag();
    }
    defender_ship_rec
        .and_then(|s| s.faction.as_deref())
        .and_then(OpponentFactionTag::from_data_slug)
        .unwrap_or(OpponentFactionTag::Unknown)
}

fn apply_dual_gate_hull_shield_research(attacker: &mut Combatant, shared: &SharedScenarioData) {
    if shared.dual_gate_research_hull_hp != 0.0 {
        attacker.hull_health *= 1.0 + shared.dual_gate_research_hull_hp;
    }
    if shared.dual_gate_research_shield_hp != 0.0 {
        attacker.shield_health *= 1.0 + shared.dual_gate_research_shield_hp;
    }
}

impl SharedScenarioData {
    /// Hostile hull class for ability conditions and mitigation (defaults when no hostile record).
    pub(crate) fn defender_ship_type_for_combat(&self) -> ShipType {
        if let Some(ref ds) = self.defender_ship_rec {
            return ds.ship_type();
        }
        self.hostile_rec
            .as_ref()
            .map(|h| h.ship_type_for_combat())
            .unwrap_or(ShipType::Battleship)
    }

    pub(crate) fn is_pvp(&self) -> bool {
        self.pvp.is_some()
    }

    /// Hostile tag bitmask for [`crate::combat::SimulationConfig::defender_hostile_tag_mask`] (0 when no hostile record or no tags).
    pub(crate) fn defender_hostile_tag_mask_for_combat(&self) -> u32 {
        self.hostile_rec
            .as_ref()
            .map(|h| h.hostile_tag_mask())
            .unwrap_or(0)
    }

    /// Player hull class for hostile-side ability conditions (defaults when no ship record).
    pub(crate) fn attacker_ship_type_for_combat(&self) -> ShipType {
        self.ship_rec
            .as_ref()
            .map(|s| ship_class_to_type(&s.ship_class))
            .unwrap_or(ShipType::Battleship)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CombatSimulationInput {
    pub attacker: Combatant,
    pub defender: Combatant,
    /// Defender-side effects (hostile upstream abilities) applied to return fire.
    /// Empty by default; populated only when a hostile ability id is mapped in the hostile ability catalog.
    pub defender_crew: CrewConfiguration,
    pub crew: CrewConfiguration,
    pub rounds: u32,
    pub defender_hull: f64,
    pub base_seed: u64,
    /// Copied into [`crate::combat::SimulationConfig::weapon_damage_profile_additive_pool`] when set
    /// (e.g. from `KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL=1` at scenario build).
    pub weapon_damage_profile_additive_pool: Option<f64>,
    /// Profile `weapon_damage` fraction `p` for [`crate::combat::SimulationConfig::profile_weapon_damage_fraction`].
    pub profile_weapon_damage_fraction: f64,
    /// Copied into [`crate::combat::SimulationConfig::engagement_enemy_types`].
    pub engagement_enemy_types: EnemyTypes,
    /// Copied into [`crate::combat::SimulationConfig::defender_level`].
    pub defender_level: Option<u32>,
    /// Copied into [`crate::combat::SimulationConfig::attacker_roster_officer_ids`] (Evolutionary Assimilation).
    pub attacker_roster_officer_ids: Vec<String>,
    pub incoming_shield_mitigation_bonus: f64,
    pub incoming_shield_mitigation_bonus_rounds: u32,
    /// Copied into [`crate::combat::SimulationConfig::attacker_owner_faction`].
    pub attacker_owner_faction: OpponentFactionTag,
}

/// PvP defender [`Combatant`] with opponent profile + defender crew officer-stat runtime,
/// including Phase 4c debuffs from the attacker's `target: enemy` pending rows.
fn build_pvp_defender_combatant(
    shared: &SharedScenarioData,
    attacker_ship_rec: &ShipRecord,
    defender_mitigation: f64,
    attacker_enemy_target_pending: &[crate::lcars::PendingOfficerStatContribution],
) -> Combatant {
    let def_ship = shared
        .defender_ship_rec
        .as_ref()
        .expect("pvp defender ship");
    let def_profile = shared
        .defender_profile
        .as_ref()
        .expect("pvp defender profile");
    let defender_id = shared
        .pvp
        .as_ref()
        .map(|p| p.defender_ship.as_str())
        .unwrap_or(shared.hostile.as_str());
    let mut defender = defender_combatant_from_ship_record(
        defender_id,
        def_ship,
        defender_mitigation,
        attacker_ship_rec.ship_type(),
        attacker_ship_rec.to_defender_stats(),
        def_profile,
        &shared.player_defender_static_buffs,
    );
    let cond_ctx = build_officer_stat_condition_context(shared, attacker_ship_rec);
    let defender_osr = crate::data::profile::compute_officer_stat_runtime_bonus(
        shared.defender_officer_stat_totals,
        shared.defender_bridge_officer_stat_totals,
        def_ship,
        def_profile,
        def_ship.faction.as_deref(),
        &shared.player_defender_static_buffs,
        &shared.defender_pending_officer_stat_contributions,
        &cond_ctx,
        attacker_enemy_target_pending,
    );
    defender = crate::data::profile::apply_profile_to_attacker(
        defender,
        def_profile,
        def_ship.faction.as_deref(),
        defender_osr,
    );
    apply_support_defender_static_if_pvp(shared, &mut defender);
    if !shared.player_defender_static_buffs.is_empty() {
        let mut dstatic = shared.player_defender_static_buffs.clone();
        let _ = take_isolytic_cascade_static_bonus(&mut dstatic);
        defender = apply_static_buffs_to_combatant(defender, &dstatic);
    }
    defender
}

fn apply_support_defender_static_if_pvp(shared: &SharedScenarioData, defender: &mut Combatant) {
    if shared.defender_opponent != DefenderOpponent::Player {
        return;
    }
    if shared.support_defender_static_buffs.is_empty() {
        return;
    }
    *defender =
        apply_static_buffs_to_combatant(defender.clone(), &shared.support_defender_static_buffs);
}

/// Build combat input from pre-resolved shared data and candidate. Resolves ship/hostile only once per run.
pub(crate) fn scenario_to_combat_input_from_shared(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
    seed: u64,
) -> CombatSimulationInput {
    let base_seed = stable_seed(
        &shared.ship,
        &shared.hostile,
        &candidate.captain,
        &candidate.bridge,
        &candidate.below_decks,
        seed,
    );
    let attacker_roster_officer_ids =
        roster_officer_ids_from_candidate(candidate, &shared.officer_index);

    let resolve_opts = resolve_options_with_candidate_tiers(
        &shared.resolve_options,
        candidate,
        shared.lcars_data.as_ref(),
    );
    let (
        crew_seats,
        static_buffs,
        proc_chance,
        proc_multiplier,
        officer_stat_totals,
        bridge_officer_stat_totals,
        pending_officer_stat_contributions,
    ) = build_crew_and_buffs(
        candidate,
        &shared.officer_index,
        shared.lcars_data.as_ref(),
        &resolve_opts,
    );
    let mut merged_static =
        support_buffs::merge_static_buff_maps(&static_buffs, &shared.support_static_buffs);
    let static_cascade_bonus = take_isolytic_cascade_static_bonus(&mut merged_static);
    let attacker_tal_assigned_captain_or_bridge =
        attacker_crew_tal_assigned_captain_or_bridge(&CrewConfiguration {
            seats: crew_seats.clone(),
        });

    let hostile_ability_catalog =
        load_hostile_ability_catalog(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
    let mut defender_crew = if shared.is_pvp() {
        let mut seats = Vec::new();
        extend_crew_with_ship_abilities(&mut seats, shared.defender_ship_rec.as_ref());
        CrewConfiguration { seats }
    } else {
        shared
            .hostile_rec
            .as_ref()
            .map(|h| {
                hostile_abilities_to_defender_crew(&h.ability, hostile_ability_catalog.as_ref())
            })
            .unwrap_or_else(|| CrewConfiguration { seats: Vec::new() })
    };
    defender_crew
        .seats
        .extend_from_slice(&shared.player_defender_officer_seats);

    if let (Some(ref ship_rec), Some(ref cached_defender), Some(rounds), Some(defender_hull)) = (
        &shared.ship_rec,
        &shared.cached_defender,
        shared.cached_rounds,
        shared.cached_defender_hull,
    ) {
        let (defender_mitigation, pierce) = if shared.is_pvp() {
            shared.defender_ship_rec.as_ref().map(|def_ship| {
                mitigation_and_pierce_for_player_vs_player(
                    ship_rec,
                    def_ship,
                    &shared.profile,
                    &merged_static,
                )
            })
        } else {
            shared.hostile_rec.as_ref().map(|h| {
                mitigation_and_pierce_for_player_vs_hostile(
                    ship_rec,
                    h,
                    &shared.profile,
                    &merged_static,
                )
            })
        }
        .unwrap_or_else(|| {
            (
                shared
                    .cached_defender_mitigation
                    .unwrap_or(cached_defender.mitigation),
                shared.cached_pierce.unwrap_or(0.0),
            )
        });
        let defender = if shared.is_pvp() {
            build_pvp_defender_combatant(
                shared,
                ship_rec,
                defender_mitigation,
                &pending_officer_stat_contributions,
            )
        } else {
            let mut d = cached_defender.clone();
            d.mitigation = defender_mitigation;
            apply_support_defender_static_if_pvp(shared, &mut d);
            if !shared.player_defender_static_buffs.is_empty() {
                let mut dstatic = shared.player_defender_static_buffs.clone();
                let _ = take_isolytic_cascade_static_bonus(&mut dstatic);
                d = apply_static_buffs_to_combatant(d, &dstatic);
            }
            d
        };

        let cond_ctx = build_officer_stat_condition_context(shared, ship_rec);
        let mut attacker = apply_profile_to_attacker(
            Combatant {
                id: shared.ship.clone(),
                attack: ship_rec.attack,
                mitigation: 0.0,
                pierce,
                crit_chance: ship_rec.crit_chance,
                crit_multiplier: ship_rec.crit_damage,
                proc_chance,
                proc_multiplier,
                end_of_round_damage: 0.0,
                hull_health: ship_rec.hull_health,
                shield_health: ship_rec.shield_health,
                shield_mitigation: ship_rec.shield_mitigation.unwrap_or(0.8),
                apex_barrier: 0.0,
                apex_shred: ship_rec.apex_shred,
                isolytic_damage: ship_rec.isolytic_damage,
                isolytic_defense: 0.0,
                weapons: if shared.is_pvp() {
                    shared
                        .defender_ship_rec
                        .as_ref()
                        .map(|def_ship| {
                            ship_weapons_with_pierce_vs_defender_ship(
                                ship_rec,
                                def_ship,
                                &shared.profile,
                                &merged_static,
                            )
                        })
                        .unwrap_or_else(|| ship_rec.to_weapons())
                } else {
                    shared
                        .hostile_rec
                        .as_ref()
                        .map(|h| {
                            ship_weapons_with_resolved_pierce_through(
                                ship_rec,
                                h,
                                &shared.profile,
                                &merged_static,
                            )
                        })
                        .unwrap_or_else(|| ship_rec.to_weapons())
                },
                hostile_mitigation_params: None,
            },
            &shared.profile,
            ship_rec.faction.as_deref(),
            crate::data::profile::compute_officer_stat_runtime_bonus(
                officer_stat_totals,
                bridge_officer_stat_totals,
                ship_rec,
                &shared.profile,
                ship_rec.faction.as_deref(),
                &merged_static,
                &pending_officer_stat_contributions,
                &cond_ctx,
                if shared.is_pvp() {
                    &shared.defender_pending_officer_stat_contributions
                } else {
                    &[]
                },
            ),
        );
        if shared.ship == USS_VOYAGER_SHIP_ID {
            if let Some(h) = shared.borg_alcove_hull_hp_bonus {
                if h.is_finite() && h != 0.0 {
                    attacker.hull_health *= 1.0 + h;
                }
            }
        }
        if let Some(h) = shared.class_gated_torpedo_family_hull_hp_bonus {
            if h.is_finite() && h != 0.0 {
                attacker.hull_health *= 1.0 + h;
            }
        }
        apply_dual_gate_hull_shield_research(&mut attacker, shared);
        apply_profile_player_apex_barrier_tal_gate(
            &mut attacker,
            &shared.profile,
            shared.defender_opponent,
            attacker_tal_assigned_captain_or_bridge,
        );
        if shared.defender_opponent == DefenderOpponent::Hostile {
            if let Some(d) = shared.class_gated_torpedo_family_hostile_shield_mitigation_sum {
                if d.is_finite() && d != 0.0 {
                    attacker.shield_mitigation = (attacker.shield_mitigation + d).clamp(0.0, 1.0);
                }
            }
        }
        if !merged_static.is_empty() {
            attacker = apply_static_buffs_to_combatant(attacker, &merged_static);
        }
        let mut seats = crew_seats.clone();
        extend_crew_with_ship_abilities(&mut seats, Some(ship_rec));
        extend_crew_with_research_derived_attack_phase_seats(
            &mut seats,
            &shared.research_derived_seats,
        );
        extend_crew_with_research_derived_attack_phase_seats(
            &mut seats,
            &shared.forbidden_tech_derived_seats,
        );
        extend_crew_with_isolytic_cascade_profile_and_static(
            &mut seats,
            &shared.profile,
            static_cascade_bonus,
        );
        extend_crew_with_player_crit_damage_reduction_profile_bonus(
            &mut seats,
            &shared.profile,
            shared.defender_opponent,
        );
        let weapon_damage_profile_additive_pool =
            weapon_damage_profile_additive_pool_from_env(&shared.profile);
        let profile_weapon_damage_fraction =
            profile_weapon_damage_fraction_for_combat(&shared.profile);
        return CombatSimulationInput {
            attacker,
            defender,
            defender_crew,
            crew: CrewConfiguration { seats },
            rounds,
            defender_hull,
            base_seed,
            weapon_damage_profile_additive_pool,
            profile_weapon_damage_fraction,
            engagement_enemy_types: shared.engagement_enemy_types.clone(),
            defender_level: shared.defender_level,
            attacker_roster_officer_ids,
            incoming_shield_mitigation_bonus: if shared.is_pvp() {
                shared.defender_incoming_shield_mitigation_bonus
            } else {
                shared.incoming_shield_mitigation_bonus
            },
            incoming_shield_mitigation_bonus_rounds: if shared.is_pvp() {
                shared.defender_incoming_shield_mitigation_bonus_rounds
            } else {
                shared.incoming_shield_mitigation_bonus_rounds
            },
            attacker_owner_faction: shared.attacker_owner_faction,
        };
    }

    let ship_hash = hash_identifier(&shared.ship);
    let hostile_hash = hash_identifier(&shared.hostile);
    let defender_hull = 260.0 + ((hostile_hash >> 16) % 280) as f64;
    let defender_mitigation = computed_defender_mitigation(&shared.ship, &shared.hostile);

    let mut attacker = apply_profile_to_attacker(
        Combatant {
            id: shared.ship.clone(),
            attack: 95.0 + (ship_hash % 70) as f64,
            mitigation: 0.0,
            pierce: 0.08 + ((ship_hash >> 8) % 14) as f64 / 100.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance,
            proc_multiplier,
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
        },
        &shared.profile,
        None,
        // Placeholder combatant path: no real ship_rec, so no officer-stat runtime.
        crate::data::profile::OfficerStatRuntimeBonus::default(),
    );
    if shared.ship == USS_VOYAGER_SHIP_ID {
        if let Some(h) = shared.borg_alcove_hull_hp_bonus {
            if h.is_finite() && h != 0.0 {
                attacker.hull_health *= 1.0 + h;
            }
        }
    }
    if let Some(h) = shared.class_gated_torpedo_family_hull_hp_bonus {
        if h.is_finite() && h != 0.0 {
            attacker.hull_health *= 1.0 + h;
        }
    }
    apply_dual_gate_hull_shield_research(&mut attacker, shared);
    apply_profile_player_apex_barrier_tal_gate(
        &mut attacker,
        &shared.profile,
        shared.defender_opponent,
        attacker_tal_assigned_captain_or_bridge,
    );
    if shared.defender_opponent == DefenderOpponent::Hostile {
        if let Some(d) = shared.class_gated_torpedo_family_hostile_shield_mitigation_sum {
            if d.is_finite() && d != 0.0 {
                attacker.shield_mitigation = (attacker.shield_mitigation + d).clamp(0.0, 1.0);
            }
        }
    }
    if !merged_static.is_empty() {
        attacker = apply_static_buffs_to_combatant(attacker, &merged_static);
    }

    let mut seats = crew_seats.clone();
    extend_crew_with_ship_abilities(&mut seats, shared.ship_rec.as_ref());
    extend_crew_with_research_derived_attack_phase_seats(
        &mut seats,
        &shared.research_derived_seats,
    );
    extend_crew_with_research_derived_attack_phase_seats(
        &mut seats,
        &shared.forbidden_tech_derived_seats,
    );
    extend_crew_with_isolytic_cascade_profile_and_static(
        &mut seats,
        &shared.profile,
        static_cascade_bonus,
    );
    extend_crew_with_player_crit_damage_reduction_profile_bonus(
        &mut seats,
        &shared.profile,
        shared.defender_opponent,
    );

    let weapon_damage_profile_additive_pool =
        weapon_damage_profile_additive_pool_from_env(&shared.profile);
    let profile_weapon_damage_fraction = profile_weapon_damage_fraction_for_combat(&shared.profile);
    let mut defender = Combatant {
        id: shared.hostile.clone(),
        attack: 0.0,
        mitigation: defender_mitigation,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        weapons: vec![],
        end_of_round_damage: 0.0,
        hull_health: defender_hull,
        shield_health: 400.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        hostile_mitigation_params: None,
    };
    apply_support_defender_static_if_pvp(shared, &mut defender);
    if !shared.player_defender_static_buffs.is_empty() {
        let mut dstatic = shared.player_defender_static_buffs.clone();
        let _ = take_isolytic_cascade_static_bonus(&mut dstatic);
        defender = apply_static_buffs_to_combatant(defender, &dstatic);
    }
    CombatSimulationInput {
        attacker,
        defender,
        defender_crew,
        crew: CrewConfiguration { seats },
        rounds: 3 + (hostile_hash % 4) as u32,
        defender_hull,
        base_seed,
        weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction,
        engagement_enemy_types: shared.engagement_enemy_types.clone(),
        defender_level: shared.defender_level,
        attacker_roster_officer_ids,
        incoming_shield_mitigation_bonus: shared.incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds: shared.incoming_shield_mitigation_bonus_rounds,
        attacker_owner_faction: shared.attacker_owner_faction,
    }
}

/// When `KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL` is `1` or `true`, scenario build exposes profile
/// `weapon_damage` bonus to the combat engine as [`SimulationConfig::weapon_damage_profile_additive_pool`]
/// so outgoing damage can use a single additive pool vs layered `(1+p)×(1+sum)` (see findings doc).
fn weapon_damage_profile_additive_pool_from_env(profile: &PlayerProfile) -> Option<f64> {
    let on = std::env::var("KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !on {
        return None;
    }
    let p = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    Some(if p.is_finite() { p.max(0.0) } else { 0.0 })
}

fn profile_weapon_damage_fraction_for_combat(profile: &PlayerProfile) -> f64 {
    let p = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
    if p.is_finite() {
        p.max(0.0)
    } else {
        0.0
    }
}

/// Merge `Name (TN)` tier suffixes from the candidate into [`ResolveOptions::officer_tiers`].
///
/// Imported roster tiers remain as defaults; explicit `(T#)` on captain / bridge / below decks
/// overrides per canonical officer id (same ids LCARS uses in [`resolve_crew_to_buff_set`]).
fn resolve_options_with_candidate_tiers(
    base: &ResolveOptions,
    candidate: &CrewCandidate,
    lcars: Option<&LcarsOfficerData>,
) -> ResolveOptions {
    let Some(lcars) = lcars else {
        return base.clone();
    };

    let mut officer_tiers = base.officer_tiers.clone().unwrap_or_default();

    let mut apply_slot = |slot: &str| {
        let (name, tier_opt) = split_name_and_tier(slot);
        let Some(tier) = tier_opt else {
            return;
        };
        let Some(id) = lcars.name_to_id.get(&normalize_lookup_key(&name)).cloned() else {
            return;
        };
        officer_tiers.insert(id, tier);
    };

    apply_slot(&candidate.captain);
    for s in &candidate.bridge {
        apply_slot(s);
    }
    for s in &candidate.below_decks {
        apply_slot(s);
    }

    ResolveOptions {
        tier: base.tier,
        officer_tiers: if officer_tiers.is_empty() {
            None
        } else {
            Some(officer_tiers)
        },
        officer_levels: None,
    }
}

/// Build (crew_seats, static_buffs, proc_chance, proc_multiplier, officer_stat_totals) from
/// candidate and officer data. The fifth element is the per-side summed Attack/Defense/Health
/// across crewed officers (see `docs/OFFICER_STAT_FORMULA.md` §1); defaults to zero when LCARS
/// data is unavailable, which silently disables the §2 runtime contribution.
#[allow(clippy::type_complexity)]
fn build_crew_and_buffs(
    candidate: &CrewCandidate,
    officers_by_name: &HashMap<String, Officer>,
    lcars_data: Option<&LcarsOfficerData>,
    resolve_options: &ResolveOptions,
) -> (
    Vec<CrewSeatContext>,
    HashMap<String, f64>,
    f64,
    f64,
    crate::combat::CrewOfficerStatTotals,
    crate::combat::CrewOfficerStatTotals,
    Vec<crate::lcars::PendingOfficerStatContribution>,
) {
    if let Some(lcars) = lcars_data {
        let captain_id = lcars
            .name_to_id
            .get(&normalize_lookup_key(
                &split_name_and_tier(&candidate.captain).0,
            ))
            .cloned();
        let bridge_ids: Vec<String> = candidate
            .bridge
            .iter()
            .filter_map(|n| {
                lcars
                    .name_to_id
                    .get(&normalize_lookup_key(&split_name_and_tier(n).0))
                    .cloned()
            })
            .collect();
        let below_ids: Vec<String> = candidate
            .below_decks
            .iter()
            .filter_map(|n| {
                lcars
                    .name_to_id
                    .get(&normalize_lookup_key(&split_name_and_tier(n).0))
                    .cloned()
            })
            .collect();

        if let Some(cap_id) = captain_id {
            let buff_set = resolve_crew_to_buff_set(
                &cap_id,
                &bridge_ids,
                &below_ids,
                &lcars.by_id,
                resolve_options,
            );
            (
                buff_set.to_crew_config().seats.clone(),
                buff_set.static_buffs,
                buff_set.proc_chance,
                buff_set.proc_multiplier,
                buff_set.officer_stat_totals,
                buff_set.bridge_officer_stat_totals,
                buff_set.pending_officer_stat_contributions,
            )
        } else {
            (
                build_crew_seats(candidate, officers_by_name),
                HashMap::new(),
                0.0,
                1.0,
                crate::combat::CrewOfficerStatTotals::default(),
                crate::combat::CrewOfficerStatTotals::default(),
                Vec::new(),
            )
        }
    } else {
        (
            build_crew_seats(candidate, officers_by_name),
            HashMap::new(),
            0.0,
            1.0,
            crate::combat::CrewOfficerStatTotals::default(),
            crate::combat::CrewOfficerStatTotals::default(),
            Vec::new(),
        )
    }
}

/// Build the fight-setup condition context that
/// [`crate::data::profile::compute_officer_stat_runtime_bonus`] uses to evaluate static
/// officer-stat ability gates (Phase 4b of docs/OFFICER_STAT_FORMULA.md).
fn build_officer_stat_condition_context(
    shared: &SharedScenarioData,
    ship_rec: &ShipRecord,
) -> crate::data::profile::OfficerStatConditionContext {
    let defender_ship_type = shared
        .defender_ship_rec
        .as_ref()
        .map(|s| s.ship_class.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            shared
                .hostile_rec
                .as_ref()
                .map(|h| h.ship_class.trim().to_string())
                .filter(|s| !s.is_empty())
        });
    // HostileRecord carries no faction slug — only the numeric id on the embedded
    // HostileFactionRef. defender_faction_slug stays None (no condition in production data
    // exercises it today); defender_hull_faction_id_is reads the numeric id.
    let defender_faction_slug: Option<String> = None;
    let defender_faction_id = shared
        .hostile_rec
        .as_ref()
        .and_then(|h| h.faction.as_ref())
        .map(|f| f.id);
    let engagement_types: Vec<String> = shared
        .engagement_enemy_types
        .0
        .iter()
        .map(|t| format!("{:?}", t).to_lowercase())
        .collect();
    crate::data::profile::OfficerStatConditionContext {
        attacker_ship_class: Some(ship_rec.ship_class.trim().to_string()).filter(|s| !s.is_empty()),
        attacker_ship_id: Some(ship_rec.id.trim().to_string()).filter(|s| !s.is_empty()),
        attacker_owner_faction: ship_rec.faction.clone(),
        defender_is_player_ship: shared.defender_opponent.defender_is_player_ship(),
        defender_ship_type,
        defender_faction_id,
        defender_faction_slug,
        engagement_types,
    }
}

/// Optional LCARS-resolved **defender** officer crew (captain + bridge + below decks).
/// When present with a non-empty captain, seats are merged after hostile ship abilities and
/// static buff keys are folded into the defender [`Combatant`] (excluding isolytic cascade static;
/// see [`take_isolytic_cascade_static_bonus`] at combat input build time).
#[derive(Debug, Clone, Default)]
pub struct PlayerDefenderOfficerCrewOverride {
    pub captain: Option<String>,
    pub bridge: Option<Vec<Option<String>>>,
    pub below_deck: Option<Vec<Option<String>>>,
    pub below_decks_slots: usize,
}

fn pad_strings_first_repeat(mut v: Vec<String>, len: usize) -> Vec<String> {
    let first = v.first().cloned().unwrap_or_default();
    while v.len() < len {
        v.push(first.clone());
    }
    v.truncate(len);
    v
}

fn officer_display_for_index(officers_by_name: &HashMap<String, Officer>, id: &str) -> String {
    let key = normalize_lookup_key(id.trim());
    officers_by_name
        .get(&key)
        .map(|o| o.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn resolve_player_defender_officer_bundle(
    o: Option<&PlayerDefenderOfficerCrewOverride>,
    officers_by_name: &HashMap<String, Officer>,
    lcars_data: Option<&LcarsOfficerData>,
    resolve_options: &ResolveOptions,
) -> (
    Vec<CrewSeatContext>,
    HashMap<String, f64>,
    crate::combat::CrewOfficerStatTotals,
    crate::combat::CrewOfficerStatTotals,
    Vec<crate::lcars::PendingOfficerStatContribution>,
) {
    let Some(o) = o else {
        return (
            Vec::new(),
            HashMap::new(),
            crate::combat::CrewOfficerStatTotals::default(),
            crate::combat::CrewOfficerStatTotals::default(),
            Vec::new(),
        );
    };
    let Some(cap) = o
        .captain
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    else {
        return (
            Vec::new(),
            HashMap::new(),
            crate::combat::CrewOfficerStatTotals::default(),
            crate::combat::CrewOfficerStatTotals::default(),
            Vec::new(),
        );
    };
    let bridge_src: Vec<String> = o
        .bridge
        .as_ref()
        .map(|v| {
            v.iter()
                .take(BRIDGE_SLOTS)
                .map(|slot| {
                    slot.as_ref()
                        .map(|id| officer_display_for_index(officers_by_name, id))
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let bridge = pad_strings_first_repeat(bridge_src, BRIDGE_SLOTS);
    let bd_cap = o
        .below_decks_slots
        .clamp(MIN_BELOW_DECKS_SLOTS, MAX_BELOW_DECKS_SLOTS);
    let below_src: Vec<String> = o
        .below_deck
        .as_ref()
        .map(|v| {
            v.iter()
                .take(bd_cap)
                .map(|slot| {
                    slot.as_ref()
                        .map(|id| officer_display_for_index(officers_by_name, id))
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let below_decks = pad_strings_first_repeat(below_src, bd_cap);
    let candidate = CrewCandidate {
        captain: cap.to_string(),
        bridge,
        below_decks,
    };
    let (
        seats,
        static_buffs,
        _proc_c,
        _proc_m,
        officer_stat_totals,
        bridge_officer_stat_totals,
        pending_contribs,
    ) = build_crew_and_buffs(&candidate, officers_by_name, lcars_data, resolve_options);
    (
        seats,
        static_buffs,
        officer_stat_totals,
        bridge_officer_stat_totals,
        pending_contribs,
    )
}

#[allow(dead_code)] // used by unit tests (computed_mitigation_is_deterministic_for_same_inputs)
pub(crate) fn scenario_to_combat_input(
    ship: &str,
    hostile: &str,
    candidate: &CrewCandidate,
    seed: u64,
    officers_by_name: &HashMap<String, Officer>,
    profile: &PlayerProfile,
    lcars_data: Option<&LcarsOfficerData>,
) -> CombatSimulationInput {
    let base_seed = stable_seed(
        ship,
        hostile,
        &candidate.captain,
        &candidate.bridge,
        &candidate.below_decks,
        seed,
    );
    let attacker_roster_officer_ids =
        roster_officer_ids_from_candidate(candidate, officers_by_name);

    let resolve_opts = ResolveOptions::default();
    let (
        crew_seats,
        mut static_buffs,
        proc_chance,
        proc_multiplier,
        officer_stat_totals,
        bridge_officer_stat_totals,
        pending_officer_stat_contributions,
    ) = build_crew_and_buffs(candidate, officers_by_name, lcars_data, &resolve_opts);
    let static_cascade_bonus = take_isolytic_cascade_static_bonus(&mut static_buffs);

    if let (Some(ship_rec), Some(hostile_rec)) = (resolve_ship(ship), resolve_hostile(hostile)) {
        let hostile_ability_catalog =
            load_hostile_ability_catalog(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
        let defender_crew = hostile_abilities_to_defender_crew(
            &hostile_rec.ability,
            hostile_ability_catalog.as_ref(),
        );
        let (defender_mitigation, pierce) = mitigation_and_pierce_for_player_vs_hostile(
            &ship_rec,
            &hostile_rec,
            profile,
            &static_buffs,
        );
        let defender_hull = hostile_rec.hull_health;
        let rounds = 100u32.min(10u32.saturating_add(hostile_rec.level));
        let mut attacker = apply_profile_to_attacker(
            Combatant {
                id: ship.to_string(),
                attack: ship_rec.attack,
                mitigation: 0.0,
                pierce,
                crit_chance: ship_rec.crit_chance,
                crit_multiplier: ship_rec.crit_damage,
                proc_chance,
                proc_multiplier,
                end_of_round_damage: 0.0,
                hull_health: ship_rec.hull_health,
                shield_health: ship_rec.shield_health,
                shield_mitigation: ship_rec.shield_mitigation.unwrap_or(0.8),
                apex_barrier: 0.0,
                apex_shred: ship_rec.apex_shred,
                isolytic_damage: ship_rec.isolytic_damage,
                isolytic_defense: 0.0,
                weapons: ship_weapons_with_resolved_pierce_through(
                    &ship_rec,
                    &hostile_rec,
                    profile,
                    &static_buffs,
                ),
                hostile_mitigation_params: None,
            },
            profile,
            ship_rec.faction.as_deref(),
            {
                // scenario_to_combat_input variant: defender info is the hostile_rec (PvE only).
                let defender_ship_type =
                    Some(hostile_rec.ship_class.trim().to_string()).filter(|s| !s.is_empty());
                let cond_ctx = crate::data::profile::OfficerStatConditionContext {
                    attacker_ship_class: Some(ship_rec.ship_class.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    attacker_ship_id: Some(ship_rec.id.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    attacker_owner_faction: ship_rec.faction.clone(),
                    defender_is_player_ship: false,
                    defender_ship_type,
                    defender_faction_id: hostile_rec.faction.as_ref().map(|f| f.id),
                    defender_faction_slug: None,
                    engagement_types: Vec::new(),
                };
                crate::data::profile::compute_officer_stat_runtime_bonus(
                    officer_stat_totals,
                    bridge_officer_stat_totals,
                    &ship_rec,
                    profile,
                    ship_rec.faction.as_deref(),
                    &static_buffs,
                    &pending_officer_stat_contributions,
                    &cond_ctx,
                    &[],
                )
            },
        );
        if !static_buffs.is_empty() {
            attacker = apply_static_buffs_to_combatant(attacker, &static_buffs);
        }
        let mut seats = crew_seats.clone();
        extend_crew_with_ship_abilities(&mut seats, Some(&ship_rec));
        extend_crew_with_isolytic_cascade_profile_and_static(
            &mut seats,
            profile,
            static_cascade_bonus,
        );
        let weapon_damage_profile_additive_pool =
            weapon_damage_profile_additive_pool_from_env(profile);
        let profile_weapon_damage_fraction = profile_weapon_damage_fraction_for_combat(profile);
        let engagement_enemy_types = hostile_rec.engagement_enemy_types_for_combat();
        let hostile_mitigation_params = HostileMitigationParams {
            defender_stats: hostile_rec.to_defender_stats(),
            base_attacker_stats: effective_attacker_stats_for_mitigation(
                &ship_rec,
                profile,
                &static_buffs,
                hostile_rec.ship_type_for_combat(),
            ),
            ship_type: hostile_rec.ship_type_for_combat(),
            mystery_mitigation_factor: hostile_rec.mystery_mitigation_factor.unwrap_or(0.0),
            floor: hostile_rec.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
            ceiling: hostile_rec.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
        };
        return CombatSimulationInput {
            attacker,
            defender: defender_combatant_from_hostile_record(
                hostile,
                &hostile_rec,
                defender_mitigation,
                ship_rec.ship_type(),
                ship_rec.to_defender_stats(),
                Some(hostile_mitigation_params),
            ),
            defender_crew,
            crew: CrewConfiguration { seats },
            rounds,
            defender_hull,
            base_seed,
            weapon_damage_profile_additive_pool,
            profile_weapon_damage_fraction,
            engagement_enemy_types,
            defender_level: Some(hostile_rec.level),
            attacker_roster_officer_ids,
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            attacker_owner_faction: attacker_owner_faction_from_ship(Some(&ship_rec)),
        };
    }

    let ship_hash = hash_identifier(ship);
    let hostile_hash = hash_identifier(hostile);
    let defender_hull = 260.0 + ((hostile_hash >> 16) % 280) as f64;
    let defender_mitigation = computed_defender_mitigation(ship, hostile);

    let mut attacker = apply_profile_to_attacker(
        Combatant {
            id: ship.to_string(),
            attack: 95.0 + (ship_hash % 70) as f64,
            mitigation: 0.0,
            pierce: 0.08 + ((ship_hash >> 8) % 14) as f64 / 100.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance,
            proc_multiplier,
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
        },
        profile,
        None,
        // Placeholder combatant path: no real ship_rec, so no officer-stat runtime.
        crate::data::profile::OfficerStatRuntimeBonus::default(),
    );
    if !static_buffs.is_empty() {
        attacker = apply_static_buffs_to_combatant(attacker, &static_buffs);
    }

    let mut seats = crew_seats.clone();
    extend_crew_with_ship_abilities(&mut seats, resolve_ship(ship).as_ref());
    extend_crew_with_isolytic_cascade_profile_and_static(&mut seats, profile, static_cascade_bonus);

    let defender_crew = CrewConfiguration { seats: Vec::new() };
    let weapon_damage_profile_additive_pool = weapon_damage_profile_additive_pool_from_env(profile);
    let profile_weapon_damage_fraction = profile_weapon_damage_fraction_for_combat(profile);
    CombatSimulationInput {
        attacker,
        defender: Combatant {
            id: hostile.to_string(),
            attack: 0.0,
            mitigation: defender_mitigation,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            weapons: vec![],
            end_of_round_damage: 0.0,
            hull_health: defender_hull,
            shield_health: 400.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        },
        defender_crew,
        crew: CrewConfiguration { seats },
        rounds: 3 + (hostile_hash % 4) as u32,
        defender_hull,
        base_seed,
        weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction,
        engagement_enemy_types: EnemyTypes::default(),
        defender_level: None,
        attacker_roster_officer_ids,
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_owner_faction: attacker_owner_faction_from_ship(resolve_ship(ship).as_ref()),
    }
}

fn synthetic_ship_type(identifier_hash: u64) -> ShipType {
    match identifier_hash % 4 {
        0 => ShipType::Battleship,
        1 => ShipType::Explorer,
        2 => ShipType::Interceptor,
        _ => ShipType::Survey,
    }
}

fn synthetic_defender_stats(hostile_hash: u64) -> DefenderStats {
    DefenderStats {
        armor: 120.0 + (hostile_hash % 260) as f64,
        shield_deflection: 110.0 + ((hostile_hash >> 11) % 240) as f64,
        dodge: 90.0 + ((hostile_hash >> 23) % 220) as f64,
    }
}

fn synthetic_attacker_stats(ship_hash: u64) -> AttackerStats {
    AttackerStats {
        armor_piercing: 85.0 + (ship_hash % 220) as f64,
        shield_piercing: 80.0 + ((ship_hash >> 9) % 210) as f64,
        accuracy: 75.0 + ((ship_hash >> 21) % 200) as f64,
    }
}

pub(crate) fn computed_defender_mitigation(ship: &str, hostile: &str) -> f64 {
    if let (Some(ship_rec), Some(hostile_rec)) = (resolve_ship(ship), resolve_hostile(hostile)) {
        return mitigation_for_hostile(
            hostile_rec.to_defender_stats(),
            ship_rec.to_attacker_stats(),
            hostile_rec.ship_type_for_combat(),
            hostile_rec.mystery_mitigation_factor.unwrap_or(0.0),
            hostile_rec.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
            hostile_rec.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
        );
    }
    let attacker = synthetic_attacker_stats(hash_identifier(ship));
    let defender_hash = hash_identifier(hostile);
    let defender = synthetic_defender_stats(defender_hash);
    let ship_type = synthetic_ship_type(defender_hash);
    mitigation(defender, attacker, ship_type)
}

pub(crate) fn stable_seed(
    ship: &str,
    hostile: &str,
    captain: &str,
    bridge: &[String],
    below_decks: &[String],
    seed: u64,
) -> u64 {
    let mut acc = seed;
    for s in [ship, hostile, captain]
        .into_iter()
        .chain(bridge.iter().map(String::as_str))
        .chain(below_decks.iter().map(String::as_str))
    {
        for b in s.bytes() {
            acc = acc.wrapping_mul(37).wrapping_add(u64::from(b));
        }
    }
    acc
}

/// Build scenario data for `(ship, hostile)` without a [DataRegistry] — same sources as legacy
/// [super::simulation::run_monte_carlo_parallel] (canonical officers, profile JSON, optional LCARS).
pub(crate) fn build_shared_scenario_data_standalone(
    ship: &str,
    hostile: &str,
    support_buffs_request: Option<&[String]>,
    defender_opponent: DefenderOpponent,
    player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
) -> SharedScenarioData {
    let officer_index = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .ok()
        .map(index_officers_by_name)
        .unwrap_or_default();
    let pid = profile_index::resolve_profile_id_for_api(None);
    let profile_path_str = profile_path(&pid, PROFILE_JSON)
        .to_string_lossy()
        .to_string();
    let ft_path = profile_path(&pid, FORBIDDEN_TECH_IMPORTED)
        .to_string_lossy()
        .to_string();
    let mut profile = load_profile(&profile_path_str);
    let ft_entries = import::load_imported_forbidden_tech(&ft_path).unwrap_or_default();
    let mut forbidden_tech_derived_seats: Vec<CrewSeatContext> = Vec::new();
    let mut borg_alcove_hull_hp_bonus: Option<f64> = None;
    let forbidden_catalog =
        forbidden_chaos::load_forbidden_chaos(forbidden_chaos::DEFAULT_FORBIDDEN_CHAOS_PATH);
    let scale_by_level_tier = forbidden_tech_level_tier_scaling_enabled_from_env();
    let effective_fids: Vec<i64> = forbidden_catalog
        .as_ref()
        .map(|c| resolve_effective_tech_fids(&profile, &ft_entries, c))
        .unwrap_or_default();
    if let Some(ref catalog) = forbidden_catalog {
        if !effective_fids.is_empty() {
            merge_tech_fids_into_profile_with_level_tier(
                &mut profile,
                &effective_fids,
                &ft_entries,
                catalog,
                scale_by_level_tier,
            );
            forbidden_tech_derived_seats = forbidden_tech_derived_attack_phase_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            );
            forbidden_tech_derived_seats.extend(
                quantum_slipstream_forbidden_tech_round_start_seats(
                    &ft_entries,
                    &effective_fids,
                    catalog,
                    scale_by_level_tier,
                ),
            );
            forbidden_tech_derived_seats.extend(ship_class_gated_torpedo_family_derived_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            ));
            forbidden_tech_derived_seats.extend(borg_operating_table_forbidden_tech_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            ));
            borg_alcove_hull_hp_bonus = borg_alcove_hull_hp_bonus_fraction(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            );
        }
    }

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let support_cat =
        SupportBuffCatalog::load(manifest.join(support_buffs::DEFAULT_SUPPORT_BUFFS_PATH)).ok();
    let (resolved_support_buffs, unknown_support_buff_ids) =
        match (support_cat.as_ref(), support_buffs_request) {
            (Some(cat), Some(req)) if !req.is_empty() => {
                support_buffs::resolve_selected_support_buff_ids(cat, req)
            }
            _ => (Vec::new(), Vec::new()),
        };
    let support_research_gates =
        SupportBuffResearchGateState::from_resolved_support_buff_ids(&resolved_support_buffs);

    let shared_research_catalog = load_research_catalog(DEFAULT_RESEARCH_CATALOG_PATH);
    let canonical_overrides = load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH);
    let research_path = profile_path(&pid, RESEARCH_IMPORTED)
        .to_string_lossy()
        .into_owned();
    let imported_research = import::load_imported_research(&research_path).unwrap_or_default();
    let exclude_canonical_rids: HashSet<i64> = canonical_overrides.keys().copied().collect();
    let (incoming_shield_mitigation_bonus, incoming_shield_mitigation_bonus_rounds) =
        incoming_shield_mitigation_for_combat(&imported_research, &canonical_overrides);

    let research_derived_seats = if let Some(ref cat) = shared_research_catalog {
        merge_research_bonuses_into_profile(
            &mut profile,
            &imported_research,
            cat,
            Some(&exclude_canonical_rids),
        );
        research_derived_attack_phase_seats(
            &imported_research,
            cat,
            &support_research_gates,
            &canonical_overrides,
        )
    } else {
        Vec::new()
    };

    let research_for_buffs = if !resolved_support_buffs.is_empty() {
        shared_research_catalog.as_ref()
    } else {
        None
    };
    let (mut support_static_buffs, support_defender_static_buffs) = match support_cat.as_ref() {
        Some(c) => {
            support_buffs::aggregate_support_static_bonuses_split(c, &resolved_support_buffs)
        }
        None => (HashMap::new(), HashMap::new()),
    };
    if let Some(ref cat) = shared_research_catalog {
        support_buffs::augment_static_buffs_with_support_gated_research(
            &mut support_static_buffs,
            &imported_research,
            cat,
            &support_research_gates,
        );
    }
    if let Some(ref cat) = support_cat {
        if !resolved_support_buffs.is_empty() {
            support_buffs::apply_support_buff_research_to_profile(
                &mut profile,
                cat,
                &resolved_support_buffs,
                research_for_buffs,
            );
        }
    }
    let applied_support_buffs = support_cat
        .as_ref()
        .map(|cat| support_buffs::describe_resolved_support_buffs(cat, &resolved_support_buffs))
        .unwrap_or_default();

    let lcars_data = if use_lcars_officer_source_standalone() {
        load_lcars_dir(DEFAULT_LCARS_OFFICERS_DIR_STANDALONE)
            .ok()
            .map(lcars_officer_data_from_officers)
    } else {
        None
    };

    let roster_path = profile_path(&pid, ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    let resolve_options = import::load_imported_roster(&roster_path)
        .map(|entries| {
            let officer_tiers: HashMap<String, u8> = entries
                .into_iter()
                .filter_map(|e| e.tier.map(|t| (e.canonical_officer_id, t)))
                .collect();
            ResolveOptions {
                tier: None,
                officer_tiers: if officer_tiers.is_empty() {
                    None
                } else {
                    Some(officer_tiers)
                },
                officer_levels: None,
            }
        })
        .unwrap_or_default();

    let ship_rec = resolve_ship(ship);
    let hostile_rec = resolve_hostile(hostile);

    let class_gated_tp_hull = forbidden_catalog.as_ref().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });
    let class_gated_tp_shield_mit = forbidden_catalog.as_ref().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });
    let class_gated_tp_accuracy = forbidden_catalog.as_ref().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hostile_accuracy_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });

    apply_class_gated_torpedo_family_hostile_accuracy_to_profile(
        &mut profile,
        defender_opponent,
        class_gated_tp_accuracy,
    );

    let (
        cached_defender,
        cached_rounds,
        cached_defender_hull,
        cached_pierce,
        cached_defender_mitigation,
    ) = if let (Some(ref ship_r), Some(ref hostile_r)) = (&ship_rec, &hostile_rec) {
        let mut attacker_stats = ship_r.to_attacker_stats();
        apply_profile_accuracy_to_attacker_stats(&mut attacker_stats, &profile);
        let defender_mitigation = mitigation_for_hostile(
            hostile_r.to_defender_stats(),
            attacker_stats,
            hostile_r.ship_type_for_combat(),
            hostile_r.mystery_mitigation_factor.unwrap_or(0.0),
            hostile_r.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
            hostile_r.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
        );
        let pierce = pierce_damage_through_bonus(
            hostile_r.to_defender_stats(),
            attacker_stats,
            hostile_r.ship_type_for_combat(),
        );
        let defender = defender_combatant_from_hostile_record(
            hostile,
            hostile_r,
            defender_mitigation,
            ship_r.ship_type(),
            ship_r.to_defender_stats(),
            None,
        );
        let rounds = 100u32.min(10u32.saturating_add(hostile_r.level));
        (
            Some(defender),
            Some(rounds),
            Some(hostile_r.hull_health),
            Some(pierce),
            Some(defender_mitigation),
        )
    } else {
        (None, None, None, None, None)
    };

    let using_placeholder_combatants = cached_defender.is_none();

    let engagement_enemy_types = hostile_rec
        .as_ref()
        .map(|h| h.engagement_enemy_types_for_combat())
        .unwrap_or_default();
    let defender_level = hostile_rec.as_ref().map(|h| h.level);

    let attacker_owner_faction = attacker_owner_faction_from_ship(ship_rec.as_ref());
    let defender_faction_tag =
        defender_faction_tag_for_scenario(hostile_rec.as_ref(), None);
    let (dual_gate_research_hull_hp, dual_gate_research_shield_hp) =
        dual_gate_hull_shield_for_scenario(
            shared_research_catalog.as_ref(),
            &imported_research,
            &exclude_canonical_rids,
            ship_rec.as_ref(),
            defender_faction_tag,
        );

    let (
        player_defender_officer_seats,
        player_defender_static_buffs,
        defender_officer_stat_totals,
        defender_bridge_officer_stat_totals,
        defender_pending_officer_stat_contributions,
    ) = resolve_player_defender_officer_bundle(
        player_defender_officer_crew.as_ref(),
        &officer_index,
        lcars_data.as_ref(),
        &resolve_options,
    );

    SharedScenarioData {
        ship: ship.to_string(),
        hostile: hostile.to_string(),
        officer_index,
        profile,
        lcars_data,
        resolve_options,
        ship_rec,
        hostile_rec,
        cached_defender,
        cached_rounds,
        cached_defender_hull,
        cached_pierce,
        cached_defender_mitigation,
        using_placeholder_combatants,
        resolved_support_buffs,
        applied_support_buffs,
        support_static_buffs,
        support_defender_static_buffs,
        unknown_support_buff_ids,
        research_derived_seats,
        forbidden_tech_derived_seats,
        borg_alcove_hull_hp_bonus,
        class_gated_torpedo_family_hull_hp_bonus: class_gated_tp_hull,
        class_gated_torpedo_family_hostile_shield_mitigation_sum: class_gated_tp_shield_mit,
        defender_opponent,
        attacker_owner_faction,
        dual_gate_research_hull_hp,
        dual_gate_research_shield_hp,
        engagement_enemy_types,
        defender_level,
        incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds,
        player_defender_officer_seats,
        player_defender_static_buffs,
        defender_officer_stat_totals,
        defender_bridge_officer_stat_totals,
        defender_pending_officer_stat_contributions,
        pvp: None,
        defender_ship_rec: None,
        defender_profile: None,
        defender_incoming_shield_mitigation_bonus: 0.0,
        defender_incoming_shield_mitigation_bonus_rounds: 0,
    }
}

/// Merge buildings, research, and forbidden tech into a profile for PvP defender-side stats.
fn load_defender_profile_for_pvp(
    registry: &crate::data::data_registry::DataRegistry,
    defender_profile_id: &str,
    defender_ship_id: &str,
) -> (PlayerProfile, f64, u32) {
    let pid = profile_index::resolve_profile_id_for_api(Some(defender_profile_id));
    let profile_path_str = profile_path(&pid, PROFILE_JSON)
        .to_string_lossy()
        .to_string();
    let ft_path = profile_path(&pid, FORBIDDEN_TECH_IMPORTED)
        .to_string_lossy()
        .to_string();
    let mut profile = load_profile(&profile_path_str);
    let ft_entries = import::load_imported_forbidden_tech(&ft_path).unwrap_or_default();
    let scale_by_level_tier = forbidden_tech_level_tier_scaling_enabled_from_env();
    let effective_fids: Vec<i64> = registry
        .forbidden_chaos_catalog()
        .map(|c| resolve_effective_tech_fids(&profile, &ft_entries, c))
        .unwrap_or_default();
    if let Some(catalog) = registry.forbidden_chaos_catalog() {
        if !effective_fids.is_empty() {
            merge_tech_fids_into_profile_with_level_tier(
                &mut profile,
                &effective_fids,
                &ft_entries,
                catalog,
                scale_by_level_tier,
            );
        }
    }
    if let Some(imported_buildings) = import::load_imported_buildings(
        profile_path(&pid, BUILDINGS_IMPORTED)
            .to_string_lossy()
            .as_ref(),
    ) {
        if !imported_buildings.is_empty() {
            if let Some(building_index) =
                building::load_building_index(DEFAULT_BUILDINGS_INDEX_PATH)
            {
                if let Some(bid_to_id) = load_bid_to_building_id(
                    DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
                    &building_index,
                ) {
                    let building_context = BuildingBonusContext {
                        ops_level: profile
                            .ops_level
                            .or_else(|| infer_ops_level(&imported_buildings, &bid_to_id)),
                        mode: BuildingMode::ShipCombat,
                        defender_opponent: BuildingDefenderOpponent::PlayerShip,
                        attacker_faction: resolve_ship(defender_ship_id)
                            .and_then(|s| s.faction)
                            .as_deref()
                            .map(BuildingAttackerFaction::from_ship_faction_slug)
                            .unwrap_or(BuildingAttackerFaction::Unknown),
                        attacker_tal_assigned_captain_or_bridge: false,
                        attacker_ship_type: resolve_ship(defender_ship_id)
                            .map(|s| ship_class_to_type(&s.ship_class)),
                    };
                    let data_dir = Path::new(DEFAULT_BUILDINGS_INDEX_PATH)
                        .parent()
                        .unwrap_or_else(|| Path::new("data/buildings"));
                    merge_building_bonuses_into_profile(
                        &mut profile,
                        &imported_buildings,
                        &bid_to_id,
                        &building_index,
                        data_dir,
                        &building_context,
                    );
                }
            }
        }
    }
    let research_path = profile_path(&pid, RESEARCH_IMPORTED)
        .to_string_lossy()
        .into_owned();
    let imported_research = import::load_imported_research(&research_path).unwrap_or_default();
    let canonical_overrides = load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH);
    let exclude_canonical_rids: HashSet<i64> = canonical_overrides.keys().copied().collect();
    let (incoming_shield_mitigation_bonus, incoming_shield_mitigation_bonus_rounds) =
        incoming_shield_mitigation_for_combat(&imported_research, &canonical_overrides);
    if let Some(catalog) = registry.research_catalog() {
        merge_research_bonuses_into_profile(
            &mut profile,
            &imported_research,
            catalog,
            Some(&exclude_canonical_rids),
        );
    }
    (
        profile,
        incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds,
    )
}

/// Build SharedScenarioData from registry (officers, ship, hostile) and load profile/roster/LCARS at call time.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_shared_scenario_data_from_registry(
    registry: &crate::data::data_registry::DataRegistry,
    ship: &str,
    hostile: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    profile_id: Option<&str>,
    support_buffs_request: Option<&[String]>,
    defender_opponent: DefenderOpponent,
    player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
    pvp: Option<PvpScenarioParams>,
) -> SharedScenarioData {
    let defender_opponent = if pvp.is_some() {
        DefenderOpponent::Player
    } else {
        defender_opponent
    };
    let officer_index = registry.officer_index().clone();

    let pid = profile_index::resolve_profile_id_for_api(profile_id);
    let profile_path_str = profile_path(&pid, PROFILE_JSON)
        .to_string_lossy()
        .to_string();
    let roster_path = profile_path(&pid, ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    let ft_path = profile_path(&pid, FORBIDDEN_TECH_IMPORTED)
        .to_string_lossy()
        .to_string();

    // Merge order invariants (profile bonus layer):
    // 1) forbidden/chaos tech (by fid) — may be scaled by tier/level (opt-in)
    // 2) buildings (by bid) — contextualized by ops level + mode
    // 3) research (by rid) — requires research catalog
    let mut profile = load_profile(&profile_path_str);
    let ft_entries = import::load_imported_forbidden_tech(&ft_path).unwrap_or_default();
    let mut forbidden_tech_derived_seats: Vec<CrewSeatContext> = Vec::new();
    let mut borg_alcove_hull_hp_bonus: Option<f64> = None;
    let scale_by_level_tier = forbidden_tech_level_tier_scaling_enabled_from_env();
    let effective_fids: Vec<i64> = registry
        .forbidden_chaos_catalog()
        .map(|c| resolve_effective_tech_fids(&profile, &ft_entries, c))
        .unwrap_or_default();
    if let Some(catalog) = registry.forbidden_chaos_catalog() {
        if !effective_fids.is_empty() {
            merge_tech_fids_into_profile_with_level_tier(
                &mut profile,
                &effective_fids,
                &ft_entries,
                catalog,
                scale_by_level_tier,
            );
            forbidden_tech_derived_seats = forbidden_tech_derived_attack_phase_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            );
            forbidden_tech_derived_seats.extend(
                quantum_slipstream_forbidden_tech_round_start_seats(
                    &ft_entries,
                    &effective_fids,
                    catalog,
                    scale_by_level_tier,
                ),
            );
            forbidden_tech_derived_seats.extend(ship_class_gated_torpedo_family_derived_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            ));
            forbidden_tech_derived_seats.extend(borg_operating_table_forbidden_tech_seats(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            ));
            borg_alcove_hull_hp_bonus = borg_alcove_hull_hp_bonus_fraction(
                &ft_entries,
                &effective_fids,
                catalog,
                scale_by_level_tier,
            );
        }
    }

    if let Some(imported_buildings) = import::load_imported_buildings(
        profile_path(&pid, BUILDINGS_IMPORTED)
            .to_string_lossy()
            .as_ref(),
    ) {
        if !imported_buildings.is_empty() {
            if let Some(building_index) =
                building::load_building_index(DEFAULT_BUILDINGS_INDEX_PATH)
            {
                if let Some(bid_to_id) = load_bid_to_building_id(
                    DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
                    &building_index,
                ) {
                    let building_context = BuildingBonusContext {
                        ops_level: profile
                            .ops_level
                            .or_else(|| infer_ops_level(&imported_buildings, &bid_to_id)),
                        mode: BuildingMode::ShipCombat,
                        defender_opponent: match defender_opponent {
                            DefenderOpponent::Hostile => BuildingDefenderOpponent::NpcHostile,
                            DefenderOpponent::Player => BuildingDefenderOpponent::PlayerShip,
                        },
                        attacker_faction: resolve_ship(ship)
                            .and_then(|s| s.faction)
                            .as_deref()
                            .map(BuildingAttackerFaction::from_ship_faction_slug)
                            .unwrap_or(BuildingAttackerFaction::Unknown),
                        attacker_tal_assigned_captain_or_bridge: false,
                        attacker_ship_type: resolve_ship(ship)
                            .map(|s| ship_class_to_type(&s.ship_class)),
                    };
                    let data_dir = Path::new(DEFAULT_BUILDINGS_INDEX_PATH)
                        .parent()
                        .unwrap_or_else(|| Path::new("data/buildings"));
                    merge_building_bonuses_into_profile(
                        &mut profile,
                        &imported_buildings,
                        &bid_to_id,
                        &building_index,
                        data_dir,
                        &building_context,
                    );
                }
            }
        }
    }

    let (resolved_support_buffs, unknown_support_buff_ids) =
        match (registry.support_buffs_catalog(), support_buffs_request) {
            (Some(cat), Some(req)) if !req.is_empty() => {
                support_buffs::resolve_selected_support_buff_ids(cat, req)
            }
            _ => (Vec::new(), Vec::new()),
        };
    let support_research_gates =
        SupportBuffResearchGateState::from_resolved_support_buff_ids(&resolved_support_buffs);

    let research_path = profile_path(&pid, RESEARCH_IMPORTED)
        .to_string_lossy()
        .into_owned();
    let imported_research = import::load_imported_research(&research_path).unwrap_or_default();
    let canonical_overrides = load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH);
    let exclude_canonical_rids: HashSet<i64> = canonical_overrides.keys().copied().collect();
    let (incoming_shield_mitigation_bonus, incoming_shield_mitigation_bonus_rounds) =
        incoming_shield_mitigation_for_combat(&imported_research, &canonical_overrides);

    let research_derived_seats = if let Some(catalog) = registry.research_catalog() {
        merge_research_bonuses_into_profile(
            &mut profile,
            &imported_research,
            catalog,
            Some(&exclude_canonical_rids),
        );
        research_derived_attack_phase_seats(
            &imported_research,
            catalog,
            &support_research_gates,
            &canonical_overrides,
        )
    } else {
        Vec::new()
    };

    let research_for_buffs = if !resolved_support_buffs.is_empty() {
        registry.research_catalog()
    } else {
        None
    };
    let (mut support_static_buffs, support_defender_static_buffs) =
        match registry.support_buffs_catalog() {
            Some(c) => {
                support_buffs::aggregate_support_static_bonuses_split(c, &resolved_support_buffs)
            }
            None => (HashMap::new(), HashMap::new()),
        };
    if let Some(cat) = registry.research_catalog() {
        support_buffs::augment_static_buffs_with_support_gated_research(
            &mut support_static_buffs,
            &imported_research,
            cat,
            &support_research_gates,
        );
    }
    if let Some(cat) = registry.support_buffs_catalog() {
        if !resolved_support_buffs.is_empty() {
            support_buffs::apply_support_buff_research_to_profile(
                &mut profile,
                cat,
                &resolved_support_buffs,
                research_for_buffs,
            );
        }
    }
    let applied_support_buffs = registry
        .support_buffs_catalog()
        .map(|cat| support_buffs::describe_resolved_support_buffs(cat, &resolved_support_buffs))
        .unwrap_or_default();

    let lcars_data = registry
        .lcars_officers()
        .map(|officers| lcars_officer_data_from_officers(officers.to_vec()));

    let resolve_options = import::load_imported_roster(&roster_path)
        .map(|entries| {
            let officer_tiers: HashMap<String, u8> = entries
                .into_iter()
                .filter_map(|e| e.tier.map(|t| (e.canonical_officer_id, t)))
                .collect();
            ResolveOptions {
                tier: None,
                officer_tiers: if officer_tiers.is_empty() {
                    None
                } else {
                    Some(officer_tiers)
                },
                officer_levels: None,
            }
        })
        .unwrap_or_default();

    let ship_rec = registry.resolve_ship_with_tier_level(ship, ship_tier, ship_level);
    let hostile_rec = if pvp.is_some() {
        None
    } else {
        registry.resolve_hostile(hostile)
    };
    let (defender_ship_rec, defender_profile, defender_incoming_shield_mitigation_bonus, defender_incoming_shield_mitigation_bonus_rounds) =
        if let Some(ref pvp_cfg) = pvp {
            let def_rec = registry.resolve_ship_with_tier_level(
                &pvp_cfg.defender_ship,
                pvp_cfg.defender_ship_tier,
                pvp_cfg.defender_ship_level,
            );
            let (def_prof, ism, ismr) = load_defender_profile_for_pvp(
                registry,
                &pvp_cfg.defender_profile_id,
                &pvp_cfg.defender_ship,
            );
            (def_rec, Some(def_prof), ism, ismr)
        } else {
            (None, None, 0.0, 0)
        };

    let class_gated_tp_hull = registry.forbidden_chaos_catalog().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hull_hp_bonus_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });
    let class_gated_tp_shield_mit = registry.forbidden_chaos_catalog().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hostile_shield_mitigation_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });
    let class_gated_tp_accuracy = registry.forbidden_chaos_catalog().and_then(|cat| {
        if effective_fids.is_empty() {
            return None;
        }
        ship_class_gated_torpedo_family_hostile_accuracy_sum_for_resolved_ship(
            &ft_entries,
            &effective_fids,
            cat,
            scale_by_level_tier,
            ship_rec.as_ref(),
        )
    });

    apply_class_gated_torpedo_family_hostile_accuracy_to_profile(
        &mut profile,
        defender_opponent,
        class_gated_tp_accuracy,
    );

    let (
        cached_defender,
        cached_rounds,
        cached_defender_hull,
        cached_pierce,
        cached_defender_mitigation,
    ) = if let (Some(ref ship_r), Some(ref def_ship_r), Some(ref def_profile)) =
        (&ship_rec, &defender_ship_rec, &defender_profile)
    {
        let empty_buffs = HashMap::new();
        let (defender_mitigation, pierce) = mitigation_and_pierce_for_player_vs_player(
            ship_r,
            def_ship_r,
            &profile,
            &empty_buffs,
        );
        let def_level = pvp
            .as_ref()
            .and_then(|p| p.defender_ship_level)
            .unwrap_or(1);
        let rounds = 100u32.min(10u32.saturating_add(def_level));
        let defender_id = pvp
            .as_ref()
            .map(|p| p.defender_ship.as_str())
            .unwrap_or(hostile);
        let defender = defender_combatant_from_ship_record(
            defender_id,
            def_ship_r,
            defender_mitigation,
            ship_r.ship_type(),
            ship_r.to_defender_stats(),
            def_profile,
            &empty_buffs,
        );
        // Profile + officer-stat runtime (including Phase 4c cross-side debuffs) applied per
        // candidate in [`scenario_to_combat_input_from_shared`] via [`build_pvp_defender_combatant`].
        (
            Some(defender),
            Some(rounds),
            Some(def_ship_r.hull_health),
            Some(pierce),
            Some(defender_mitigation),
        )
    } else if let (Some(ref ship_r), Some(ref hostile_r)) = (&ship_rec, &hostile_rec) {
        let mut attacker_stats = ship_r.to_attacker_stats();
        apply_profile_accuracy_to_attacker_stats(&mut attacker_stats, &profile);
        let defender_mitigation = mitigation_for_hostile(
            hostile_r.to_defender_stats(),
            attacker_stats,
            hostile_r.ship_type_for_combat(),
            hostile_r.mystery_mitigation_factor.unwrap_or(0.0),
            hostile_r.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
            hostile_r.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
        );
        let pierce = pierce_damage_through_bonus(
            hostile_r.to_defender_stats(),
            attacker_stats,
            hostile_r.ship_type_for_combat(),
        );
        let defender = defender_combatant_from_hostile_record(
            hostile,
            hostile_r,
            defender_mitigation,
            ship_r.ship_type(),
            ship_r.to_defender_stats(),
            None,
        );
        let rounds = 100u32.min(10u32.saturating_add(hostile_r.level));
        (
            Some(defender),
            Some(rounds),
            Some(hostile_r.hull_health),
            Some(pierce),
            Some(defender_mitigation),
        )
    } else {
        (None, None, None, None, None)
    };

    let using_placeholder_combatants = cached_defender.is_none();

    let engagement_enemy_types = if pvp.is_some() {
        EnemyTypes::default()
    } else {
        hostile_rec
            .as_ref()
            .map(|h| h.engagement_enemy_types_for_combat())
            .unwrap_or_default()
    };
    let defender_level = if let Some(ref pvp_cfg) = pvp {
        pvp_cfg.defender_ship_level
    } else {
        hostile_rec.as_ref().map(|h| h.level)
    };

    let attacker_owner_faction = attacker_owner_faction_from_ship(ship_rec.as_ref());
    let defender_faction_tag =
        defender_faction_tag_for_scenario(hostile_rec.as_ref(), defender_ship_rec.as_ref());
    let (dual_gate_research_hull_hp, dual_gate_research_shield_hp) =
        dual_gate_hull_shield_for_scenario(
            registry.research_catalog(),
            &imported_research,
            &exclude_canonical_rids,
            ship_rec.as_ref(),
            defender_faction_tag,
        );

    let (
        player_defender_officer_seats,
        player_defender_static_buffs,
        defender_officer_stat_totals,
        defender_bridge_officer_stat_totals,
        defender_pending_officer_stat_contributions,
    ) = resolve_player_defender_officer_bundle(
        player_defender_officer_crew.as_ref(),
        &officer_index,
        lcars_data.as_ref(),
        &resolve_options,
    );

    let scenario_hostile_key = pvp
        .as_ref()
        .map(|p| p.defender_ship.clone())
        .unwrap_or_else(|| hostile.to_string());

    SharedScenarioData {
        ship: ship.to_string(),
        hostile: scenario_hostile_key,
        officer_index,
        profile,
        lcars_data,
        resolve_options,
        ship_rec,
        hostile_rec,
        cached_defender,
        cached_rounds,
        cached_defender_hull,
        cached_pierce,
        cached_defender_mitigation,
        using_placeholder_combatants,
        resolved_support_buffs,
        applied_support_buffs,
        support_static_buffs,
        support_defender_static_buffs,
        unknown_support_buff_ids,
        research_derived_seats,
        forbidden_tech_derived_seats,
        borg_alcove_hull_hp_bonus,
        class_gated_torpedo_family_hull_hp_bonus: class_gated_tp_hull,
        class_gated_torpedo_family_hostile_shield_mitigation_sum: class_gated_tp_shield_mit,
        defender_opponent,
        attacker_owner_faction,
        dual_gate_research_hull_hp,
        dual_gate_research_shield_hp,
        engagement_enemy_types,
        defender_level,
        incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds,
        player_defender_officer_seats,
        player_defender_static_buffs,
        defender_officer_stat_totals,
        defender_bridge_officer_stat_totals,
        defender_pending_officer_stat_contributions,
        pvp,
        defender_ship_rec,
        defender_profile,
        defender_incoming_shield_mitigation_bonus,
        defender_incoming_shield_mitigation_bonus_rounds,
    }
}

fn infer_ops_level(
    imported_buildings: &[import::BuildingEntry],
    bid_to_id: &HashMap<i64, String>,
) -> Option<u32> {
    imported_buildings.iter().find_map(|entry| {
        let id = bid_to_id.get(&entry.bid)?;
        if id != "ops_center" {
            return None;
        }
        if entry.level < 0 {
            return Some(0);
        }
        Some(entry.level.min(i64::from(u32::MAX)) as u32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    use crate::combat::abilities::{AbilityClass, AbilityCondition, CrewSeat};
    use crate::combat::AttackerStats;
    use crate::combat::OpponentFactionTag;
    use crate::data::data_registry::DataRegistry;
    use crate::data::forbidden_chaos::{ForbiddenChaosList, ForbiddenChaosRecord};
    use crate::data::hostile::load_hostile_record;
    use crate::data::import::BuildingEntry;
    use crate::data::profile::merge_tech_fids_into_profile_with_level_tier;
    use crate::data::profile_index::{
        create_profile, delete_profile, load_profile_index, profile_path, PROFILE_JSON,
        RESEARCH_IMPORTED,
    };
    use crate::data::ship::{ShipAbility, ShipRecord};
    use crate::optimizer::crew_generator::CrewCandidate;
    use uuid::Uuid;

    static SHARED_SCENARIO_RESEARCH_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn defender_combatant_inherits_top_level_hostile_crit_damage_and_chance() {
        // Hostile JSON without per-weapon crit fields: top-level `crit_damage` / `crit_chance`
        // should flow into Combatant.crit_multiplier / crit_chance so the engine's counter-fire
        // accessors (`weapon_crit_multiplier(idx)` / `weapon_crit_chance(idx)`) fall back to them.
        let j = r#"{
            "id":"hh","hostile_name":"H","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "crit_chance":0.27,"crit_damage":2.4
        }"#;
        let rec: HostileRecord = serde_json::from_str(j).unwrap();
        let d = defender_combatant_from_hostile_record(
            "hh",
            &rec,
            0.0,
            ShipType::Battleship,
            DefenderStats::default(),
            None,
        );
        assert!((d.crit_chance - 0.27).abs() < 1e-12);
        assert!((d.crit_multiplier - 2.4).abs() < 1e-12);
        // No weapon rows: weapon accessor index 0 falls back to the scalar fields.
        assert!((d.weapon_crit_chance(0) - 0.27).abs() < 1e-12);
        assert!((d.weapon_crit_multiplier(0) - 2.4).abs() < 1e-12);
    }

    #[test]
    fn defender_combatant_per_weapon_crit_overrides_top_level() {
        // When a hostile weapon component carries its own `crit_chance` / `crit_modifier`, the
        // per-weapon accessor must prefer the row over the top-level fallback.
        let j = r#"{
            "id":"hh2","hostile_name":"H2","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "crit_chance":0.10,"crit_damage":1.5,
            "components":[
                {"order":1,"data":{"tag":"Weapon","minimum_damage":10,"maximum_damage":20,
                  "crit_chance":0.5,"crit_modifier":3.0}}
            ]
        }"#;
        let rec: HostileRecord = serde_json::from_str(j).unwrap();
        let d = defender_combatant_from_hostile_record(
            "hh2",
            &rec,
            0.0,
            ShipType::Battleship,
            DefenderStats::default(),
            None,
        );
        assert_eq!(d.weapons.len(), 1);
        assert!((d.weapon_crit_chance(0) - 0.5).abs() < 1e-12);
        assert!((d.weapon_crit_multiplier(0) - 3.0).abs() < 1e-12);
        // Combatant scalars still hold the top-level values (consumed when a row is silent).
        assert!((d.crit_chance - 0.10).abs() < 1e-12);
        assert!((d.crit_multiplier - 1.5).abs() < 1e-12);
    }

    #[test]
    fn defender_combatant_clamps_hostile_crit_chance_to_unit_interval() {
        let j = r#"{
            "id":"hh3","hostile_name":"H3","level":1,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "crit_chance":1.5
        }"#;
        let rec: HostileRecord = serde_json::from_str(j).unwrap();
        let d = defender_combatant_from_hostile_record(
            "hh3",
            &rec,
            0.0,
            ShipType::Battleship,
            DefenderStats::default(),
            None,
        );
        assert!((d.crit_chance - 1.0).abs() < 1e-12);
    }

    #[test]
    fn bundled_numeric_hostile_maps_weapon_components_for_scenario() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/hostiles");
        let rec = load_hostile_record(&dir, "2918121098").expect("bundled hostile 2918121098.json");
        let w = rec.weapons_from_components();
        assert!(
            !w.is_empty(),
            "expected data.stfc.space style hostile to yield per-weapon stats from components"
        );
        let d = defender_combatant_from_hostile_record(
            "2918121098",
            &rec,
            0.5,
            ShipType::Explorer,
            DefenderStats::default(),
            None,
        );
        assert!(!d.weapons.is_empty());
        assert!(d.pierce > 0.0, "counter pierce-through should be positive");
    }

    #[test]
    fn profile_merge_order_is_non_commutative_for_mult_then_add() {
        // This test locks the reason we care about merge order:
        // forbidden/chaos tech supports multiplicative bonuses (`operator: mult`) while buildings/research
        // tend to be additive; therefore the order changes results.
        let mut profile = PlayerProfile::default();
        let catalog = ForbiddenChaosList {
            source: Some("test".into()),
            last_updated: None,
            items: vec![ForbiddenChaosRecord {
                fid: Some(1),
                name: "Test mult".into(),
                tech_type: "forbidden".into(),
                tier: None,
                bonuses: vec![crate::data::forbidden_chaos::BonusEntry {
                    stat: "weapon_damage".into(),
                    value: 0.10,
                    operator: "mult".into(),
                }],
            }],
        };
        let imported = vec![crate::data::import::ForbiddenTechEntry {
            fid: 1,
            tier: 1,
            level: 1,
            shard_count: 0,
        }];

        // Intended order: FT mult first, then additive layers.
        merge_tech_fids_into_profile_with_level_tier(
            &mut profile,
            &[1],
            &imported,
            &catalog,
            false,
        );
        let after_ft = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
        assert!((after_ft - 0.10).abs() < 1e-9);

        // Simulate building + research additive merges (0.20 + 0.30).
        *profile.bonuses.entry("weapon_damage".into()).or_insert(0.0) += 0.20;
        *profile.bonuses.entry("weapon_damage".into()).or_insert(0.0) += 0.30;
        let intended = profile.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
        assert!((intended - 0.60).abs() < 1e-9);

        // Wrong order: additive first, then FT mult yields an interaction term (b*v).
        let mut wrong = PlayerProfile::default();
        wrong.bonuses.insert("weapon_damage".into(), 0.50);
        merge_tech_fids_into_profile_with_level_tier(&mut wrong, &[1], &imported, &catalog, false);
        let wrong_v = wrong.bonuses.get("weapon_damage").copied().unwrap_or(0.0);
        assert!(
            (wrong_v - intended).abs() > 1e-6,
            "expected non-commutative merge; intended={intended}, wrong={wrong_v}"
        );
    }

    #[test]
    fn research_merged_accuracy_multiplies_ship_base_in_effective_attacker_stats() {
        use crate::data::import::ResearchEntry;
        use crate::data::profile::merge_research_bonuses_into_profile;
        use crate::data::research::{
            ResearchBonusEntry, ResearchCatalog, ResearchLevel, ResearchRecord,
        };

        let catalog = ResearchCatalog {
            source: Some("test".into()),
            last_updated: None,
            items: vec![ResearchRecord {
                rid: 42_001,
                name: None,
                data_version: None,
                source_note: None,
                levels: vec![ResearchLevel {
                    level: 1,
                    bonuses: vec![ResearchBonusEntry {
                        stat: "accuracy".into(),
                        value: 0.05,
                        operator: "add".into(),
                        condition: Default::default(),
                    }],
                }],
            }],
        };
        let imported = vec![ResearchEntry {
            rid: 42_001,
            level: 1,
        }];
        let mut profile = PlayerProfile::default();
        merge_research_bonuses_into_profile(&mut profile, &imported, &catalog, None);

        let ship_rec = ShipRecord {
            id: "t".into(),
            ship_name: "T".into(),
            ship_class: "explorer".into(),
            faction: None,
            armor_piercing: 100.0,
            shield_piercing: 100.0,
            accuracy: 400.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 1.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: None,
            ..Default::default()
        };
        let stats = effective_attacker_stats_for_mitigation(
            &ship_rec,
            &profile,
            &HashMap::new(),
            ShipType::Battleship,
        );
        assert!(
            (stats.accuracy - 420.0).abs() < 1e-6,
            "expected 400 * (1 + 0.05) research accuracy bonus, got {}",
            stats.accuracy
        );
    }

    #[test]
    fn ship_ability_catalog_overrides_apply_when_normalized_to_ships_extended() {
        // This asserts end-to-end wiring:
        // ship_ability_catalog_overrides.json → ship_ability_catalog.json → normalize_data_stfc_space →
        // ships_extended/<id>.json → scenario extend_crew_with_ship_abilities → ship ability seat with faction gate.
        //
        // Franklin-A has upstream ability id `2016654425` which is overridden to:
        // - combat_begin + attack_multiplier
        // - condition_opponent_faction: swarm
        //
        // Pick a known Swarm hostile from bundled `data/hostiles/`.
        let swarm_hostile_id = "23104545";

        let registry = DataRegistry::load().expect("DataRegistry::load");
        let shared = build_shared_scenario_data_from_registry(
            registry.as_ref(),
            "uss_franklin_a",
            swarm_hostile_id,
            Some(1),
            Some(1),
            None,
            None,
            DefenderOpponent::Hostile,
            None,
            None,
        );

        let ship = shared.ship_rec.as_ref().expect("ship record");
        let hostile = shared.hostile_rec.as_ref().expect("hostile record");
        assert_eq!(hostile.opponent_faction_tag(), OpponentFactionTag::Swarm);

        let mut seats = Vec::new();
        extend_crew_with_ship_abilities(&mut seats, Some(ship));
        let has_swarm_gated_attack_mult = seats.iter().any(|s| {
            s.seat == CrewSeat::Ship
                && matches!(s.ability.effect, AbilityEffect::AttackMultiplier(_))
                && matches!(
                    s.ability.condition,
                    Some(AbilityCondition::DefenderFactionIs(
                        OpponentFactionTag::Swarm
                    ))
                )
        });
        assert!(
            has_swarm_gated_attack_mult,
            "expected Swarm-gated ship ability AttackMultiplier seat from ships_extended catalog import"
        );
    }

    #[test]
    fn effective_accuracy_stacks_profile_officer_buffs_hull_combat_begin_and_mult() {
        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("accuracy".to_string(), 0.25);

        let ship_rec = ShipRecord {
            id: "s".into(),
            ship_name: "S".into(),
            ship_class: "battleship".into(),
            faction: None,
            armor_piercing: 100.0,
            shield_piercing: 100.0,
            accuracy: 100.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 50.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: Some(vec![ShipAbility {
                id: "cb".into(),
                timing: "combat_begin".into(),
                effect_type: "accuracy".into(),
                value: 20.0,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
                condition_opponent_ship_class: None,
                condition_opponent_hostile_tags: None,
                round_cap: None,
                level_scaled_values: None,
            }]),
            ..Default::default()
        };
        let mut static_buffs = HashMap::new();
        static_buffs.insert("accuracy".to_string(), 10.0);
        static_buffs.insert("accuracy_cb_mult".to_string(), 1.2);

        let stats = effective_attacker_stats_for_mitigation(
            &ship_rec,
            &profile,
            &static_buffs,
            ShipType::Battleship,
        );
        // 100 * 1.25 = 125; +10 officer static = 135; +20 hull combat_begin = 155; *1.2 = 186
        assert!(
            (stats.accuracy - 186.0).abs() < 1e-6,
            "got {}",
            stats.accuracy
        );

        let ship_rec_vs_interceptor_only = ShipRecord {
            abilities: Some(vec![ShipAbility {
                id: "cb2".into(),
                timing: "combat_begin".into(),
                effect_type: "accuracy".into(),
                value: 99.0,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
                condition_opponent_ship_class: Some("interceptor".into()),
                condition_opponent_hostile_tags: None,
                round_cap: None,
                level_scaled_values: None,
            }]),
            ..ship_rec.clone()
        };
        let stats_mismatch = effective_attacker_stats_for_mitigation(
            &ship_rec_vs_interceptor_only,
            &profile,
            &static_buffs,
            ShipType::Battleship,
        );
        let stats_match = effective_attacker_stats_for_mitigation(
            &ship_rec_vs_interceptor_only,
            &profile,
            &static_buffs,
            ShipType::Interceptor,
        );
        assert!(
            (stats_match.accuracy - stats_mismatch.accuracy - 99.0 * 1.2).abs() < 1e-6,
            "hull accuracy with interceptor gate should apply only vs interceptors"
        );

        let ship_rec_round_capped = ShipRecord {
            abilities: Some(vec![ShipAbility {
                id: "cb3".into(),
                timing: "combat_begin".into(),
                effect_type: "accuracy".into(),
                value: 77.0,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
                condition_opponent_ship_class: None,
                condition_opponent_hostile_tags: None,
                round_cap: Some(3),
                level_scaled_values: None,
            }]),
            ..ship_rec.clone()
        };
        let stats_capped = effective_attacker_stats_for_mitigation(
            &ship_rec_round_capped,
            &profile,
            &static_buffs,
            ShipType::Battleship,
        );
        assert!(
            (stats_capped.accuracy - 135.0 * 1.2).abs() < 1e-6,
            "round-capped combat_begin hull accuracy must not fold into static mitigation accuracy"
        );
    }

    #[test]
    fn officer_accuracy_buff_lowers_hostile_mitigation_and_raises_pierce() {
        // Moderate defender stats so mitigation is not stuck at the hostile ceiling (high-tier
        // bundled hostiles often clamp, hiding the dodge/accuracy leg).
        let hostile_rec: HostileRecord = serde_json::from_value(serde_json::json!({
            "id": "test_hostile",
            "hostile_name": "Test Hostile",
            "level": 10,
            "ship_class": "battleship",
            "armor": 220.0,
            "shield_deflection": 210.0,
            "dodge": 190.0,
            "hull_health": 5000.0,
            "shield_health": 2000.0
        }))
        .expect("minimal hostile JSON");
        let ship_rec = ShipRecord {
            id: "test_ship".into(),
            ship_name: "Test".into(),
            ship_class: "explorer".into(),
            faction: None,
            armor_piercing: 150.0,
            shield_piercing: 150.0,
            accuracy: 120.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 100.0,
            crit_chance: 0.1,
            crit_damage: 1.5,
            hull_health: 5000.0,
            shield_health: 1000.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: None,
            ..Default::default()
        };
        let profile = PlayerProfile::default();
        let (m0, p0) = mitigation_and_pierce_for_player_vs_hostile(
            &ship_rec,
            &hostile_rec,
            &profile,
            &HashMap::new(),
        );
        let mut buffs = HashMap::new();
        buffs.insert("accuracy".to_string(), 80.0);
        let (m1, p1) =
            mitigation_and_pierce_for_player_vs_hostile(&ship_rec, &hostile_rec, &profile, &buffs);
        assert!(
            p1 > p0 && m1 < m0,
            "expected more pierce and less mitigation with accuracy; m0={m0} m1={m1} p0={p0} p1={p1}"
        );
    }

    #[test]
    fn ship_abilities_merged_when_shared_scenario_fallback_no_cached_defender() {
        let ship_rec = ShipRecord {
            id: "test_ship".into(),
            ship_name: "Test".into(),
            ship_class: "battleship".into(),
            faction: None,
            armor_piercing: 100.0,
            shield_piercing: 100.0,
            accuracy: 100.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            attack: 50.0,
            crit_chance: 0.0,
            crit_damage: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            weapons: None,
            abilities: Some(vec![ShipAbility {
                id: "1".into(),
                timing: "round_start".into(),
                effect_type: "pierce_bonus".into(),
                value: 0.05,
                duration_rounds: None,
                condition_morale: false,
                condition_defender_burning: false,
                condition_defender_hull_breach: false,
                condition_opponent_faction: None,
                condition_opponent_ship_class: None,
                condition_opponent_hostile_tags: None,
                round_cap: None,
                level_scaled_values: None,
            }]),
            ..Default::default()
        };

        let shared = SharedScenarioData {
            ship: "test_ship".into(),
            hostile: "unknown_hostile_xyz".into(),
            officer_index: HashMap::new(),
            profile: PlayerProfile::default(),
            lcars_data: None,
            resolve_options: ResolveOptions {
                tier: None,
                officer_tiers: None,
                officer_levels: None,
            },
            ship_rec: Some(ship_rec),
            hostile_rec: None,
            cached_defender: None,
            cached_rounds: None,
            cached_defender_hull: None,
            cached_pierce: None,
            cached_defender_mitigation: None,
            using_placeholder_combatants: true,
            resolved_support_buffs: vec![],
            applied_support_buffs: vec![],
            support_static_buffs: HashMap::new(),
            support_defender_static_buffs: HashMap::new(),
            unknown_support_buff_ids: vec![],
            research_derived_seats: vec![],
            forbidden_tech_derived_seats: vec![],
            borg_alcove_hull_hp_bonus: None,
            class_gated_torpedo_family_hull_hp_bonus: None,
            class_gated_torpedo_family_hostile_shield_mitigation_sum: None,
            defender_opponent: DefenderOpponent::Hostile,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            dual_gate_research_hull_hp: 0.0,
            dual_gate_research_shield_hp: 0.0,
            engagement_enemy_types: EnemyTypes::default(),
            defender_level: None,
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            player_defender_officer_seats: vec![],
            player_defender_static_buffs: HashMap::new(),
            defender_officer_stat_totals: crate::combat::CrewOfficerStatTotals::default(),
            defender_bridge_officer_stat_totals: crate::combat::CrewOfficerStatTotals::default(),
            defender_pending_officer_stat_contributions: Vec::new(),
            pvp: None,
            defender_ship_rec: None,
            defender_profile: None,
            defender_incoming_shield_mitigation_bonus: 0.0,
            defender_incoming_shield_mitigation_bonus_rounds: 0,
        };

        let candidate = CrewCandidate {
            captain: "Kirk".to_string(),
            bridge: vec!["Spock".to_string(), "Uhura".to_string()],
            below_decks: vec![
                "Scotty".to_string(),
                "McCoy".to_string(),
                "Rand".to_string(),
            ],
        };

        let input = scenario_to_combat_input_from_shared(&shared, &candidate, 1);
        let ship_seats: Vec<_> = input
            .crew
            .seats
            .iter()
            .filter(|s| s.seat == CrewSeat::Ship)
            .collect();
        assert_eq!(ship_seats.len(), 1);
        assert_eq!(ship_seats[0].ability.class, AbilityClass::ShipAbility);
    }

    #[test]
    fn computed_mitigation_changes_with_defense_and_piercing_inputs() {
        let ship_hash = hash_identifier("USS Enterprise");
        let hostile_hash = hash_identifier("Hostile D4");
        let ship_type = synthetic_ship_type(hostile_hash);

        let base_defender = synthetic_defender_stats(hostile_hash);
        let base_attacker = synthetic_attacker_stats(ship_hash);
        let base = mitigation(base_defender, base_attacker, ship_type);

        let stronger_defender = mitigation(
            DefenderStats {
                armor: base_defender.armor * 1.4,
                shield_deflection: base_defender.shield_deflection * 1.4,
                dodge: base_defender.dodge * 1.4,
            },
            base_attacker,
            ship_type,
        );
        let stronger_attacker = mitigation(
            base_defender,
            AttackerStats {
                armor_piercing: base_attacker.armor_piercing * 1.4,
                shield_piercing: base_attacker.shield_piercing * 1.4,
                accuracy: base_attacker.accuracy * 1.4,
            },
            ship_type,
        );

        assert_ne!(base, stronger_defender);
        assert_ne!(base, stronger_attacker);
    }

    #[test]
    fn computed_mitigation_is_bounded_between_zero_and_one() {
        let samples = [
            ("Mayflower", "Borg Cube"),
            ("Saladin", "Klingon Patrol"),
            ("Kelvin", "Romulan Interceptor"),
            ("Defiant", "Dominion Cruiser"),
        ];

        for (ship, hostile) in samples {
            let value = computed_defender_mitigation(ship, hostile);
            assert!(
                (0.0..=1.0).contains(&value),
                "mitigation={value} for {ship} vs {hostile}"
            );
        }
    }

    #[test]
    fn computed_mitigation_is_deterministic_for_same_inputs() {
        let first = computed_defender_mitigation("Franklin", "Hostile Miner");
        let second = computed_defender_mitigation("Franklin", "Hostile Miner");
        assert_eq!(first, second);

        let candidate = CrewCandidate {
            captain: "Kirk".to_string(),
            bridge: vec!["Spock".to_string(), "Spock".to_string()],
            below_decks: vec![
                "Scotty".to_string(),
                "Scotty".to_string(),
                "Scotty".to_string(),
            ],
        };
        let officers = HashMap::new();
        let profile = PlayerProfile::default();

        let one = scenario_to_combat_input(
            "Franklin",
            "Hostile Miner",
            &candidate,
            7,
            &officers,
            &profile,
            None,
        );
        let two = scenario_to_combat_input(
            "Franklin",
            "Hostile Miner",
            &candidate,
            7,
            &officers,
            &profile,
            None,
        );
        assert_eq!(one.defender.mitigation, two.defender.mitigation);
    }

    #[test]
    fn infer_ops_level_uses_ops_center_building() {
        let imported_buildings = vec![
            BuildingEntry { bid: 1, level: 20 },
            BuildingEntry { bid: 99, level: 35 },
        ];
        let bid_to_id = HashMap::from([
            (1_i64, "ship_hangar".to_string()),
            (99_i64, "ops_center".to_string()),
        ]);

        assert_eq!(infer_ops_level(&imported_buildings, &bid_to_id), Some(35));
    }

    #[test]
    fn build_shared_scenario_data_merges_research_import_into_profile() {
        let _guard = SHARED_SCENARIO_RESEARCH_LOCK.lock().unwrap();

        let mut index = load_profile_index();
        let id = format!("scenario_research_{}", Uuid::new_v4().as_simple());
        let entry = create_profile(&mut index, Some(&id), "Scenario research integration")
            .expect("create profile");

        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let mut index = load_profile_index();
                let _ = delete_profile(&mut index, &self.0);
            }
        }
        let _cleanup = Cleanup(entry.id.clone());

        fs::write(profile_path(&entry.id, PROFILE_JSON), "{}").expect("profile.json");
        fs::write(
            profile_path(&entry.id, RESEARCH_IMPORTED),
            r#"{"research":[{"rid":2232304457,"level":2}]}"#,
        )
        .expect("research.imported.json");

        let registry = DataRegistry::load().expect("DataRegistry::load");
        let shared = build_shared_scenario_data_from_registry(
            registry.as_ref(),
            "saladin",
            "2918121098",
            None,
            None,
            Some(&entry.id),
            None,
            DefenderOpponent::Hostile,
            None,
            None,
        );

        // Catalog rid 2232304457: weapon_damage +0.05 at L1, +0.07 at L2 → cumulative 0.12
        let wd = shared
            .profile
            .bonuses
            .get("weapon_damage")
            .copied()
            .expect("weapon_damage from research merge");
        assert!(
            (wd - 0.12).abs() < 1e-9,
            "expected weapon_damage ≈ 0.12, got {wd} (bonuses={:?})",
            shared.profile.bonuses
        );
    }

    #[test]
    fn player_crit_reduction_profile_bonus_emits_pvp_only_seat() {
        let mut seats = Vec::new();
        let mut profile = PlayerProfile::default();
        profile
            .bonuses
            .insert("player_crit_damage_reduction".to_string(), 0.18);

        extend_crew_with_player_crit_damage_reduction_profile_bonus(
            &mut seats,
            &profile,
            DefenderOpponent::Hostile,
        );
        assert!(
            seats.is_empty(),
            "hostile fights must not receive pvp-only crit reduction seat"
        );

        extend_crew_with_player_crit_damage_reduction_profile_bonus(
            &mut seats,
            &profile,
            DefenderOpponent::Player,
        );
        assert_eq!(seats.len(), 1);
        match seats[0].ability.effect {
            AbilityEffect::HostileCritDamageReduction {
                reduction,
                duration_rounds,
            } => {
                assert!((reduction - 0.18).abs() < 1e-12);
                assert_eq!(duration_rounds, crate::combat::types::MAX_COMBAT_ROUNDS);
            }
            _ => panic!("unexpected effect emitted for player_crit_damage_reduction"),
        }
    }

    #[test]
    fn player_apex_barrier_profile_bonus_respects_tal_gate() {
        let mut profile = PlayerProfile::default();
        profile
            .bonuses
            .insert("apex_barrier_vs_player_tal_not_on_bridge".to_string(), 0.22);
        let mut attacker = Combatant {
            id: "s".into(),
            attack: 1.0,
            mitigation: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            weapons: vec![],
            end_of_round_damage: 0.0,
            hull_health: 1.0,
            shield_health: 1.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            hostile_mitigation_params: None,
        };

        apply_profile_player_apex_barrier_tal_gate(
            &mut attacker,
            &profile,
            DefenderOpponent::Hostile,
            false,
        );
        assert_eq!(attacker.apex_barrier, 0.0);

        apply_profile_player_apex_barrier_tal_gate(
            &mut attacker,
            &profile,
            DefenderOpponent::Player,
            true,
        );
        assert_eq!(attacker.apex_barrier, 0.0);

        apply_profile_player_apex_barrier_tal_gate(
            &mut attacker,
            &profile,
            DefenderOpponent::Player,
            false,
        );
        assert!((attacker.apex_barrier - 0.22).abs() < 1e-12);
    }
}
