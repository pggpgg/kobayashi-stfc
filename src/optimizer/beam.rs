//! Slot-by-slot Diversity Beam Search over full legal officer pools (roadmap §2.1).
//!
//! Unlike the enumerative generator (which narrows large pools to ~10 captains and ~12 bridge
//! officers before enumeration), diversity beam search scores **every** legal officer directly
//! from their compiled LCARS abilities folded against the scenario's static combat gates
//! ([`fold_static_condition`]), then expands 6 parallel tactical beams slot-by-slot:
//!
//! 0. `damage` — weapon/crit/isolytic/apex-shred/shots/proc offense
//! 1. `survivability` — mitigation/dodge/armor/shield-deflection/enemy-crit-reduction/regen
//! 2. `r1_burst` — CombatBegin / early-round burst, shield bypass, shots, flat crit/isolytic
//! 3. `sustain_chain` — hull/shield regeneration, prev-round repair, on-kill regen, ramp
//! 4. `anti_dodge_pierce` — piercing, accuracy, morale (+10% all piercing), enemy pierce debuff
//! 5. `captain_diversity` — distinct captain groups paired via native [`OfficerSynergyIdentity`]
//!
//! Beam search **only proposes** legal, constraint-compliant crews; Monte Carlo racing and
//! independent confirmation remain the sole judge of crew quality.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::combat::condition::{fold_static_condition, FoldedCondition};
use crate::combat::{
    AbilityClass, AbilityCondition, AbilityEffect, CombatContext, CrewSeat, EnemyType,
    TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::data::synergy::{captain_synergy_indices, OfficerSynergyIdentity};
use crate::lcars::{resolve_officer_ability, LcarsOfficer};
use crate::optimizer::constraints::{
    filter_candidates, normalize_officer_name, CrewSearchConstraints,
};
use crate::optimizer::crew_generator::{CrewCandidate, OfficerPools, BRIDGE_SLOTS};
use crate::optimizer::monte_carlo::crew_candidate_stable_hash;
use crate::optimizer::monte_carlo::scenario::SharedScenarioData;
use crate::optimizer::qd_archive::{captain_group_lookup_from_registry, resolve_captain_group};

/// Method-provenance label for crews proposed by diversity beam search.
pub const METHOD_BEAM_DIVERSITY: &str = "beam_diversity";

const NUM_BEAMS: usize = 6;
const BEAM_DAMAGE: usize = 0;
const BEAM_SURVIVABILITY: usize = 1;
const BEAM_R1_BURST: usize = 2;
const BEAM_SUSTAIN_CHAIN: usize = 3;
const BEAM_ANTI_DODGE_PIERCE: usize = 4;
const BEAM_CAPTAIN_DIVERSITY: usize = 5;

const STATE_MORALE: u8 = 1 << 0;
const STATE_BURNING: u8 = 1 << 1;
const STATE_HULL_BREACH: u8 = 1 << 2;
const STATE_ASSIMILATED: u8 = 1 << 3;

/// Parameters controlling diversity beam proposal generation.
#[derive(Debug, Clone)]
pub struct BeamSearchParams<'a> {
    /// Target number of distinct crews to propose across all 6 beams.
    pub target_count: usize,
    pub below_decks_slots: usize,
    pub below_decks_pool_mode: BelowDecksPoolMode,
    pub enemy_type: EnemyType,
    pub profile_id: Option<&'a str>,
    pub constraints: Option<&'a CrewSearchConstraints>,
}

#[derive(Debug, Clone)]
struct RoleProfile {
    name: String,
    norm: String,
    group_slug: String,
    synergy: Option<OfficerSynergyIdentity>,
    /// Tactical scores along the 6 beam objectives.
    scores: [f64; NUM_BEAMS],
    /// Bridge ability effectiveness amplifier (e.g., Pike/Janeway captain maneuver).
    bridge_effectiveness_boost: f64,
    /// Captain maneuver amplifier.
    captain_maneuver_boost: f64,
    /// Combat states this officer can inflict/trigger in this role.
    provides_state: u8,
    /// Combat states this officer's conditional abilities reward in this role.
    needs_state: u8,
    /// Curated below-decks priority bonus (only used for below-decks ranking).
    curated_bd_bonus: f64,
    /// Raw LCARS Attack+Defense+Health tiebreaker.
    officer_power: f64,
}

pub(crate) fn static_context_from_shared(shared: &SharedScenarioData) -> CombatContext {
    let defender_faction = shared.defender_faction_for_combat();
    let defender_hull_faction_id = shared
        .hostile_rec
        .as_ref()
        .and_then(|h| h.faction.as_ref().map(|f| f.id))
        .unwrap_or(0);
    CombatContext {
        round_index: 1,
        defender_hull_pct: 1.0,
        defender_shield_pct: 1.0,
        attacker_hull_pct: 1.0,
        attacker_hull_pct_of_fight_start: None,
        attacker_shield_pct: 1.0,
        attacker_morale_active: false,
        defender_morale_active: false,
        defender_burning_active: false,
        defender_hull_breach_active: false,
        attacker_burning_active: false,
        attacker_hull_breach_active: false,
        defender_assimilated_active: false,
        defender_faction,
        attacker_owner_faction: shared.attacker_owner_faction,
        defender_hull_faction_id,
        defender_ship_type: shared.defender_ship_type_for_combat(),
        attacker_ship_type: shared.attacker_ship_type_for_combat(),
        attacker_ship_id: Arc::from(shared.ship.as_str()),
        defender_is_npc_hostile: shared.defender_opponent.defender_is_npc_hostile(),
        defender_is_player_ship: shared.defender_opponent.defender_is_player_ship(),
        attacker_tal_assigned_captain_or_bridge: false,
        defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
        engagement_enemy_types: Arc::new(shared.engagement_enemy_types.clone()),
        combat_battle_type_id: shared.combat_context.combat_battle_type_id,
        defender_level: shared.defender_level,
        combat_game_context: shared.combat_context.combat_game_context,
        attacker_is_defending: shared.combat_context.attacker_is_defending,
        attacker_at_station: shared.combat_context.attacker_at_station,
    }
}

fn collect_needed_states(cond: &AbilityCondition) -> u8 {
    match cond {
        AbilityCondition::MoraleActive => STATE_MORALE,
        AbilityCondition::DefenderBurning => STATE_BURNING,
        AbilityCondition::DefenderHullBreach => STATE_HULL_BREACH,
        AbilityCondition::DefenderAssimilated => STATE_ASSIMILATED,
        AbilityCondition::And(parts) | AbilityCondition::Or(parts) => parts
            .iter()
            .fold(0u8, |acc, c| acc | collect_needed_states(c)),
        AbilityCondition::Not(inner) => collect_needed_states(inner),
        _ => 0,
    }
}

/// Saturating magnitude helper so huge raw stat numbers (e.g. flat Armor +5000) and fractional
/// multipliers (e.g. +0.50 weapon damage) map onto comparable dimensionless tactical units.
#[inline]
fn soft_mag(v: f64, scale: f64) -> f64 {
    if !v.is_finite() || v <= 0.0 {
        return 0.0;
    }
    (v / scale).min(4.0)
}

