//! Phase 4d: per-round officer-stat breakpoint recomputation in the combat loop.
//!
//! Dynamic `officerstat*` rows (morale-gated Kirk, etc.) are collected at LCARS resolve time
//! and re-evaluated each round via [`OfficerStatRoundContext`].

use std::collections::HashMap;

use crate::combat::abilities::{CombatContext, TimingWindow};
use crate::combat::condition::evaluate_ability_condition;
use crate::combat::CrewOfficerStatTotals;
use crate::data::profile::{
    OfficerStatConditionContext, OfficerStatRuntimeBonus, PlayerProfile,
};
use crate::data::ship::{OfficerBonusTable, ShipRecord};
use crate::lcars::{DynamicOfficerStatContribution, PendingOfficerStatContribution};

/// Per-round delta from a dynamic officer-stat gate relative to the fight-setup baseline.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OfficerStatRoundDelta {
    /// Added to outbound `pre_attack_multiplier`: `(1+active.attack_bonus)/(1+baseline.attack_bonus) - 1`.
    pub attack_pre_mult_add: f64,
    pub defense_armor_add: f64,
    pub defense_shield_deflection_add: f64,
    pub defense_dodge_add: f64,
    /// Multiplier on max hull/shield HP for survivability: `(1+active.health_bonus)/(1+baseline.health_bonus)`.
    pub health_max_mult: f64,
}

impl OfficerStatRoundDelta {
    pub fn from_runtime(baseline: OfficerStatRuntimeBonus, active: OfficerStatRuntimeBonus) -> Self {
        let attack_ratio = (1.0 + active.attack_bonus) / (1.0 + baseline.attack_bonus);
        let health_ratio = (1.0 + active.health_bonus) / (1.0 + baseline.health_bonus);
        Self {
            attack_pre_mult_add: attack_ratio - 1.0,
            defense_armor_add: active.defense_armor_add - baseline.defense_armor_add,
            defense_shield_deflection_add: active.defense_shield_deflection_add
                - baseline.defense_shield_deflection_add,
            defense_dodge_add: active.defense_dodge_add - baseline.defense_dodge_add,
            health_max_mult: health_ratio,
        }
    }

    pub fn is_effectively_zero(self) -> bool {
        self.attack_pre_mult_add.abs() <= f64::EPSILON
            && self.defense_armor_add.abs() <= f64::EPSILON
            && self.defense_shield_deflection_add.abs() <= f64::EPSILON
            && self.defense_dodge_add.abs() <= f64::EPSILON
            && (self.health_max_mult - 1.0).abs() <= f64::EPSILON
    }
}

/// Immutable inputs for per-round officer-stat breakpoint lookup (stored on [`crate::combat::PreCombatSetup`]).
#[derive(Debug, Clone)]
pub struct OfficerStatRoundContext {
    pub officer_bonus: OfficerBonusTable,
    pub ship_class: String,
    pub base_armor: f64,
    pub base_shield_deflection: f64,
    pub base_dodge: f64,
    pub totals: CrewOfficerStatTotals,
    pub bridge_totals: CrewOfficerStatTotals,
    pub owner_faction: Option<String>,
    pub profile: PlayerProfile,
    pub static_buffs: HashMap<String, f64>,
    pub static_pending: Vec<PendingOfficerStatContribution>,
    pub opponent_enemy_pending: Vec<PendingOfficerStatContribution>,
    pub dynamic_contributions: Vec<DynamicOfficerStatContribution>,
    pub static_cond_ctx: OfficerStatConditionContext,
    pub baseline_osr: OfficerStatRuntimeBonus,
}

impl OfficerStatRoundContext {
    /// Build round context when the ship has breakpoint tables and at least one dynamic contribution.
    pub fn try_from_ship_and_buffs(
        ship: &ShipRecord,
        profile: &PlayerProfile,
        static_buffs: &HashMap<String, f64>,
        totals: CrewOfficerStatTotals,
        bridge_totals: CrewOfficerStatTotals,
        static_pending: &[PendingOfficerStatContribution],
        opponent_enemy_pending: &[PendingOfficerStatContribution],
        dynamic_contributions: &[DynamicOfficerStatContribution],
        static_cond_ctx: OfficerStatConditionContext,
    ) -> Option<Self> {
        if ship.officer_bonus.is_empty() || dynamic_contributions.is_empty() {
            return None;
        }
        let baseline_osr = crate::data::profile::compute_officer_stat_runtime_bonus_with_round(
            totals,
            bridge_totals,
            ship,
            profile,
            ship.faction.as_deref(),
            static_buffs,
            static_pending,
            &static_cond_ctx,
            opponent_enemy_pending,
            &[],
            None,
            None,
        );
        Some(Self {
            officer_bonus: ship.officer_bonus.clone(),
            ship_class: ship.ship_class.clone(),
            base_armor: ship.armor,
            base_shield_deflection: ship.shield_deflection,
            base_dodge: ship.dodge,
            totals,
            bridge_totals,
            owner_faction: ship.faction.clone(),
            profile: profile.clone(),
            static_buffs: static_buffs.clone(),
            static_pending: static_pending.to_vec(),
            opponent_enemy_pending: opponent_enemy_pending.to_vec(),
            dynamic_contributions: dynamic_contributions.to_vec(),
            static_cond_ctx,
            baseline_osr,
        })
    }

