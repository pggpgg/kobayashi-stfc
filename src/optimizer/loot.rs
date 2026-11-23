//! Farming loot & cargo capacity evaluation for chain-grind objectives (issue #415).
//!
//! Combines upstream hostile resource drops (`HostileRecord::resources`), applicable canonical
//! officer loot & cargo bonuses (`officers.canonical.json`), and ship cargo capacity
//! (`Cargo.max_resources`) so Loops and chain-grind optimization can distinguish plain repeat-kill
//! survival (`min_hull_damage`) from actual expected farming payout (`max_expected_loot`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::combat::{OpponentFactionTag, ShipType};
use crate::data::hostile::HostileRecord;
use crate::data::hostile_loca::is_basic_currency_name;
use crate::data::officer::{normalize_officer_lookup_key, Officer, OfficerAbility};
use crate::lcars::ResolveOptions;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::crew_resolution::split_name_and_tier;
use crate::optimizer::monte_carlo::scenario::SharedScenarioData;

/// Expected farming loot metrics attached to [`crate::optimizer::chain::ChainSimulationSummary`]
/// when the hostile carries positive drops and `max_expected_loot` is active (or whenever loot
/// accounting is evaluated for a farming chain).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChainLootSummary {
    /// Upstream resource id for the named primary farming drop (e.g. `103783761` = Tech Module).
    pub resource_id: u64,
    /// Human-readable name of the primary farming resource (e.g. `"Tech Module"`).
    pub resource_name: String,
    /// Mean expected quantity of `resource_name` collected across the chain (capped by cargo hold).
    pub expected_loot_mean: f64,
    /// Lower 95% confidence bound on `expected_loot_mean`.
    pub expected_loot_ci_low: f64,
    /// Upper 95% confidence bound on `expected_loot_mean`.
    pub expected_loot_ci_high: f64,
    /// Mean unbuffed drop per single hostile kill (`(min + max) / 2`).
    pub base_drop_per_kill: f64,
    /// Effective multiplicative loot factor (`1.0 + sum_applicable_officer_loot_bonuses`).
    pub loot_multiplier: f64,
    /// Effective ship cargo hold capacity after officer `CargoCapacity` bonuses (`None` when ship
    /// cargo data is unavailable, in which case drops are uncapped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_capacity: Option<f64>,
    /// True when uncapped `kills_target` drops exceed `cargo_capacity`.
    pub cargo_capped: bool,
    /// Human-readable summary of officer loot/cargo bonuses applied to this crew.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loot_sources: Vec<String>,
}

/// Precomputed per-crew farming loot parameters for a chain scenario.
#[derive(Debug, Clone)]
pub struct CrewLootProfile {
    pub resource_id: u64,
    pub resource_name: String,
    pub primary_base_per_kill: f64,
    pub other_base_per_kill: f64,
    pub primary_loot_multiplier: f64,
    pub other_loot_multiplier: f64,
    pub effective_cargo_capacity: Option<f64>,
    pub loot_sources: Vec<String>,
}

impl CrewLootProfile {
    /// Expected primary resource quantity collected after `kills` clean hostile kills, enforcing
    /// total cargo capacity across all positive drops from this hostile.
    pub fn primary_loot_for_kills(&self, kills: u32) -> f64 {
        if kills == 0 || self.primary_base_per_kill <= 0.0 {
            return 0.0;
        }
        let k = f64::from(kills);
        let primary_uncapped = k * self.primary_base_per_kill * self.primary_loot_multiplier;
        let other_uncapped = k * self.other_base_per_kill * self.other_loot_multiplier;
        let total_uncapped = primary_uncapped + other_uncapped;
        if let Some(cap) = self.effective_cargo_capacity.filter(|&c| c > 0.0) {
            if total_uncapped > cap && total_uncapped > 0.0 {
                return primary_uncapped * (cap / total_uncapped);
            }
        }
        primary_uncapped
    }

    /// True if `kills_target` kills would fill or exceed the effective cargo hold.
    pub fn is_cargo_capped_at_target(&self, kills_target: u32) -> bool {
        let Some(cap) = self.effective_cargo_capacity.filter(|&c| c > 0.0) else {
            return false;
        };
        let k = f64::from(kills_target.max(1));
        let total_uncapped = k * self.primary_base_per_kill * self.primary_loot_multiplier
            + k * self.other_base_per_kill * self.other_loot_multiplier;
        total_uncapped >= cap - 1e-6
    }
}