#[allow(clippy::too_many_arguments)]
fn score_single_effect(
    effect: AbilityEffect,
    timing: TimingWindow,
    weight: f64,
    scores: &mut [f64; NUM_BEAMS],
    provides_state: &mut u8,
    bridge_boost: &mut f64,
    captain_boost: &mut f64,
) {
    let early_mult = match timing {
        TimingWindow::CombatBegin => 1.35,
        TimingWindow::RoundStart | TimingWindow::AttackPhase | TimingWindow::DefensePhase => 1.0,
        _ => 0.75,
    };
    let w = weight;

    match effect {
        AbilityEffect::AttackMultiplier(v)
        | AbilityEffect::BaseWeaponDamageBonus(v)
        | AbilityEffect::ComponentBaseWeaponDamageBonus(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_DAMAGE] += w * m * 2.2;
            scores[BEAM_R1_BURST] += w * m * 2.0 * early_mult;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.4;
        }
        AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
            growth_per_round,
            ceiling,
        } => {
            let m = soft_mag(growth_per_round * 5.0 + ceiling, 0.50);
            scores[BEAM_DAMAGE] += w * m * 2.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
        }
        AbilityEffect::CritChanceBonus(v) => {
            let m = soft_mag(v, 0.15);
            scores[BEAM_DAMAGE] += w * m * 2.1;
            scores[BEAM_R1_BURST] += w * m * 2.1 * early_mult;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.2;
        }
        AbilityEffect::CritDamageMultiplier(v) => {
            let bonus = if v > 1.0 { v - 1.0 } else { v };
            let m = soft_mag(bonus, 0.30);
            scores[BEAM_DAMAGE] += w * m * 2.0;
            scores[BEAM_R1_BURST] += w * m * 1.8 * early_mult;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.1;
        }
        AbilityEffect::BaseCritDamageBonus(v) => {
            let m = soft_mag(v, 0.30);
            scores[BEAM_DAMAGE] += w * m * 1.9;
            scores[BEAM_R1_BURST] += w * m * 1.7 * early_mult;
        }
        AbilityEffect::OnHitCritDamageStack { bonus, .. } => {
            let m = soft_mag(bonus, 0.10);
            scores[BEAM_DAMAGE] += w * m * 2.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.2;
        }
        AbilityEffect::OnHitTimedBonus {
            crit_chance,
            cascade_damage,
            ..
        } => {
            let m = soft_mag(crit_chance, 0.10) + soft_mag(cascade_damage, 0.08);
            scores[BEAM_DAMAGE] += w * m * 2.0;
            scores[BEAM_R1_BURST] += w * m * 1.5;
        }
        AbilityEffect::IsolyticDamageBonus(v) | AbilityEffect::IsolyticCascadeDamageBonus(v) => {
            let m = soft_mag(v, 0.15);
            scores[BEAM_DAMAGE] += w * m * 2.6;
            scores[BEAM_R1_BURST] += w * m * 2.3 * early_mult;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.8;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.6;
        }
        AbilityEffect::IsolyticDefenseBonus(v) => {
            let m = soft_mag(v, 0.15);
            scores[BEAM_SURVIVABILITY] += w * m * 2.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
        }
        AbilityEffect::ApexShredBonus(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_DAMAGE] += w * m * 2.3;
            scores[BEAM_R1_BURST] += w * m * 2.0 * early_mult;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.4;
        }
        AbilityEffect::ApexBarrierBonus(v) => {
            let m = soft_mag(v, 1000.0);
            scores[BEAM_SURVIVABILITY] += w * m * 2.8;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.2;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.5;
        }
        AbilityEffect::ShotsBonus {
            chance, bonus_pct, ..
        } => {
            let m = soft_mag(chance.clamp(0.0, 1.0) * bonus_pct, 0.20);
            scores[BEAM_DAMAGE] += w * m * 2.5;
            scores[BEAM_R1_BURST] += w * m * 2.6 * early_mult;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.5;
        }
        AbilityEffect::ProcAttackMultiplier { chance, multiplier } => {
            let bonus = (multiplier - 1.0).max(0.0) * chance.clamp(0.0, 1.0);
            let m = soft_mag(bonus, 0.20);
            scores[BEAM_DAMAGE] += w * m * 2.0;
            scores[BEAM_R1_BURST] += w * m * 1.8 * early_mult;
        }
        AbilityEffect::ProcPierceBonus { chance, bonus } => {
            let m = soft_mag(bonus * chance.clamp(0.0, 1.0), 0.15);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.2;
            scores[BEAM_DAMAGE] += w * m * 1.1;
        }
        AbilityEffect::AccumulatingAttackMultiplier {
            growth_per_round,
            ceiling,
            ..
        } => {
            let m = soft_mag(growth_per_round * 5.0 + (ceiling - 1.0).max(0.0), 0.50);
            scores[BEAM_DAMAGE] += w * m * 1.8;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
        }
        AbilityEffect::DecayingAttackMultiplier { initial, .. } => {
            let m = soft_mag((initial - 1.0).max(0.0), 0.30);
            scores[BEAM_R1_BURST] += w * m * 2.4;
            scores[BEAM_DAMAGE] += w * m * 1.5;
        }
        AbilityEffect::Morale(chance) => {
            *provides_state |= STATE_MORALE;
            let c = chance.clamp(0.0, 1.0);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * c * 2.4;
            scores[BEAM_DAMAGE] += w * c * 1.6;
            scores[BEAM_R1_BURST] += w * c * 1.6;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * c * 1.5;
        }
        AbilityEffect::HullBreach { chance, .. } => {
            *provides_state |= STATE_HULL_BREACH;
            let c = chance.clamp(0.0, 1.0);
            scores[BEAM_DAMAGE] += w * c * 2.4;
            scores[BEAM_R1_BURST] += w * c * 1.9;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * c * 1.4;
        }
        AbilityEffect::Burning { chance, .. } => {
            *provides_state |= STATE_BURNING;
            let c = chance.clamp(0.0, 1.0);
            scores[BEAM_DAMAGE] += w * c * 1.8;
            scores[BEAM_SUSTAIN_CHAIN] += w * c * 1.7;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * c * 1.2;
        }
        AbilityEffect::Assimilated { chance, .. } => {
            *provides_state |= STATE_ASSIMILATED;
            let c = chance.clamp(0.0, 1.0);
            scores[BEAM_SURVIVABILITY] += w * c * 1.5;
            scores[BEAM_DAMAGE] += w * c * 1.2;
        }
        AbilityEffect::MitigationAdditive(v)
        | AbilityEffect::HostileEngagementDefensiveBonus(v) => {
            let m = soft_mag(v, 0.15);
            scores[BEAM_SURVIVABILITY] += w * m * 3.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.4;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.6;
        }
        AbilityEffect::BaseMitigationStatsBonus(v)
        | AbilityEffect::CumulativeBaseMitigationStatsBonus(v)
        | AbilityEffect::BaseArmorBonus(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_SURVIVABILITY] += w * m * 2.4;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
        }
        AbilityEffect::BaseMitigationStatsAdd(v) => {
            let m = soft_mag(v, 500.0);
            scores[BEAM_SURVIVABILITY] += w * m * 2.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.4;
        }
        AbilityEffect::DodgeBonus(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_SURVIVABILITY] += w * m * 2.1;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
        }
        AbilityEffect::HostileCritDamageReduction { reduction, .. } => {
            let m = soft_mag(reduction, 0.20);
            scores[BEAM_SURVIVABILITY] += w * m * 2.7;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.2;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.4;
        }
        AbilityEffect::HostileCritChanceReduction(v)
        | AbilityEffect::TimedHostileCritChanceReduction { reduction: v, .. } => {
            let m = soft_mag(v, 0.15);
            scores[BEAM_SURVIVABILITY] += w * m * 2.2;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.6;
        }
        AbilityEffect::OpponentWeaponDamageReduction { reduction, .. } => {
            let m = soft_mag(reduction, 0.20);
            scores[BEAM_SURVIVABILITY] += w * m * 3.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.5;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.4;
        }
        AbilityEffect::HostileBasePiercingStatsReduction(v)
        | AbilityEffect::HostileBaseArmorPiercingReduction(v)
        | AbilityEffect::HostileCounterStatDebuff { reduction: v, .. } => {
            let m = soft_mag(v, 0.20);
            scores[BEAM_SURVIVABILITY] += w * m * 2.6;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.2;
        }
        AbilityEffect::HostileBaseDefenseStatsReduction(v)
        | AbilityEffect::BasePiercingStatsBonus(v)
        | AbilityEffect::BaseArmorPiercingBonus(v)
        | AbilityEffect::OnHitBaseArmorPiercingBonus(v)
        | AbilityEffect::CumulativeBasePiercingStatsBonus(v) => {
            let m = soft_mag(v, 0.35);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.7;
            scores[BEAM_DAMAGE] += w * m * 1.6;
            scores[BEAM_R1_BURST] += w * m * 1.4 * early_mult;
        }
        AbilityEffect::PierceBonus(v) | AbilityEffect::AccuracyBonus(v) => {
            let m = soft_mag(v, 0.20);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.8;
            scores[BEAM_DAMAGE] += w * m * 1.5;
            scores[BEAM_R1_BURST] += w * m * 1.5 * early_mult;
        }
        AbilityEffect::BaseArmorPiercingAdd(v) => {
            let m = soft_mag(v, 500.0);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.2;
            scores[BEAM_DAMAGE] += w * m * 1.2;
        }
        AbilityEffect::ShieldMitigationBypassFraction(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_R1_BURST] += w * m * 2.6 * early_mult;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.2;
            scores[BEAM_DAMAGE] += w * m * 1.7;
        }
        AbilityEffect::ShieldRegenMaxFraction(f)
        | AbilityEffect::HullRegenMaxFraction(f)
        | AbilityEffect::HullRegenPrevRoundFraction(f)
        | AbilityEffect::ShieldRegenPrevRoundFraction(f)
        | AbilityEffect::OnKillHullRegen(f) => {
            let m = soft_mag(f, 0.10);
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 3.0;
            scores[BEAM_SURVIVABILITY] += w * m * 2.4;
            scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.3;
        }
        AbilityEffect::ShieldRegen(v) | AbilityEffect::HullRegen(v) => {
            let m = soft_mag(v, 10_000.0);
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
            scores[BEAM_SURVIVABILITY] += w * m * 1.5;
        }
        AbilityEffect::HullHpMultiplier(v) | AbilityEffect::ShieldHpMultiplier(v) => {
            let m = soft_mag(v, 0.25);
            scores[BEAM_SURVIVABILITY] += w * m * 2.2;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
        }
        AbilityEffect::IncomingShotDeflection(shots) => {
            let m = (shots as f64).min(4.0);
            scores[BEAM_SURVIVABILITY] += w * m * 2.6;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
        }
        AbilityEffect::DefenderFireDelay {
            chance,
            delay_rounds,
            ..
        } => {
            let m = chance.clamp(0.0, 1.0) * (delay_rounds.max(1) as f64).min(3.0);
            scores[BEAM_SURVIVABILITY] += w * m * 2.4;
            scores[BEAM_R1_BURST] += w * m * 1.8;
        }
        AbilityEffect::BridgeAbilityEffectivenessBonus(v) => {
            if v.is_finite() && v > 0.0 {
                *bridge_boost += v * w;
                let m = soft_mag(v, 0.35);
                for s in scores.iter_mut() {
                    *s += w * m * 2.5;
                }
            }
        }
        AbilityEffect::CaptainManeuverMultiplier(v) => {
            let bonus = (v - 1.0).max(0.0);
            if bonus.is_finite() && bonus > 0.0 {
                *captain_boost += bonus * w;
                let m = soft_mag(bonus, 0.35);
                for s in scores.iter_mut() {
                    *s += w * m * 1.8;
                }
            }
        }
        AbilityEffect::DamageTakenCritStack {
            amount,
            reduce_opponent,
            ..
        } => {
            let m = soft_mag(amount, 0.20);
            if reduce_opponent {
                scores[BEAM_SURVIVABILITY] += w * m * 2.2;
                scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
            } else {
                scores[BEAM_DAMAGE] += w * m * 2.0;
                scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
            }
        }
        AbilityEffect::ReactiveDamageTaken {
            kind,
            chance,
            duration_rounds,
            ..
        } => {
            let p = chance.clamp(0.0, 1.0);
            let dur_boost = if duration_rounds == 0 { 1.4 } else { 1.0 };
            match kind {
                crate::combat::ReactiveDamageTakenKind::CritChanceBonus(v) => {
                    let m = soft_mag(v * p * dur_boost, 0.10);
                    scores[BEAM_DAMAGE] += w * m * 2.1;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.6;
                }
                crate::combat::ReactiveDamageTakenKind::BaseCritDamageBonus(v) => {
                    let m = soft_mag(v * p * dur_boost, 0.20);
                    scores[BEAM_DAMAGE] += w * m * 1.9;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.4;
                }
                crate::combat::ReactiveDamageTakenKind::BaseWeaponDamageBonus(v) => {
                    let m = soft_mag(v * p * dur_boost, 0.20);
                    scores[BEAM_DAMAGE] += w * m * 2.0;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
                }
                crate::combat::ReactiveDamageTakenKind::AttackerShieldMitigationBonus(v) => {
                    let m = soft_mag(v * p, 0.10);
                    scores[BEAM_SURVIVABILITY] += w * m * 2.2;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
                }
                crate::combat::ReactiveDamageTakenKind::OpponentWeaponDamageReduction(v) => {
                    let m = soft_mag(v * p, 0.20);
                    scores[BEAM_SURVIVABILITY] += w * m * 2.3;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
                }
                crate::combat::ReactiveDamageTakenKind::HostileCritChanceReduction(v) => {
                    let m = soft_mag(v * p * dur_boost, 0.15);
                    scores[BEAM_SURVIVABILITY] += w * m * 1.9;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
                }
                crate::combat::ReactiveDamageTakenKind::ApexBarrierFlatBonus(v) => {
                    let m = soft_mag(v * p * dur_boost, 2_500.0);
                    scores[BEAM_SURVIVABILITY] += w * m * 2.3;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
                }
                crate::combat::ReactiveDamageTakenKind::ApexBarrierMultiplierBonus(v) => {
                    let m = soft_mag(v * p, 0.50);
                    scores[BEAM_SURVIVABILITY] += w * m * 2.1;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
                }
                crate::combat::ReactiveDamageTakenKind::DrainOpponentShieldFlat(v) => {
                    let m = soft_mag(v * p, 50_000.0);
                    scores[BEAM_DAMAGE] += w * m * 1.5;
                    scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.6;
                }
                crate::combat::ReactiveDamageTakenKind::ApplyDefenderBurning => {
                    *provides_state |= STATE_BURNING;
                    let m = soft_mag(p, 0.35);
                    scores[BEAM_DAMAGE] += w * m * 1.4;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.3;
                }
                crate::combat::ReactiveDamageTakenKind::ApplyDefenderHullBreach => {
                    *provides_state |= STATE_HULL_BREACH;
                    let m = soft_mag(p, 0.35);
                    scores[BEAM_DAMAGE] += w * m * 1.6;
                    scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.2;
                }
                crate::combat::ReactiveDamageTakenKind::ApplyAttackerMorale => {
                    *provides_state |= STATE_MORALE;
                    let m = soft_mag(p, 0.35);
                    scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.5;
                    scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.2;
                }
                crate::combat::ReactiveDamageTakenKind::PreAllAllDefensesAdd(v) => {
                    let m = soft_mag(v * p, 5_000.0);
                    scores[BEAM_SURVIVABILITY] += w * m * 2.5;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
                }
                crate::combat::ReactiveDamageTakenKind::PreAllAllPiercingAdd(v) => {
                    let m = soft_mag(v * p, 5_000.0);
                    scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.5;
                    scores[BEAM_DAMAGE] += w * m * 1.6;
                }
            }
        }
        AbilityEffect::CrewStatScaledDeferred {
            target,
            coefficient,
            chance,
            ..
        } => {
            use crate::combat::CrewStatScaledTarget;
            let p = chance.clamp(0.0, 1.0);
            let m = soft_mag(coefficient * p, 2.0);
            match target {
                CrewStatScaledTarget::AllDefensesPreAll
                | CrewStatScaledTarget::BaseArmorPreAll
                | CrewStatScaledTarget::BaseShieldDeflectionPreAll
                | CrewStatScaledTarget::BaseDodgePreAll => {
                    scores[BEAM_SURVIVABILITY] += w * m * 2.6;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.1;
                    scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.2;
                }
                CrewStatScaledTarget::AllPiercingPreAll
                | CrewStatScaledTarget::BaseArmorPiercingPreAll
                | CrewStatScaledTarget::BaseShieldPiercingPreAll
                | CrewStatScaledTarget::BaseAccuracyPreAll => {
                    scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.6;
                    scores[BEAM_DAMAGE] += w * m * 1.6;
                    scores[BEAM_R1_BURST] += w * m * 1.4 * early_mult;
                }
                CrewStatScaledTarget::ShieldRepairFlat => {
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.4;
                    scores[BEAM_SURVIVABILITY] += w * m * 2.1;
                    scores[BEAM_CAPTAIN_DIVERSITY] += w * m * 1.2;
                }
                CrewStatScaledTarget::WeaponDamageFlatPreCrit => {
                    scores[BEAM_DAMAGE] += w * m * 2.4;
                    scores[BEAM_R1_BURST] += w * m * 2.0 * early_mult;
                    scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.5;
                }
                CrewStatScaledTarget::DefenderShieldDrainFlat => {
                    scores[BEAM_DAMAGE] += w * m * 1.8;
                    scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.8;
                    scores[BEAM_R1_BURST] += w * m * 1.4 * early_mult;
                }
            }
        }
        AbilityEffect::PreAllDefenseAdd {
            armor,
            shield_deflection,
            dodge,
            ..
        } => {
            let m = soft_mag(armor.max(shield_deflection).max(dodge), 5_000.0);
            scores[BEAM_SURVIVABILITY] += w * m * 2.6;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 2.0;
        }
        AbilityEffect::PreAllPiercingAdd {
            armor_piercing,
            shield_piercing,
            accuracy,
            ..
        }
        | AbilityEffect::OnHitPreAllPiercingAdd {
            armor_piercing,
            shield_piercing,
            accuracy,
        } => {
            let m = soft_mag(armor_piercing.max(shield_piercing).max(accuracy), 5_000.0);
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 2.6;
            scores[BEAM_DAMAGE] += w * m * 1.6;
        }
        AbilityEffect::FlatWeaponDamagePreCrit { amount, chance, .. } => {
            let m = soft_mag(amount * chance.clamp(0.0, 1.0), 50_000.0);
            scores[BEAM_DAMAGE] += w * m * 2.4;
            scores[BEAM_R1_BURST] += w * m * 2.0 * early_mult;
        }
        AbilityEffect::DefenderShieldDrainPerRound { fraction, .. } => {
            let m = soft_mag(fraction, 0.15);
            scores[BEAM_DAMAGE] += w * m * 1.6;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.8;
        }
        AbilityEffect::DefenderShieldDrainFlat(v)
        | AbilityEffect::OnHitDefenderShieldDrainFlat(v) => {
            let m = soft_mag(v, 50_000.0);
            scores[BEAM_DAMAGE] += w * m * 1.6;
            scores[BEAM_ANTI_DODGE_PIERCE] += w * m * 1.8;
        }
        AbilityEffect::AttackerShieldMitigationBonus(v) => {
            let m = soft_mag(v, 0.10);
            scores[BEAM_SURVIVABILITY] += w * m * 2.0;
            scores[BEAM_SUSTAIN_CHAIN] += w * m * 1.8;
        }
        _ => {
            // Any other statically-passing combat ability gets a modest positive baseline.
            scores[BEAM_CAPTAIN_DIVERSITY] += w * 0.25;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_role_profile(
    name: &str,
    seat_role: CrewSeat,
    lcars_by_norm: &HashMap<String, &LcarsOfficer>,
    shared: &SharedScenarioData,
    static_ctx: &CombatContext,
    group_lookup: &HashMap<String, String>,
    curated_bd_rank: &HashMap<String, u32>,
) -> RoleProfile {
    let norm = normalize_officer_name(name);
    let group_slug = resolve_captain_group(name, group_lookup);
    let mut scores = [0.0_f64; NUM_BEAMS];
    let mut provides_state = 0u8;
    let mut needs_state = 0u8;
    let mut bridge_boost = 0.0_f64;
    let mut captain_boost = 0.0_f64;
    let mut synergy = None;
    let mut officer_power = 0.0_f64;

    if let Some(officer) = lcars_by_norm.get(&norm) {
        synergy = officer.native_synergy.as_ref().map(|ns| ns.identity);
        let level = officer.resolve_level(None, None).unwrap_or(1);
        officer_power = officer
            .stats_at_level(level)
            .map(|s| s.attack + s.defense + s.health)
            .unwrap_or(0.0);

        let mut abilities_to_eval: Vec<(&crate::lcars::LcarsAbility, CrewSeat, AbilityClass)> =
            Vec::new();
        match seat_role {
            CrewSeat::Captain => {
                if let Some(ref cap) = officer.captain_ability {
                    abilities_to_eval.push((cap, CrewSeat::Captain, AbilityClass::CaptainManeuver));
                }
                if let Some(ref br) = officer.bridge_ability {
                    abilities_to_eval.push((br, CrewSeat::Bridge, AbilityClass::BridgeAbility));
                }
            }
            CrewSeat::Bridge => {
                if let Some(ref br) = officer.bridge_ability {
                    abilities_to_eval.push((br, CrewSeat::Bridge, AbilityClass::BridgeAbility));
                }
            }
            CrewSeat::BelowDeck => {
                if let Some(ref bd) = officer.below_decks_ability {
                    abilities_to_eval.push((bd, CrewSeat::BelowDeck, AbilityClass::BelowDeck));
                }
            }
            CrewSeat::Ship => {}
        }

        for (ability, seat, class) in abilities_to_eval {
            let compiled = resolve_officer_ability(
                officer,
                ability,
                seat,
                class,
                &shared.resolve_options,
                NO_EXPLICIT_CONTRIBUTION_BATCH,
            );
            for seat_ctx in compiled {
                let cond_weight = match seat_ctx.ability.condition.as_ref() {
                    None => 1.0,
                    Some(cond) => match fold_static_condition(cond, static_ctx) {
                        FoldedCondition::AlwaysTrue => 1.05,
                        FoldedCondition::AlwaysFalse => 0.0,
                        FoldedCondition::Dynamic(dyn_cond) => {
                            let needed = collect_needed_states(&dyn_cond);
                            needs_state |= needed;
                            if needed != 0 {
                                0.80
                            } else {
                                0.65
                            }
                        }
                    },
                };
                if cond_weight <= 0.0 {
                    continue;
                }
                score_single_effect(
                    seat_ctx.ability.effect,
                    seat_ctx.ability.timing,
                    cond_weight,
                    &mut scores,
                    &mut provides_state,
                    &mut bridge_boost,
                    &mut captain_boost,
                );
            }
        }
    }

    // Small power tiebreaker so officers with unmodeled or equal abilities rank by real stats.
    let power_tie = (officer_power / 50_000.0).min(0.40);
    for s in &mut scores {
        *s += power_tie;
    }

    let curated_bd_bonus = curated_bd_rank
        .get(&norm)
        .map(|&rank| 2.5 - (rank as f64 * 0.06).min(1.8))
        .unwrap_or(0.0);
    if seat_role == CrewSeat::BelowDeck {
        for s in &mut scores {
            *s += curated_bd_bonus;
        }
    }

    RoleProfile {
        name: name.to_string(),
        norm,
        group_slug,
        synergy,
        scores,
        bridge_effectiveness_boost: bridge_boost,
        captain_maneuver_boost: captain_boost,
        provides_state,
        needs_state,
        curated_bd_bonus,
        officer_power,
    }
}

/// Scenario-ranked officer lists per seat produced by the LCARS static-folded scorer.
/// Reused by portfolio's elite neighborhood expansion so 1-slot mutations explore high-value
/// officers first without recompiling LCARS profiles.
#[derive(Debug, Clone, Default)]
pub(crate) struct RankedOfficerSeats {
    pub captains: Vec<String>,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

/// Propose legal, constraint-compliant, tactically diverse crews using slot-by-slot beam search
/// over the full legal officer pools, also returning scenario-ranked per-seat officer lists.
#[cfg(test)]
pub(crate) fn propose_diversity_beam_crews_with_seats(
    registry: &DataRegistry,
    shared: &SharedScenarioData,
    params: &BeamSearchParams<'_>,
) -> (Vec<CrewCandidate>, RankedOfficerSeats) {
    if params.target_count == 0 {
        return (Vec::new(), RankedOfficerSeats::default());
    }
    let mut tags = shared.engagement_enemy_types.0.clone();
    if !tags.contains(&params.enemy_type) {
        tags.push(params.enemy_type);
    }
    let Some(pools) = crate::optimizer::crew_generator::build_officer_pools_from_registry_for_tags(
        registry,
        params.below_decks_pool_mode,
        Some(tags.as_slice()),
        params.profile_id,
        params.below_decks_slots,
        params.constraints,
    ) else {
        return (Vec::new(), RankedOfficerSeats::default());
    };
    let pools = crate::optimizer::crew_generator::filter_officer_pools_for_scenario(
        registry,
        shared,
        pools,
        params.below_decks_pool_mode,
        params.below_decks_slots,
        false,
        params.constraints,
    );
    propose_diversity_beam_crews_from_pools_with_seats(registry, shared, &pools, params)
}

pub(crate) fn propose_diversity_beam_crews_from_pools_with_seats(
    registry: &DataRegistry,
    shared: &SharedScenarioData,
    pools: &OfficerPools,
    params: &BeamSearchParams<'_>,
) -> (Vec<CrewCandidate>, RankedOfficerSeats) {
    if params.target_count == 0 || pools.captains.is_empty() {
        return (Vec::new(), RankedOfficerSeats::default());
    }
    if pools.bridge.len() < BRIDGE_SLOTS && pools.captains.len() + pools.bridge.len() < 3 {
        return (Vec::new(), RankedOfficerSeats::default());
    }

    let static_ctx = static_context_from_shared(shared);
    let group_lookup = captain_group_lookup_from_registry(registry);
    let curated_bd_rank: HashMap<String, u32> =
        crate::data::below_decks_priority::curated_below_decks_priority()
            .iter()
            .enumerate()
            .map(|(i, n)| (normalize_officer_name(n), i as u32))
            .collect();

    let mut lcars_by_norm: HashMap<String, &LcarsOfficer> = HashMap::new();
    if let Some(lcars) = registry.lcars_officers() {
        for officer in lcars {
            let key = normalize_officer_name(&officer.name);
            if !key.is_empty() {
                lcars_by_norm.insert(key, officer);
            }
        }
    }

    let bridge_norm_set: HashSet<String> = pools
        .bridge
        .iter()
        .map(|name| normalize_officer_name(name))
        .collect();
    let captain_profiles: Vec<RoleProfile> = pools
        .captains
        .iter()
        .filter(|name| bridge_norm_set.contains(&normalize_officer_name(name)))
        .map(|name| {
            build_role_profile(
                name,
                CrewSeat::Captain,
                &lcars_by_norm,
                shared,
                &static_ctx,
                &group_lookup,
                &curated_bd_rank,
            )
        })
        .collect();
    let bridge_profiles: Vec<RoleProfile> = pools
        .bridge
        .iter()
        .map(|name| {
            build_role_profile(
                name,
                CrewSeat::Bridge,
                &lcars_by_norm,
                shared,
                &static_ctx,
                &group_lookup,
                &curated_bd_rank,
            )
        })
        .collect();
    let below_profiles: Vec<RoleProfile> = pools
        .below_decks
        .iter()
        .map(|name| {
            build_role_profile(
                name,
                CrewSeat::BelowDeck,
                &lcars_by_norm,
                shared,
                &static_ctx,
                &group_lookup,
                &curated_bd_rank,
            )
        })
        .collect();

    if captain_profiles.is_empty() || bridge_profiles.len() < BRIDGE_SLOTS {
        return (Vec::new(), RankedOfficerSeats::default());
    }

    // Index bridge officers by native synergy group_id for O(1) synergy partner lookup.
    let mut bridge_by_synergy_group: HashMap<u64, Vec<usize>> = HashMap::new();
    for (idx, bp) in bridge_profiles.iter().enumerate() {
        if let Some(syn) = bp.synergy {
            bridge_by_synergy_group
                .entry(syn.group_id)
                .or_default()
                .push(idx);
        }
    }

    // Must-include officers from constraints (if any) so beam search actively seats them.
    let must_include_norms: HashSet<String> = params
        .constraints
        .map(|c| {
            c.must_include
                .iter()
                .map(|s| normalize_officer_name(s))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let per_beam_target = (params.target_count / NUM_BEAMS).max(8);
    let cap_width = (per_beam_target / 2)
        .clamp(10, 32)
        .min(captain_profiles.len());
    let bridge_core_width = per_beam_target.clamp(12, 96);
    let max_cores_per_captain = (bridge_core_width / 4).clamp(2, 6);

    // Pre-sort bridge and below-decks officer indices once per beam objective.
    let mut bridge_order_by_beam: [Vec<usize>; NUM_BEAMS] = std::array::from_fn(|beam| {
        let mut idxs: Vec<usize> = (0..bridge_profiles.len()).collect();
        idxs.sort_by(|&a, &b| {
            bridge_profiles[b].scores[beam]
                .total_cmp(&bridge_profiles[a].scores[beam])
                .then_with(|| {
                    bridge_profiles[b]
                        .officer_power
                        .total_cmp(&bridge_profiles[a].officer_power)
                })
                .then_with(|| bridge_profiles[a].name.cmp(&bridge_profiles[b].name))
        });
        idxs
    });
    // Ensure beam 5 (captain_diversity) orders bridge officers by balanced damage+survivability.
    bridge_order_by_beam[BEAM_CAPTAIN_DIVERSITY].sort_by(|&a, &b| {
        let sa =
            bridge_profiles[a].scores[BEAM_DAMAGE] + bridge_profiles[a].scores[BEAM_SURVIVABILITY];
        let sb =
            bridge_profiles[b].scores[BEAM_DAMAGE] + bridge_profiles[b].scores[BEAM_SURVIVABILITY];
        sb.total_cmp(&sa)
            .then_with(|| bridge_profiles[a].name.cmp(&bridge_profiles[b].name))
    });

    let below_order_by_beam: [Vec<usize>; NUM_BEAMS] = std::array::from_fn(|beam| {
        let mut idxs: Vec<usize> = (0..below_profiles.len()).collect();
        idxs.sort_by(|&a, &b| {
            below_profiles[b].scores[beam]
                .total_cmp(&below_profiles[a].scores[beam])
                .then_with(|| {
                    below_profiles[b]
                        .curated_bd_bonus
                        .total_cmp(&below_profiles[a].curated_bd_bonus)
                })
                .then_with(|| {
                    below_profiles[b]
                        .officer_power
                        .total_cmp(&below_profiles[a].officer_power)
                })
                .then_with(|| below_profiles[a].name.cmp(&below_profiles[b].name))
        });
        idxs
    });

    // Global bridge staples (top 6 damage + top 6 survivability) so cross-role synergies always pair.
    let mut universal_bridge_staples: Vec<usize> = Vec::new();
    for &idx in bridge_order_by_beam[BEAM_DAMAGE]
        .iter()
        .take(6)
        .chain(bridge_order_by_beam[BEAM_SURVIVABILITY].iter().take(6))
        .chain(bridge_order_by_beam[BEAM_ANTI_DODGE_PIERCE].iter().take(4))
    {
        if !universal_bridge_staples.contains(&idx) {
            universal_bridge_staples.push(idx);
        }
    }

    let mut beam_outputs: [Vec<CrewCandidate>; NUM_BEAMS] = Default::default();

    for beam in 0..NUM_BEAMS {
        // Step 1: Select beam captains.
        let mut cap_order: Vec<usize> = (0..captain_profiles.len()).collect();
        cap_order.sort_by(|&a, &b| {
            let sa = if beam == BEAM_CAPTAIN_DIVERSITY {
                captain_profiles[a].scores[BEAM_DAMAGE]
                    + captain_profiles[a].scores[BEAM_SURVIVABILITY]
                    + captain_profiles[a].scores[BEAM_CAPTAIN_DIVERSITY]
            } else {
                captain_profiles[a].scores[beam]
            };
            let sb = if beam == BEAM_CAPTAIN_DIVERSITY {
                captain_profiles[b].scores[BEAM_DAMAGE]
                    + captain_profiles[b].scores[BEAM_SURVIVABILITY]
                    + captain_profiles[b].scores[BEAM_CAPTAIN_DIVERSITY]
            } else {
                captain_profiles[b].scores[beam]
            };
            sb.total_cmp(&sa)
                .then_with(|| {
                    captain_profiles[b]
                        .officer_power
                        .total_cmp(&captain_profiles[a].officer_power)
                })
                .then_with(|| captain_profiles[a].name.cmp(&captain_profiles[b].name))
        });

        let selected_captains: Vec<usize> = if beam == BEAM_CAPTAIN_DIVERSITY {
            // Pick at most 1 captain per distinct group first to maximize captain-family coverage.
            let mut chosen = Vec::with_capacity(cap_width);
            let mut seen_groups: HashSet<&str> = HashSet::new();
            for &cidx in &cap_order {
                if chosen.len() >= cap_width {
                    break;
                }
                if seen_groups.insert(captain_profiles[cidx].group_slug.as_str()) {
                    chosen.push(cidx);
                }
            }
            for &cidx in &cap_order {
                if chosen.len() >= cap_width {
                    break;
                }
                if !chosen.contains(&cidx) {
                    chosen.push(cidx);
                }
            }
            chosen
        } else {
            // Allow at most 2 captains per group in specialist beams so one family doesn't crowd out others.
            let mut chosen = Vec::with_capacity(cap_width);
            let mut group_counts: HashMap<&str, usize> = HashMap::new();
            for &cidx in &cap_order {
                if chosen.len() >= cap_width {
                    break;
                }
                let cnt = group_counts
                    .entry(captain_profiles[cidx].group_slug.as_str())
                    .or_insert(0);
                if *cnt < 2 {
                    *cnt += 1;
                    chosen.push(cidx);
                }
            }
            for &cidx in &cap_order {
                if chosen.len() >= cap_width {
                    break;
                }
                if !chosen.contains(&cidx) {
                    chosen.push(cidx);
                }
            }
            chosen
        };

        // Step 2: Expand (captain, bridge1, bridge2) cores.
        // Track `(per_cap_rank, score, cidx, b1_idx, b2_idx)` so every selected captain gets its
        // strongest bridge core first before any single captain claims 2nd/3rd/4th slots.
        let mut scored_cores: Vec<(usize, f64, usize, usize, usize)> = Vec::new();
        for &cidx in &selected_captains {
            let cap = &captain_profiles[cidx];
            let mut shortlist: Vec<usize> = Vec::with_capacity(28);
            let push_br = |bidx: usize, list: &mut Vec<usize>| {
                if bridge_profiles[bidx].norm != cap.norm && !list.contains(&bidx) {
                    list.push(bidx);
                }
            };

            // (a) Must-include bridge officers first.
            if !must_include_norms.is_empty() {
                for (bidx, bp) in bridge_profiles.iter().enumerate() {
                    if must_include_norms.contains(&bp.norm) {
                        push_br(bidx, &mut shortlist);
                    }
                }
            }
            // (b) Native synergy partners of this captain.
            if let Some(syn) = cap.synergy {
                if let Some(partners) = bridge_by_synergy_group.get(&syn.group_id) {
                    let mut sorted_partners = partners.clone();
                    sorted_partners.sort_by(|&a, &b| {
                        bridge_profiles[b].scores[beam]
                            .total_cmp(&bridge_profiles[a].scores[beam])
                            .then_with(|| bridge_profiles[a].name.cmp(&bridge_profiles[b].name))
                    });
                    for &pidx in sorted_partners.iter().take(6) {
                        push_br(pidx, &mut shortlist);
                    }
                }
            }
            // (c) Top bridge specialists for this beam.
            for &bidx in bridge_order_by_beam[beam].iter().take(12) {
                push_br(bidx, &mut shortlist);
            }
            // (d) Universal staples (cross-pollination of damage + survivability + pierce).
            for &bidx in &universal_bridge_staples {
                push_br(bidx, &mut shortlist);
            }
            // (e) State enablers/payoffs matching this captain's state bits.
            if cap.needs_state != 0 || cap.provides_state != 0 {
                let mut added_state = 0usize;
                for &bidx in &bridge_order_by_beam[beam] {
                    if added_state >= 4 {
                        break;
                    }
                    let bp = &bridge_profiles[bidx];
                    if (bp.provides_state & cap.needs_state) != 0
                        || (bp.needs_state & cap.provides_state) != 0
                    {
                        let before = shortlist.len();
                        push_br(bidx, &mut shortlist);
                        if shortlist.len() > before {
                            added_state += 1;
                        }
                    }
                }
            }

            shortlist.truncate(22);
            if shortlist.len() < BRIDGE_SLOTS {
                continue;
            }

            let bridge_mult = 1.0 + cap.bridge_effectiveness_boost.clamp(0.0, 2.0);
            let cap_base = if beam == BEAM_CAPTAIN_DIVERSITY {
                cap.scores[BEAM_DAMAGE] + cap.scores[BEAM_SURVIVABILITY]
            } else {
                cap.scores[beam]
            };

            let mut per_cap_pairs: Vec<(f64, usize, usize)> = Vec::new();
            for i in 0..shortlist.len() {
                let b1_idx = shortlist[i];
                let b1 = &bridge_profiles[b1_idx];
                for &b2_idx in &shortlist[(i + 1)..] {
                    let b2 = &bridge_profiles[b2_idx];
                    if b1.norm == b2.norm {
                        continue;
                    }
                    let syn_bonus = match cap.synergy {
                        Some(cap_syn) => {
                            let idx = captain_synergy_indices(cap_syn, [b1.synergy, b2.synergy]);
                            (idx[0] + idx[1]) as f64 / 4.0
                        }
                        None => 0.0,
                    };
                    let cap_amp = 1.0
                        + 0.40 * syn_bonus
                        + (b1.captain_maneuver_boost + b2.captain_maneuver_boost).clamp(0.0, 1.5)
                            * 0.35;
                    let b1_s = if beam == BEAM_CAPTAIN_DIVERSITY {
                        b1.scores[BEAM_DAMAGE] + b1.scores[BEAM_SURVIVABILITY]
                    } else {
                        b1.scores[beam]
                    };
                    let b2_s = if beam == BEAM_CAPTAIN_DIVERSITY {
                        b2.scores[BEAM_DAMAGE] + b2.scores[BEAM_SURVIVABILITY]
                    } else {
                        b2.scores[beam]
                    };

                    // State enabler <-> payoff combo bonus across the 3-officer bridge.
                    let provided = cap.provides_state | b1.provides_state | b2.provides_state;
                    let needed = cap.needs_state | b1.needs_state | b2.needs_state;
                    let state_matches = (provided & needed).count_ones() as f64;
                    let state_bonus = state_matches * 1.15;

                    let must_bonus = (usize::from(must_include_norms.contains(&b1.norm))
                        + usize::from(must_include_norms.contains(&b2.norm)))
                        as f64
                        * 4.0;

                    let joint =
                        cap_base * cap_amp + (b1_s + b2_s) * bridge_mult + state_bonus + must_bonus;
                    per_cap_pairs.push((joint, b1_idx, b2_idx));
                }
            }

            per_cap_pairs.sort_by(|a, b| {
                b.0.total_cmp(&a.0)
                    .then_with(|| bridge_profiles[a.1].name.cmp(&bridge_profiles[b.1].name))
                    .then_with(|| bridge_profiles[a.2].name.cmp(&bridge_profiles[b.2].name))
            });
            for (per_cap_rank, (score, b1_idx, b2_idx)) in per_cap_pairs
                .into_iter()
                .take(max_cores_per_captain)
                .enumerate()
            {
                scored_cores.push((per_cap_rank, score, cidx, b1_idx, b2_idx));
            }
        }

        scored_cores.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.total_cmp(&a.1))
                .then_with(|| captain_profiles[a.2].name.cmp(&captain_profiles[b.2].name))
                .then_with(|| bridge_profiles[a.3].name.cmp(&bridge_profiles[b.3].name))
                .then_with(|| bridge_profiles[a.4].name.cmp(&bridge_profiles[b.4].name))
        });
        scored_cores.truncate(bridge_core_width);

        // Step 3: Fill below-decks slots for each bridge core (primary + optional shifted variant).
        let bd_order = &below_order_by_beam[beam];
        let mut beam_crews: Vec<CrewCandidate> = Vec::with_capacity(scored_cores.len() * 2);
        for (core_rank, &(_, _, cidx, b1_idx, b2_idx)) in scored_cores.iter().enumerate() {
            let cap_name = &captain_profiles[cidx].name;
            let b1_name = &bridge_profiles[b1_idx].name;
            let b2_name = &bridge_profiles[b2_idx].name;
            let cap_norm = &captain_profiles[cidx].norm;
            let b1_norm = &bridge_profiles[b1_idx].norm;
            let b2_norm = &bridge_profiles[b2_idx].norm;

            // Variant 0: best below-decks fill for this beam (honoring must_include first).
            // Variant 1 (for top half of cores): offset by 1–2 slots so racing explores alternative BD combos.
            let num_variants = if params.below_decks_slots > 0 && core_rank < bridge_core_width / 2
            {
                2
            } else {
                1
            };

            for variant in 0..num_variants {
                let mut used: HashSet<&str> = HashSet::with_capacity(3 + params.below_decks_slots);
                used.insert(cap_norm.as_str());
                used.insert(b1_norm.as_str());
                used.insert(b2_norm.as_str());

                let mut bd: Vec<String> = Vec::with_capacity(params.below_decks_slots);
                if !must_include_norms.is_empty() {
                    for &didx in bd_order {
                        if bd.len() >= params.below_decks_slots {
                            break;
                        }
                        let dp = &below_profiles[didx];
                        if must_include_norms.contains(&dp.norm) && used.insert(dp.norm.as_str()) {
                            bd.push(dp.name.clone());
                        }
                    }
                }
                let skip = if variant == 0 {
                    0
                } else {
                    ((core_rank % 3) + 1)
                        .min(bd_order.len().saturating_sub(params.below_decks_slots))
                };
                for &didx in bd_order.iter().skip(skip).chain(bd_order.iter().take(skip)) {
                    if bd.len() >= params.below_decks_slots {
                        break;
                    }
                    let dp = &below_profiles[didx];
                    if used.insert(dp.norm.as_str()) {
                        bd.push(dp.name.clone());
                    }
                }
                if bd.len() == params.below_decks_slots {
                    beam_crews.push(CrewCandidate {
                        captain: cap_name.clone(),
                        bridge: vec![b1_name.clone(), b2_name.clone()],
                        below_decks: bd,
                    });
                }
            }
        }

        // Enforce explicit constraints per beam before interleaving.
        beam_outputs[beam] = match params.constraints {
            Some(c) if !c.is_empty() => filter_candidates(beam_crews, c),
            _ => beam_crews,
        };
    }

    // Round-robin interleave across the 6 beams + deduplicate by canonical stable hash.
    let mut out: Vec<CrewCandidate> = Vec::with_capacity(params.target_count);
    let mut seen: HashSet<u64> = HashSet::with_capacity(params.target_count * 2);
    let mut cursors = [0usize; NUM_BEAMS];

    while out.len() < params.target_count {
        let mut progressed = false;
        for beam in 0..NUM_BEAMS {
            if out.len() >= params.target_count {
                break;
            }
            while cursors[beam] < beam_outputs[beam].len() {
                let candidate = beam_outputs[beam][cursors[beam]].clone();
                cursors[beam] += 1;
                if seen.insert(crew_candidate_stable_hash(&candidate)) {
                    out.push(candidate);
                    progressed = true;
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
    }

    // Build scenario-ranked officer lists per seat (best across all specialist tactical axes).
    let rank_profiles = |profiles: &[RoleProfile], include_curated: bool| -> Vec<String> {
        let mut idxs: Vec<usize> = (0..profiles.len()).collect();
        let composite = |p: &RoleProfile| -> f64 {
            let best_axis = p.scores[..5].iter().copied().fold(0.0_f64, f64::max);
            let sum_axes: f64 = p.scores[..5].iter().sum();
            best_axis
                + 0.15 * sum_axes
                + if include_curated {
                    p.curated_bd_bonus
                } else {
                    0.0
                }
        };
        idxs.sort_by(|&a, &b| {
            composite(&profiles[b])
                .total_cmp(&composite(&profiles[a]))
                .then_with(|| {
                    profiles[b]
                        .officer_power
                        .total_cmp(&profiles[a].officer_power)
                })
                .then_with(|| profiles[a].name.cmp(&profiles[b].name))
        });
        idxs.into_iter().map(|i| profiles[i].name.clone()).collect()
    };
    let ranked_seats = RankedOfficerSeats {
        captains: rank_profiles(&captain_profiles, false),
        bridge: rank_profiles(&bridge_profiles, false),
        below_decks: rank_profiles(&below_profiles, true),
    };

    (out, ranked_seats)
}

/// Compact LCARS tactical summary for one officer in one specific seat role, folded against the
/// scenario's static combat gates. Reused by counterfactual explanations & substitute search.
#[derive(Debug, Clone)]
pub(crate) struct OfficerRoleSummary {
    pub provides_state: u8,
    pub needs_state: u8,
    pub bridge_effectiveness_boost: f64,
    pub captain_maneuver_boost: f64,
    pub synergy: Option<OfficerSynergyIdentity>,
}

/// Human-readable names for trigger-state bitmasks (`STATE_*`).
pub(crate) fn state_bit_labels(mask: u8) -> Vec<&'static str> {
    let mut out = Vec::new();
    if mask & STATE_MORALE != 0 {
        out.push("Morale");
    }
    if mask & STATE_BURNING != 0 {
        out.push("Burning");
    }
    if mask & STATE_HULL_BREACH != 0 {
        out.push("Hull Breach");
    }
    if mask & STATE_ASSIMILATED != 0 {
        out.push("Assimilated");
    }
    out
}

/// Score one officer in a specific seat role using the exact static-folded beam profiler.
pub(crate) fn summarize_officer_role(
    registry: &DataRegistry,
    shared: &SharedScenarioData,
    name: &str,
    seat_role: CrewSeat,
) -> OfficerRoleSummary {
    let static_ctx = static_context_from_shared(shared);
    let group_lookup = captain_group_lookup_from_registry(registry);
    let curated_bd_rank: HashMap<String, u32> = HashMap::new();
    let mut lcars_by_norm: HashMap<String, &LcarsOfficer> = HashMap::new();
    if let Some(lcars) = registry.lcars_officers() {
        for officer in lcars {
            let key = normalize_officer_name(&officer.name);
            if !key.is_empty() {
                lcars_by_norm.insert(key, officer);
            }
        }
    }
    let p = build_role_profile(
        name,
        seat_role,
        &lcars_by_norm,
        shared,
        &static_ctx,
        &group_lookup,
        &curated_bd_rank,
    );
    OfficerRoleSummary {
        provides_state: p.provides_state,
        needs_state: p.needs_state,
        bridge_effectiveness_boost: p.bridge_effectiveness_boost,
        captain_maneuver_boost: p.captain_maneuver_boost,
        synergy: p.synergy,
    }
}

/// Rank legal replacement candidates for a single seat, rewarding officers that preserve required
/// trigger states (`desired_states`), match captain synergy (`captain_synergy`), and score strongly
/// on static-folded LCARS abilities for the scenario.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rank_replacement_names_for_seat(
    registry: &DataRegistry,
    shared: &SharedScenarioData,
    pool_names: &[String],
    seat_role: CrewSeat,
    desired_states: u8,
    captain_synergy: Option<OfficerSynergyIdentity>,
    other_bridge_synergies: &[OfficerSynergyIdentity],
    limit: usize,
) -> Vec<String> {
    if pool_names.is_empty() || limit == 0 {
        return Vec::new();
    }
    let static_ctx = static_context_from_shared(shared);
    let group_lookup = captain_group_lookup_from_registry(registry);
    let curated_bd_rank: HashMap<String, u32> = HashMap::new();
    let mut lcars_by_norm: HashMap<String, &LcarsOfficer> = HashMap::new();
    if let Some(lcars) = registry.lcars_officers() {
        for officer in lcars {
            let key = normalize_officer_name(&officer.name);
            if !key.is_empty() {
                lcars_by_norm.insert(key, officer);
            }
        }
    }

    let mut scored: Vec<(String, f64, f64)> = pool_names
        .iter()
        .map(|name| {
            let p = build_role_profile(
                name,
                seat_role,
                &lcars_by_norm,
                shared,
                &static_ctx,
                &group_lookup,
                &curated_bd_rank,
            );
            let best_axis = p.scores[..5].iter().copied().fold(0.0_f64, f64::max);
            let sum_axes: f64 = p.scores[..5].iter().sum();
            let mut rank_score = best_axis + 0.15 * sum_axes;
            // Reward covering combat states consumed by remaining crewmates.
            let matched_states = (p.provides_state & desired_states).count_ones() as f64;
            rank_score += matched_states * 2.2;
            // Reward matching captain synergy for bridge seats or preserving synergy for captain.
            if let Some(cand_syn) = p.synergy {
                match seat_role {
                    CrewSeat::Bridge => {
                        if let Some(cap_syn) = captain_synergy {
                            if cand_syn.group_id == cap_syn.group_id {
                                let same_class_used = other_bridge_synergies.iter().any(|s| {
                                    s.group_id == cap_syn.group_id
                                        && s.officer_class == cand_syn.officer_class
                                });
                                if !same_class_used {
                                    rank_score += 1.8;
                                }
                            }
                        }
                    }
                    CrewSeat::Captain => {
                        let matching_bridge = other_bridge_synergies
                            .iter()
                            .filter(|s| s.group_id == cand_syn.group_id)
                            .count() as f64;
                        rank_score += matching_bridge * 1.4;
                    }
                    _ => {}
                }
            }
            (p.name, rank_score, p.officer_power)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| b.2.total_cmp(&a.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    scored.into_iter().take(limit).map(|(n, _, _)| n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::profile_index::DEMO_PROFILE_ID;
    use crate::data::support_buffs::SupportBuffScenarioRequest;
    use crate::optimizer::enforce_candidate_optimization_eligibility_with_registry;
    use crate::optimizer::monte_carlo::scenario::{
        build_shared_scenario_data_from_registry, DefenderOpponent,
    };

    #[test]
    fn diversity_beam_produces_legal_diverse_and_deterministic_crews() {
        let registry = DataRegistry::load().expect("registry loads");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "uss_saladin",
            "2918121098",
            None,
            None,
            Some(DEMO_PROFILE_ID),
            SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let params = BeamSearchParams {
            target_count: 96,
            below_decks_slots: 3,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            enemy_type: EnemyType::RedMovingSpace,
            profile_id: Some(DEMO_PROFILE_ID),
            constraints: None,
        };
        let c1 = propose_diversity_beam_crews_with_seats(&registry, &shared, &params).0;
        let c2 = propose_diversity_beam_crews_with_seats(&registry, &shared, &params).0;
        assert_eq!(c1.len(), c2.len());
        assert!(
            c1.len() >= 48,
            "expected broad beam output, got {}",
            c1.len()
        );
        for (a, b) in c1.iter().zip(c2.iter()) {
            assert_eq!(crew_candidate_stable_hash(a), crew_candidate_stable_hash(b));
        }

        // Distinct captain diversity across proposed crews.
        let distinct_captains: HashSet<&str> = c1.iter().map(|c| c.captain.as_str()).collect();
        assert!(
            distinct_captains.len() >= 10,
            "expected >= 10 distinct captains, got {}",
            distinct_captains.len()
        );

        // Strict legality & eligibility check.
        let (legal, summary) = enforce_candidate_optimization_eligibility_with_registry(
            &registry,
            Some(DEMO_PROFILE_ID),
            3,
            EnemyType::RedMovingSpace,
            c1.clone(),
        );
        assert_eq!(legal.len(), c1.len(), "legality dropped: {summary:?}");
    }

    #[test]
    fn diversity_beam_respects_captain_and_exclude_constraints() {
        let registry = DataRegistry::load().expect("registry loads");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "uss_saladin",
            "2918121098",
            None,
            None,
            Some(DEMO_PROFILE_ID),
            SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let constraints = CrewSearchConstraints {
            captain_must_be: vec!["Kirk".to_string()],
            exclude: vec!["Spock".to_string()],
            ..CrewSearchConstraints::default()
        };
        let params = BeamSearchParams {
            target_count: 32,
            below_decks_slots: 3,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            enemy_type: EnemyType::RedMovingSpace,
            profile_id: Some(DEMO_PROFILE_ID),
            constraints: Some(&constraints),
        };
        let crews = propose_diversity_beam_crews_with_seats(&registry, &shared, &params).0;
        assert!(!crews.is_empty());
        for c in &crews {
            assert_eq!(normalize_officer_name(&c.captain), "kirk");
            assert!(c
                .bridge
                .iter()
                .chain(c.below_decks.iter())
                .all(|o| normalize_officer_name(o) != "spock"));
        }
    }
}