    /// Recompute runtime bonus for one timing window, applying dynamic rows whose conditions pass.
    pub fn runtime_bonus_for_timing(
        &self,
        combat_ctx: &CombatContext,
        timing: TimingWindow,
    ) -> OfficerStatRuntimeBonus {
        let active_dynamic: Vec<PendingOfficerStatContribution> = self
            .dynamic_contributions
            .iter()
            .filter(|c| c.timing == timing)
            .filter(|c| c.runtime_condition.as_ref().is_none_or(|cond| {
                evaluate_ability_condition(cond, combat_ctx)
            }))
            .map(|c| PendingOfficerStatContribution {
                stat_key: c.stat_key.clone(),
                value: c.value,
                target_attacker: c.target_attacker,
                conditions: Vec::new(),
                opponent_scope: c.opponent_scope,
            })
            .collect();

        let ship = ShipRecord {
            ship_class: self.ship_class.clone(),
            armor: self.base_armor,
            shield_deflection: self.base_shield_deflection,
            dodge: self.base_dodge,
            officer_bonus: self.officer_bonus.clone(),
            ..ShipRecord::default()
        };

        crate::data::profile::compute_officer_stat_runtime_bonus_with_round(
            self.totals,
            self.bridge_totals,
            &ship,
            &self.profile,
            self.owner_faction.as_deref(),
            &self.static_buffs,
            &self.static_pending,
            &self.static_cond_ctx,
            &self.opponent_enemy_pending,
            &active_dynamic,
            Some(combat_ctx),
            Some(timing),
        )
    }

    pub fn delta_for_timing(
        &self,
        combat_ctx: &CombatContext,
        timing: TimingWindow,
    ) -> OfficerStatRoundDelta {
        let active = self.runtime_bonus_for_timing(combat_ctx, timing);
        OfficerStatRoundDelta::from_runtime(self.baseline_osr, active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::AbilityCondition;
    use crate::combat::OpponentFactionTag;
    use crate::combat::ShipType;
    use crate::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable};
    use crate::lcars::OfficerStatOpponentScope;

    fn test_ship() -> ShipRecord {
        ShipRecord {
            ship_class: "battleship".to_string(),
            armor: 1000.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            officer_bonus: OfficerBonusTable {
                attack: vec![OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                }],
                defense: vec![OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                }],
                health: vec![OfficerBonusBreakpoint {
                    value: 0.0,
                    bonus: 0.0,
                }],
            },
            ..ShipRecord::default()
        }
    }

    #[test]
    fn round_delta_zero_when_no_dynamic_rows_active() {
        let ship = test_ship();
        let dynamic = vec![DynamicOfficerStatContribution {
            stat_key: "officer_stat_all".to_string(),
            value: 0.40,
            target_attacker: true,
            opponent_scope: OfficerStatOpponentScope::AllCrewed,
            runtime_condition: Some(AbilityCondition::MoraleActive),
            timing: TimingWindow::RoundStart,
        }];
        let ctx = OfficerStatRoundContext::try_from_ship_and_buffs(
            &ship,
            &PlayerProfile::default(),
            &HashMap::new(),
            CrewOfficerStatTotals::default(),
            CrewOfficerStatTotals::default(),
            &[],
            &[],
            &dynamic,
            OfficerStatConditionContext::default(),
        )
        .expect("context");

        let combat_ctx = CombatContext {
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
            defender_ship_type: ShipType::Explorer,
            attacker_ship_type: ShipType::Battleship,
            attacker_ship_id: std::sync::Arc::from("att"),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: true,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: std::sync::Arc::new(Default::default()),
            combat_battle_type_id: None,
            defender_level: Some(50),
        };

        let delta = ctx.delta_for_timing(&combat_ctx, TimingWindow::RoundStart);
        assert!(delta.is_effectively_zero());
    }
}