/// Resolve an officer from `officer_index` by normalized name or exact canonical id,
/// returning any explicit inline `(T#)` rank override carried on the slot label.
fn lookup_officer_with_tier<'a>(
    officer_index: &'a HashMap<String, Officer>,
    slot_value: &str,
) -> Option<(&'a Officer, Option<u8>)> {
    let (name_part, explicit_tier) = split_name_and_tier(slot_value);
    let trimmed = name_part.trim();
    if trimmed.is_empty() {
        return None;
    }
    let key = normalize_officer_lookup_key(trimmed);
    if let Some(off) = officer_index.get(&key) {
        return Some((off, explicit_tier));
    }
    officer_index
        .values()
        .find(|o| o.id.eq_ignore_ascii_case(trimmed))
        .map(|o| (o, explicit_tier))
}

fn officer_rank_1_based(officer: &Officer, explicit_tier: Option<u8>, opts: &ResolveOptions) -> u8 {
    if let Some(t) = explicit_tier {
        return t.clamp(1, 5);
    }
    if let Some(tiers) = opts.officer_tiers.as_ref() {
        if let Some(&t) = tiers
            .get(&officer.id)
            .or_else(|| tiers.get(&officer.name))
            .or_else(|| tiers.get(&normalize_officer_lookup_key(&officer.name)))
        {
            return t.clamp(1, 5);
        }
    }
    opts.tier.unwrap_or(1).clamp(1, 5)
}

/// Captain synergy value: `value_by_rank[0]` base + `value_by_rank[1]` (1 bridge synergy match)
/// + `value_by_rank[2]` (2 bridge synergy matches). Mirrors `SynergyBuffData::CalculateValueAndChance`.
fn captain_synergy_value(
    captain: &Officer,
    bridge: &[&Officer],
    ability: &OfficerAbility,
) -> Option<f64> {
    let base = *ability.value_by_rank.first()?;
    if !base.is_finite() {
        return None;
    }
    let cap_group = captain.group.as_deref().map(str::trim).unwrap_or("");
    let matching_bridge = if cap_group.is_empty() {
        0usize
    } else {
        bridge
            .iter()
            .filter(|b| {
                b.group
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|g| !g.is_empty() && g.eq_ignore_ascii_case(cap_group))
            })
            .count()
    };
    let mut total = base;
    if matching_bridge >= 1 {
        if let Some(&s1) = ability.value_by_rank.get(1) {
            if s1.is_finite() {
                total += s1;
            }
        }
    }
    if matching_bridge >= 2 {
        if let Some(&s2) = ability.value_by_rank.get(2) {
            if s2.is_finite() {
                total += s2;
            }
        }
    }
    Some(total)
}

fn rank_scaled_value(
    officer: &Officer,
    explicit_tier: Option<u8>,
    ability: &OfficerAbility,
    opts: &ResolveOptions,
) -> Option<f64> {
    if ability.value_by_rank.is_empty() {
        return None;
    }
    let rank = officer_rank_1_based(officer, explicit_tier, opts) as usize;
    let idx = rank.saturating_sub(1).min(ability.value_by_rank.len() - 1);
    let val = ability.value_by_rank[idx];
    (val.is_finite() && val > 0.0).then_some(val)
}

fn ship_condition_matches(condition: &str, ship_id: &str) -> bool {
    let ship = ship_id.trim().to_ascii_lowercase();
    match condition {
        "SelfHullFranklins" => ship == "uss_franklin" || ship == "uss_franklin_a",
        "SelfHullVoyager" => ship == "uss_voyager",
        "SelfHullNseaProtector" => ship == "nsea_protector",
        "SelfHullAmalgam" => ship == "amalgam",
        _ => false,
    }
}

/// Evaluate whether an officer loot/cargo ability's canonical conditions hold for this scenario.
/// Unknown or unsupported conditions fail closed (return false) per project accuracy policy.
fn loot_ability_conditions_match(
    conditions: &[String],
    ship_id: &str,
    defender_is_hostile: bool,
    defender_is_armada: bool,
) -> bool {
    for cond in conditions {
        let ok = match cond.as_str() {
            "EnemyHostile" => defender_is_hostile,
            "TargetNotArmada" | "TargetNotSoloArmada" => !defender_is_armada,
            "TargetIsArmada" => defender_is_armada,
            "SelfHullFranklins"
            | "SelfHullVoyager"
            | "SelfHullNseaProtector"
            | "SelfHullAmalgam" => ship_condition_matches(cond, ship_id),
            // Unknown / unsupported conditions (e.g. Origin Sector faction gate) fail closed.
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn hostile_is_hirogen_or_8472(hostile: &HostileRecord) -> bool {
    let name = hostile.hostile_name.to_ascii_lowercase();
    name.contains("hirogen") || name.contains("species 8472") || name.contains("8472")
}

/// Determine whether a canonical loot modifier applies to this hostile encounter and resource.
fn loot_modifier_applies(
    modifier: &str,
    hostile: &HostileRecord,
    defender_faction: OpponentFactionTag,
    defender_is_armada: bool,
    resource_name: &str,
) -> bool {
    let res_lower = resource_name.to_ascii_lowercase();
    match modifier {
        "CombatPveRewards" => true,
        "HostileLoot" => !defender_is_armada,
        "XindiHostileLoot" => defender_faction == OpponentFactionTag::Xindi,
        "ArmadaLoot" => defender_is_armada,
        "GornHostileVolatileLoot" => defender_faction == OpponentFactionTag::GornHuntingPack,
        "HirogenRelicAndBiotoxinLoot" => hostile_is_hirogen_or_8472(hostile),
        "ActianVenomAndNanoprobeLoot" => {
            matches!(
                defender_faction,
                OpponentFactionTag::Actian | OpponentFactionTag::Borg
            )
        }
        "TrelliumRewards" => defender_faction == OpponentFactionTag::MirrorUniverse,
        "WokAugmentAllLootRewards" => defender_faction == OpponentFactionTag::Augment,
        "BrokenShipPartsLoot" => res_lower.contains("broken") && res_lower.contains("part"),
        "CombatParsteelReward" => res_lower == "parsteel",
        "CombatTritaniumReward" => res_lower == "tritanium",
        "CombatDilithiumReward" => res_lower == "dilithium",
        "PveChestLootMultiplierLimitedResources" => {
            res_lower.contains("xindi")
                || res_lower.contains("hirogen")
                || res_lower.contains("biotoxin")
                || res_lower.contains("trellium")
                || res_lower.contains("fatu-kray")
                || res_lower.contains("mycellium")
                || res_lower.contains("mycelium")
        }
        _ => false,
    }
}

/// Select the primary farming drop `(resource_id, resource_name, primary_mean, other_drops_mean)`
/// from a hostile's `resources[]` table. Returns `None` if the hostile drops no positive resources.
pub fn select_hostile_farming_drops(
    hostile: &HostileRecord,
    resource_names: &HashMap<u64, String>,
) -> Option<(u64, String, f64, f64)> {
    let mut drops: Vec<(u64, String, f64, bool)> = Vec::new();
    for drop in &hostile.resources {
        let lo = drop.min.max(0) as f64;
        let hi = drop.max.max(0) as f64;
        let mean = (lo + hi) * 0.5;
        if !mean.is_finite() || mean <= 0.0 {
            continue;
        }
        let name = resource_names
            .get(&drop.resource_id)
            .cloned()
            .unwrap_or_else(|| format!("Resource {}", drop.resource_id));
        let is_specialty = !is_basic_currency_name(&name);
        drops.push((drop.resource_id, name, mean, is_specialty));
    }
    if drops.is_empty() {
        return None;
    }

    let (best_idx, _) = drops.iter().enumerate().max_by(|(_, a), (_, b)| {
        a.3.cmp(&b.3)
            .then_with(|| a.2.total_cmp(&b.2))
            .then_with(|| b.0.cmp(&a.0))
    })?;
    let (primary_id, primary_name, primary_mean, _) = drops[best_idx].clone();
    let other_mean: f64 = drops
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != best_idx)
        .map(|(_, d)| d.2)
        .sum();
    Some((primary_id, primary_name, primary_mean, other_mean))
}

/// Resolve the farming loot & cargo profile for one crew candidate against the shared scenario.
/// Returns `None` when the defender is not an NPC hostile or has no positive catalog drops.
pub(crate) fn resolve_crew_loot_profile(
    shared: &SharedScenarioData,
    candidate: &CrewCandidate,
) -> Option<CrewLootProfile> {
    if !shared.defender_opponent.defender_is_npc_hostile() {
        return None;
    }
    let hostile = shared.hostile_rec.as_ref()?;
    let (resource_id, resource_name, primary_base_per_kill, other_base_per_kill) =
        select_hostile_farming_drops(hostile, shared.resource_names.as_ref())?;

    let defender_faction = hostile.opponent_faction_tag();
    let defender_is_armada = shared.defender_ship_type_for_combat() == ShipType::Armada;
    let ship_id = shared
        .ship_rec
        .as_ref()
        .map(|s| s.id.as_str())
        .unwrap_or(shared.ship.as_str());

    let captain_entry = lookup_officer_with_tier(&shared.officer_index, &candidate.captain);
    let bridge_entries: Vec<(&Officer, Option<u8>)> = candidate
        .bridge
        .iter()
        .filter_map(|s| lookup_officer_with_tier(&shared.officer_index, s))
        .collect();
    let bridge_offs: Vec<&Officer> = bridge_entries.iter().map(|(o, _)| *o).collect();
    let below_entries: Vec<(&Officer, Option<u8>)> = candidate
        .below_decks
        .iter()
        .filter_map(|s| lookup_officer_with_tier(&shared.officer_index, s))
        .collect();

    let mut primary_loot_bonus = 0.0_f64;
    let mut other_loot_bonus = 0.0_f64;
    let mut cargo_bonus = 0.0_f64;
    let mut loot_sources: Vec<String> = Vec::new();

    // Helper to evaluate one seated officer's abilities for its active slot(s).
    let mut eval_officer =
        |off: &Officer, explicit_tier: Option<u8>, active_slots: &[&str], is_captain_seat: bool| {
            for ab in &off.abilities {
                let slot = ab.slot.as_str();
                if !active_slots.contains(&slot) {
                    continue;
                }
                let Some(modifier) = ab.modifier.as_deref() else {
                    continue;
                };
                if !loot_ability_conditions_match(&ab.conditions, ship_id, true, defender_is_armada)
                {
                    continue;
                }
                let val = if slot == "captain" && is_captain_seat {
                    captain_synergy_value(off, &bridge_offs, ab)
                } else {
                    rank_scaled_value(off, explicit_tier, ab, &shared.resolve_options)
                };
                let Some(val) = val.filter(|&v| v.is_finite() && v > 0.0) else {
                    continue;
                };

                if modifier == "CargoCapacity" {
                    cargo_bonus += val;
                    loot_sources.push(format!(
                        "{} (+{}% cargo)",
                        off.name,
                        (val * 100.0).round() as i64
                    ));
                    continue;
                }

                if loot_modifier_applies(
                    modifier,
                    hostile,
                    defender_faction,
                    defender_is_armada,
                    &resource_name,
                ) {
                    primary_loot_bonus += val;
                    // General/faction loot multipliers also amplify secondary drops from the same hostile.
                    if !matches!(
                        modifier,
                        "BrokenShipPartsLoot"
                            | "CombatParsteelReward"
                            | "CombatTritaniumReward"
                            | "CombatDilithiumReward"
                            | "PveChestLootMultiplierLimitedResources"
                    ) {
                        other_loot_bonus += val;
                    }
                    loot_sources.push(format!(
                        "{} (+{}% loot)",
                        off.name,
                        (val * 100.0).round() as i64
                    ));
                }
            }
        };

    // Captain uses both captain maneuver (`captain`) and officer ability (`officer`).
    if let Some((cap, tier)) = captain_entry {
        eval_officer(cap, tier, &["captain", "officer"], true);
    }
    for &(br, tier) in &bridge_entries {
        eval_officer(br, tier, &["officer"], false);
    }
    for &(bd, tier) in &below_entries {
        eval_officer(bd, tier, &["below_decks"], false);
    }

    let effective_cargo_capacity = shared
        .ship_cargo_capacity
        .filter(|&c| c.is_finite() && c > 0.0)
        .map(|base| base * (1.0 + cargo_bonus.max(0.0)));

    Some(CrewLootProfile {
        resource_id,
        resource_name,
        primary_base_per_kill,
        other_base_per_kill,
        primary_loot_multiplier: 1.0 + primary_loot_bonus.max(0.0),
        other_loot_multiplier: 1.0 + other_loot_bonus.max(0.0),
        effective_cargo_capacity,
        loot_sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::hostile::{HostileFactionRef, HostileResourceDrop};

    fn yoton_cruiser_stub() -> HostileRecord {
        HostileRecord {
            id: "3919830203".into(),
            hostile_name: "Xindi-Aquatic Cruiser".into(),
            level: 62,
            ship_class: "interceptor".into(),
            armor: 1.0,
            shield_deflection: 1.0,
            dodge: 1.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: None,
            apex_barrier: 0.0,
            isolytic_defense: 0.0,
            mitigation_floor: None,
            mitigation_ceiling: None,
            mystery_mitigation_factor: None,
            loca_id: Some(63050),
            faction: Some(HostileFactionRef {
                id: 2_745_774_076,
                loca_id: Some(32),
            }),
            upstream_ship_type: 5,
            hull_type_raw: 0,
            rarity: 1,
            is_scout: false,
            is_outpost: false,
            strength: 0,
            systems: vec![589_242_343],
            xp_amount: 0,
            warp: 1000,
            warp_with_superhighway: 1000,
            stat_health: 0.0,
            stat_defense: 0.0,
            stat_attack: 0.0,
            dpr: 0.0,
            stat_strength: 0.0,
            accuracy: 0.0,
            armor_piercing: 0.0,
            shield_piercing: 0.0,
            crit_chance: 0.0,
            crit_damage: 1.5,
            crit_damage_is_bonus: None,
            hostile_tags: vec![],
            engagement_enemy_types: None,
            components: vec![],
            ability: vec![],
            resources: vec![
                HostileResourceDrop {
                    resource_id: 103_783_761,
                    min: 1_081_486,
                    max: 1_135_561,
                },
                HostileResourceDrop {
                    resource_id: 2_010_160_465,
                    min: 23_465,
                    max: 29_097,
                },
            ],
        }
    }

    #[test]
    fn primary_drop_selects_tech_module_and_caps_at_cargo_hold() {
        let mut names = HashMap::new();
        names.insert(103_783_761, "Tech Module".to_string());
        names.insert(2_010_160_465, "Xindi Scraps".to_string());

        let hostile = yoton_cruiser_stub();
        let (rid, rname, primary, other) =
            select_hostile_farming_drops(&hostile, &names).expect("drops");
        assert_eq!(rid, 103_783_761);
        assert_eq!(rname, "Tech Module");
        assert!((primary - 1_108_523.5).abs() < 1e-6);
        assert!((other - 26_281.0).abs() < 1e-6);

        // Large cargo hold (not full): Archer (+300% -> 4x) earns 4x Kang's Tech Modules over 4 kills.
        let kang = CrewLootProfile {
            resource_id: rid,
            resource_name: rname.clone(),
            primary_base_per_kill: primary,
            other_base_per_kill: other,
            primary_loot_multiplier: 1.0,
            other_loot_multiplier: 1.0,
            effective_cargo_capacity: Some(25_000_000.0),
            loot_sources: vec![],
        };
        let archer = CrewLootProfile {
            primary_loot_multiplier: 4.0,
            other_loot_multiplier: 4.0,
            ..kang.clone()
        };
        assert!(!kang.is_cargo_capped_at_target(4));
        assert!(!archer.is_cargo_capped_at_target(4));
        assert!(
            archer.primary_loot_for_kills(4) > kang.primary_loot_for_kills(4) * 3.99,
            "uncapped Archer must earn 4x Kang Tech Modules"
        );

        // Tight cargo hold (4,000,000): BOTH crews fill the hold completely and tie on capped payout.
        let kang_tight = CrewLootProfile {
            effective_cargo_capacity: Some(4_000_000.0),
            ..kang
        };
        let archer_tight = CrewLootProfile {
            effective_cargo_capacity: Some(4_000_000.0),
            ..archer
        };
        assert!(kang_tight.is_cargo_capped_at_target(4));
        assert!(archer_tight.is_cargo_capped_at_target(4));
        assert!(
            (archer_tight.primary_loot_for_kills(4) - kang_tight.primary_loot_for_kills(4)).abs()
                < 1e-6,
            "once hold is full for both crews, extra loot multiplier must not exceed hold capacity"
        );
    }
}
