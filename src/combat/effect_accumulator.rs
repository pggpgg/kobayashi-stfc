//! Effect scaling and stacking for the combat loop.

use serde_json::{Map, Value};

use crate::combat::abilities::{AbilityEffect, ActiveAbilityEffect, TimingWindow};
use crate::combat::events::round_f64;
use crate::combat::stacking::{StackContribution, StatStacking};
use crate::combat::types::{
    CombatEvent, Combatant, EventSource, TraceCollector, ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
};

#[derive(Debug, Clone)]
pub(crate) struct EffectAccumulator {
    stacks: StatStacking<EffectStatKey>,
    pre_attack_modifier_sum: f64,
    /// Cumulative Galaxy-class hull growth `g` (weapon_damage fraction units); applied in the engine as `×(1+g/(1+p))`.
    galaxy_additive_weapon_frac: f64,
    attack_phase_damage_modifier_sum: f64,
    round_end_modifier_sum: f64,
    /// Sum of timed [`AbilityEffect::CritChanceBonus`] for the current shot stack.
    crit_chance_bonus: f64,
    /// Product of timed [`AbilityEffect::CritDamageMultiplier`] for the current shot stack.
    crit_damage_multiplier: f64,
    /// When true, each applied effect appends a row to [`Self::contribution_lines`] for `stack_resolution` traces.
    trace_contributions: bool,
    contribution_lines: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EffectStatKey {
    PreAttackPierceBonus,
    DefenseMitigationBonus,
    PreAttackDamage,
    AttackPhaseDamage,
    RoundEndDamage,
    ApexShredBonus,
    ApexBarrierBonus,
    ShieldRegen,
    HullRegen,
    ShieldRegenMaxFraction,
    HullRegenMaxFraction,
    IsolyticDamageBonus,
    IsolyticDefenseBonus,
    IsolyticCascadeDamageBonus,
    ShieldMitigationBonus,
}

impl EffectStatKey {
    pub(crate) fn as_trace_key(self) -> &'static str {
        match self {
            EffectStatKey::PreAttackPierceBonus => "pre_attack_pierce_bonus",
            EffectStatKey::DefenseMitigationBonus => "defense_mitigation_bonus",
            EffectStatKey::PreAttackDamage => "pre_attack_damage",
            EffectStatKey::AttackPhaseDamage => "attack_phase_damage",
            EffectStatKey::RoundEndDamage => "round_end_damage",
            EffectStatKey::ApexShredBonus => "apex_shred_bonus",
            EffectStatKey::ApexBarrierBonus => "apex_barrier_bonus",
            EffectStatKey::ShieldRegen => "shield_regen",
            EffectStatKey::HullRegen => "hull_regen",
            EffectStatKey::ShieldRegenMaxFraction => "shield_regen_max_fraction",
            EffectStatKey::HullRegenMaxFraction => "hull_regen_max_fraction",
            EffectStatKey::IsolyticDamageBonus => "isolytic_damage_bonus",
            EffectStatKey::IsolyticDefenseBonus => "isolytic_defense_bonus",
            EffectStatKey::IsolyticCascadeDamageBonus => "isolytic_cascade_damage_bonus",
            EffectStatKey::ShieldMitigationBonus => "shield_mitigation_bonus",
        }
    }
}

impl Default for EffectAccumulator {
    fn default() -> Self {
        let mut stacks = StatStacking::new();
        stacks.add(StackContribution::base(
            EffectStatKey::PreAttackPierceBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::DefenseMitigationBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(EffectStatKey::PreAttackDamage, 0.0));
        stacks.add(StackContribution::base(
            EffectStatKey::AttackPhaseDamage,
            0.0,
        ));
        stacks.add(StackContribution::base(EffectStatKey::RoundEndDamage, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::ApexShredBonus, 0.0));
        stacks.add(StackContribution::base(
            EffectStatKey::ApexBarrierBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(EffectStatKey::ShieldRegen, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::HullRegen, 0.0));
        stacks.add(StackContribution::base(
            EffectStatKey::ShieldRegenMaxFraction,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::HullRegenMaxFraction,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::IsolyticDamageBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::IsolyticDefenseBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::IsolyticCascadeDamageBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(
            EffectStatKey::ShieldMitigationBonus,
            0.0,
        ));

        Self {
            stacks,
            pre_attack_modifier_sum: 0.0,
            galaxy_additive_weapon_frac: 0.0,
            attack_phase_damage_modifier_sum: 0.0,
            round_end_modifier_sum: 0.0,
            crit_chance_bonus: 0.0,
            crit_damage_multiplier: 1.0,
            trace_contributions: false,
            contribution_lines: Vec::new(),
        }
    }
}

fn timing_trace_id(timing: TimingWindow) -> &'static str {
    match timing {
        TimingWindow::CombatBegin => "combat_begin",
        TimingWindow::RoundStart => "round_start",
        TimingWindow::AttackPhase => "attack_phase",
        TimingWindow::AfterSubround => "after_subround",
        TimingWindow::DefensePhase => "defense_phase",
        TimingWindow::RoundEnd => "round_end",
        TimingWindow::ShieldBreak => "shield_break",
        TimingWindow::SelfShieldBreak => "self_shield_break",
        TimingWindow::Kill => "kill",
        TimingWindow::HullBreach => "hull_breach",
        TimingWindow::ReceiveDamage => "receive_damage",
        TimingWindow::CombatEnd => "combat_end",
    }
}

impl EffectAccumulator {
    pub(crate) fn set_trace_contributions(&mut self, on: bool) {
        self.trace_contributions = on;
    }

    fn push_contribution_line(
        &mut self,
        ability: &str,
        officer_id: Option<&str>,
        timing: TimingWindow,
        effect_kind: &'static str,
        target: &str,
        value: f64,
    ) {
        if !self.trace_contributions {
            return;
        }
        let mut m = Map::new();
        m.insert("ability".to_string(), Value::String(ability.to_string()));
        if let Some(o) = officer_id {
            m.insert("officer_id".to_string(), Value::String(o.to_string()));
        }
        m.insert(
            "timing".to_string(),
            Value::String(timing_trace_id(timing).to_string()),
        );
        m.insert("effect".to_string(), Value::String(effect_kind.to_string()));
        m.insert("target".to_string(), Value::String(target.to_string()));
        m.insert("value".to_string(), Value::from(round_f64(value)));
        self.contribution_lines.push(Value::Object(m));
    }

    fn add_stack_flat_traced(
        &mut self,
        key: EffectStatKey,
        flat_value: f64,
        timing: TimingWindow,
        source: Option<(&str, Option<&str>)>,
        effect_kind: &'static str,
    ) {
        self.stacks.add(StackContribution::flat(key, flat_value));
        if self.trace_contributions {
            if let Some((ab, oid)) = source {
                let target = format!("stack:{}:flat", key.as_trace_key());
                self.push_contribution_line(ab, oid, timing, effect_kind, &target, flat_value);
            }
        }
    }

    fn trace_add_pre_mod(
        &mut self,
        source: Option<(&str, Option<&str>)>,
        timing: TimingWindow,
        effect_kind: &'static str,
        delta: f64,
    ) {
        self.pre_attack_modifier_sum += delta;
        if self.trace_contributions {
            if let Some((ab, oid)) = source {
                self.push_contribution_line(
                    ab,
                    oid,
                    timing,
                    effect_kind,
                    "pre_attack_modifier_sum",
                    delta,
                );
            }
        }
    }

    fn trace_add_attack_phase_mod(
        &mut self,
        source: Option<(&str, Option<&str>)>,
        timing: TimingWindow,
        effect_kind: &'static str,
        delta: f64,
    ) {
        self.attack_phase_damage_modifier_sum += delta;
        if self.trace_contributions {
            if let Some((ab, oid)) = source {
                self.push_contribution_line(
                    ab,
                    oid,
                    timing,
                    effect_kind,
                    "attack_phase_damage_modifier_sum",
                    delta,
                );
            }
        }
    }

    fn trace_add_round_end_mod(
        &mut self,
        source: Option<(&str, Option<&str>)>,
        timing: TimingWindow,
        effect_kind: &'static str,
        delta: f64,
    ) {
        self.round_end_modifier_sum += delta;
        if self.trace_contributions {
            if let Some((ab, oid)) = source {
                self.push_contribution_line(
                    ab,
                    oid,
                    timing,
                    effect_kind,
                    "round_end_modifier_sum",
                    delta,
                );
            }
        }
    }

    fn trace_crit_chance(
        &mut self,
        source: Option<(&str, Option<&str>)>,
        timing: TimingWindow,
        v: f64,
    ) {
        self.crit_chance_bonus += v;
        if self.trace_contributions {
            if let Some((ab, oid)) = source {
                self.push_contribution_line(
                    ab,
                    oid,
                    timing,
                    "CritChanceBonus",
                    "crit_chance_bonus",
                    v,
                );
            }
        }
    }

    fn trace_crit_damage_mult(
        &mut self,
        source: Option<(&str, Option<&str>)>,
        timing: TimingWindow,
        factor: f64,
    ) {
        if factor.is_finite() && factor > 0.0 {
            self.crit_damage_multiplier *= factor;
            if self.trace_contributions {
                if let Some((ab, oid)) = source {
                    self.push_contribution_line(
                        ab,
                        oid,
                        timing,
                        "CritDamageMultiplier",
                        "crit_damage_multiplier",
                        factor,
                    );
                }
            }
        }
    }

    pub(crate) fn pre_attack_multiplier(&self) -> f64 {
        (1.0 + self.pre_attack_modifier_sum).max(0.0)
    }

    #[inline]
    pub(crate) fn galaxy_additive_weapon_frac(&self) -> f64 {
        self.galaxy_additive_weapon_frac
    }

    pub(crate) fn pre_attack_pierce_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::PreAttackPierceBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn defense_mitigation_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::DefenseMitigationBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_apex_shred_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ApexShredBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_apex_barrier_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ApexBarrierBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_shield_regen(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ShieldRegen)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_hull_regen(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::HullRegen)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_shield_regen_max_fraction(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ShieldRegenMaxFraction)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_hull_regen_max_fraction(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::HullRegenMaxFraction)
            .unwrap_or(0.0)
    }

    /// Sum flat shield restoration from timed effects (used for defender round-start before the main phase accumulator runs).
    pub(crate) fn sum_shield_regen_from_effects(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::ShieldRegen(v) = scale_effect(e.effect, assimilated_active) {
                    Some(v)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Sum flat hull restoration from timed effects (not [`AbilityEffect::OnKillHullRegen`]).
    pub(crate) fn sum_hull_regen_from_effects(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::HullRegen(v) = scale_effect(e.effect, assimilated_active) {
                    Some(v)
                } else {
                    None
                }
            })
            .sum()
    }

    pub(crate) fn sum_shield_regen_max_fraction_from_effects(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::ShieldRegenMaxFraction(f) =
                    scale_effect(e.effect, assimilated_active)
                {
                    Some(f)
                } else {
                    None
                }
            })
            .sum()
    }

    pub(crate) fn sum_hull_regen_max_fraction_from_effects(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::HullRegenMaxFraction(f) =
                    scale_effect(e.effect, assimilated_active)
                {
                    Some(f)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Sum `fraction` values for [`AbilityEffect::HullRegenPrevRoundFraction`] (PIC Hugh–style heal).
    pub(crate) fn sum_hull_regen_prev_round_fraction(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::HullRegenPrevRoundFraction(f) =
                    scale_effect(e.effect, assimilated_active)
                {
                    Some(f)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Sum `fraction` values for [`AbilityEffect::ShieldRegenPrevRoundFraction`] (Mara Dalen below decks vs armadas).
    pub(crate) fn sum_shield_regen_prev_round_fraction(
        effects: &[ActiveAbilityEffect],
        assimilated_active: bool,
    ) -> f64 {
        effects
            .iter()
            .filter_map(|e| {
                if let AbilityEffect::ShieldRegenPrevRoundFraction(f) =
                    scale_effect(e.effect, assimilated_active)
                {
                    Some(f)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Remove shield/hull regen stacks so values from CombatBegin/RoundStart are not applied again at round end.
    pub(crate) fn clear_shield_hull_regen_stacks(&mut self) {
        self.stacks.remove_totals_for(&EffectStatKey::ShieldRegen);
        self.stacks.remove_totals_for(&EffectStatKey::HullRegen);
        self.stacks
            .remove_totals_for(&EffectStatKey::ShieldRegenMaxFraction);
        self.stacks
            .remove_totals_for(&EffectStatKey::HullRegenMaxFraction);
    }

    pub(crate) fn composed_isolytic_damage_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::IsolyticDamageBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_isolytic_defense_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::IsolyticDefenseBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_isolytic_cascade_damage_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::IsolyticCascadeDamageBonus)
            .unwrap_or(0.0)
    }

    pub(crate) fn composed_shield_mitigation_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ShieldMitigationBonus)
            .unwrap_or(0.0)
    }

    #[inline]
    pub(crate) fn crit_chance_bonus(&self) -> f64 {
        self.crit_chance_bonus
    }

    #[inline]
    pub(crate) fn crit_damage_multiplier(&self) -> f64 {
        self.crit_damage_multiplier
    }

    pub(crate) fn compose_attack_phase_damage(&self, pre_attack_damage: f64) -> f64 {
        self.compose_damage_channel(EffectStatKey::AttackPhaseDamage, pre_attack_damage)
    }

    pub(crate) fn compose_round_end_damage(&self, round_end_damage: f64) -> f64 {
        self.compose_damage_channel(EffectStatKey::RoundEndDamage, round_end_damage)
    }

    fn compose_damage_channel(&self, key: EffectStatKey, base: f64) -> f64 {
        let flat = self
            .stacks
            .totals_for(&key)
            .map(|totals| totals.flat)
            .unwrap_or(0.0);
        let multiplier = match key {
            EffectStatKey::AttackPhaseDamage => 1.0 + self.attack_phase_damage_modifier_sum,
            EffectStatKey::RoundEndDamage => 1.0 + self.round_end_modifier_sum,
            _ => 1.0,
        };

        base * multiplier + flat
    }

    pub(crate) fn set_pre_attack_damage_base(&mut self, base: f64) {
        self.stacks.add(StackContribution::base(
            EffectStatKey::PreAttackDamage,
            base,
        ));
    }

    pub(crate) fn composed_pre_attack_damage(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::PreAttackDamage)
            .unwrap_or(0.0)
    }

    pub(crate) fn clear(&mut self) {
        self.stacks.clear();
        self.pre_attack_modifier_sum = 0.0;
        self.galaxy_additive_weapon_frac = 0.0;
        self.attack_phase_damage_modifier_sum = 0.0;
        self.round_end_modifier_sum = 0.0;
        self.crit_chance_bonus = 0.0;
        self.crit_damage_multiplier = 1.0;
        self.contribution_lines.clear();
    }

    pub(crate) fn merge_from(&mut self, other: &EffectAccumulator) {
        self.stacks.merge_from(&other.stacks);
        self.pre_attack_modifier_sum = other.pre_attack_modifier_sum;
        self.galaxy_additive_weapon_frac = other.galaxy_additive_weapon_frac;
        self.attack_phase_damage_modifier_sum = other.attack_phase_damage_modifier_sum;
        self.round_end_modifier_sum = other.round_end_modifier_sum;
        self.crit_chance_bonus += other.crit_chance_bonus;
        self.crit_damage_multiplier *= other.crit_damage_multiplier;
        if self.trace_contributions {
            self.contribution_lines
                .extend(other.contribution_lines.iter().cloned());
        }
    }

    /// Additive merge for cross-sub-round carry (AfterSubround → next weapon) without overwriting round base sums.
    pub(crate) fn merge_carry_additive(&mut self, carry: &EffectAccumulator) {
        self.stacks.merge_from(&carry.stacks);
        self.pre_attack_modifier_sum += carry.pre_attack_modifier_sum;
        self.galaxy_additive_weapon_frac += carry.galaxy_additive_weapon_frac;
        self.attack_phase_damage_modifier_sum += carry.attack_phase_damage_modifier_sum;
        self.round_end_modifier_sum += carry.round_end_modifier_sum;
        self.crit_chance_bonus += carry.crit_chance_bonus;
        self.crit_damage_multiplier *= carry.crit_damage_multiplier;
        if self.trace_contributions {
            self.contribution_lines
                .extend(carry.contribution_lines.iter().cloned());
        }
    }

    /// JSON-friendly base / modifier / flat decomposition for trace (`stack_resolution` event).
    pub(crate) fn stack_resolution_values(&self) -> Map<String, Value> {
        const EPS: f64 = 1e-12;
        let mut out = Map::new();
        out.insert(
            "pre_attack_multiplier".to_string(),
            Value::from(round_f64(self.pre_attack_multiplier())),
        );
        if self.galaxy_additive_weapon_frac.abs() > EPS {
            out.insert(
                "galaxy_additive_weapon_frac".to_string(),
                Value::from(round_f64(self.galaxy_additive_weapon_frac)),
            );
        }
        out.insert(
            "attack_phase_damage_multiplier".to_string(),
            Value::from(round_f64(1.0 + self.attack_phase_damage_modifier_sum)),
        );
        out.insert(
            "round_end_damage_multiplier".to_string(),
            Value::from(round_f64(1.0 + self.round_end_modifier_sum)),
        );
        if self.crit_chance_bonus.abs() > EPS {
            out.insert(
                "crit_chance_bonus".to_string(),
                Value::from(round_f64(self.crit_chance_bonus)),
            );
        }
        if (self.crit_damage_multiplier - 1.0).abs() > EPS {
            out.insert(
                "crit_damage_multiplier".to_string(),
                Value::from(round_f64(self.crit_damage_multiplier)),
            );
        }

        let mut stacks_obj = Map::new();
        for (&key, totals) in self.stacks.iter_totals() {
            if totals.base.abs() <= EPS && totals.modifier.abs() <= EPS && totals.flat.abs() <= EPS
            {
                continue;
            }
            let composed = totals.compose();
            stacks_obj.insert(
                key.as_trace_key().to_string(),
                Value::Object(Map::from_iter([
                    ("base".to_string(), Value::from(round_f64(totals.base))),
                    (
                        "modifier_sum".to_string(),
                        Value::from(round_f64(totals.modifier)),
                    ),
                    ("flat".to_string(), Value::from(round_f64(totals.flat))),
                    ("composed".to_string(), Value::from(round_f64(composed))),
                ])),
            );
        }
        if !stacks_obj.is_empty() {
            out.insert("stacks".to_string(), Value::Object(stacks_obj));
        }
        if !self.contribution_lines.is_empty() {
            out.insert(
                "effect_contributions".to_string(),
                Value::Array(self.contribution_lines.clone()),
            );
        }
        out
    }

    pub(crate) fn add_effects(
        &mut self,
        timing: TimingWindow,
        effects: &[ActiveAbilityEffect],
        base_attack: f64,
        assimilated_active: bool,
        round_index: u32,
    ) {
        for effect in effects {
            let scaled = scale_effect(effect.effect, assimilated_active);
            let src = Some((effect.ability_name.as_str(), effect.officer_id.as_deref()));
            self.add_effect(timing, scaled, base_attack, round_index, src);
        }
    }

    pub(crate) fn add_effect(
        &mut self,
        timing: TimingWindow,
        effect: AbilityEffect,
        base_attack: f64,
        round_index: u32,
        source: Option<(&str, Option<&str>)>,
    ) {
        if matches!(
            timing,
            TimingWindow::CombatBegin
                | TimingWindow::RoundStart
                | TimingWindow::AttackPhase
                | TimingWindow::AfterSubround
                | TimingWindow::DefensePhase
        ) {
            if let AbilityEffect::CritChanceBonus(v) = &effect {
                self.trace_crit_chance(source, timing, *v);
                return;
            }
            if let AbilityEffect::CritDamageMultiplier(m) = &effect {
                self.trace_crit_damage_mult(source, timing, *m);
                return;
            }
        }

        match timing {
            TimingWindow::CombatBegin | TimingWindow::RoundStart => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.trace_add_pre_mod(source, timing, "AttackMultiplier", modifier);
                }
                AbilityEffect::PierceBonus(value) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::PreAttackPierceBonus,
                        value,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegen,
                        v,
                        timing,
                        source,
                        "ShieldRegen",
                    );
                }
                AbilityEffect::ShieldRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "ShieldRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegen,
                        v,
                        timing,
                        source,
                        "HullRegen",
                    );
                }
                AbilityEffect::HullRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "HullRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::MitigationAdditive(_) => {}
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.trace_add_pre_mod(source, timing, "DecayingAttackMultiplier", value - 1.0);
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.trace_add_pre_mod(
                        source,
                        timing,
                        "AccumulatingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round, cap } => {
                    if matches!(timing, TimingWindow::RoundStart) {
                        let r = round_index as f64;
                        let debuff = (per_round * r).min(cap);
                        if debuff.is_finite() && debuff > 0.0 {
                            self.add_stack_flat_traced(
                                EffectStatKey::ShieldMitigationBonus,
                                -debuff,
                                timing,
                                source,
                                "CumulativeOpponentShieldMitigationDebuff",
                            );
                        }
                    }
                }
                AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
                    growth_per_round,
                    ceiling,
                } => {
                    if matches!(timing, TimingWindow::RoundStart) {
                        let r = round_index as f64;
                        let g = (r * growth_per_round).min(ceiling);
                        if g.is_finite() && g > 0.0 {
                            self.galaxy_additive_weapon_frac += g;
                            if let Some((ab, oid)) = source {
                                self.push_contribution_line(
                                    ab,
                                    oid,
                                    timing,
                                    "GalaxyAdditiveWeaponDamageGrowth",
                                    "galaxy_additive_weapon_frac",
                                    g,
                                );
                            }
                        }
                    }
                }
                AbilityEffect::AccuracyBonus(_) => {}
            },
            TimingWindow::AttackPhase => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.trace_add_attack_phase_mod(source, timing, "AttackMultiplier", modifier);
                }
                AbilityEffect::PierceBonus(value) => {
                    let flat = value * base_attack * 0.5;
                    self.add_stack_flat_traced(
                        EffectStatKey::AttackPhaseDamage,
                        flat,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ShieldRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::MitigationAdditive(_) => {}
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.trace_add_attack_phase_mod(
                        source,
                        timing,
                        "DecayingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.trace_add_attack_phase_mod(
                        source,
                        timing,
                        "AccumulatingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. } => {}
                AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { .. } => {}
                AbilityEffect::AccuracyBonus(_) => {}
            },
            // Same stacking rules as AttackPhase; evaluated once per sub-round end for carry into later weapons.
            TimingWindow::AfterSubround => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.trace_add_attack_phase_mod(source, timing, "AttackMultiplier", modifier);
                }
                AbilityEffect::PierceBonus(value) => {
                    let flat = value * base_attack * 0.5;
                    self.add_stack_flat_traced(
                        EffectStatKey::AttackPhaseDamage,
                        flat,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ShieldRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::MitigationAdditive(_) => {}
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.trace_add_attack_phase_mod(
                        source,
                        timing,
                        "DecayingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.trace_add_attack_phase_mod(
                        source,
                        timing,
                        "AccumulatingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. } => {}
                AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { .. } => {}
                AbilityEffect::AccuracyBonus(_) => {}
            },
            TimingWindow::DefensePhase => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::DefenseMitigationBonus,
                        modifier,
                        timing,
                        source,
                        "AttackMultiplier",
                    );
                }
                AbilityEffect::PierceBonus(value) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::DefenseMitigationBonus,
                        value,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ShieldRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenMaxFraction(_) => {}
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier { .. }
                | AbilityEffect::AccumulatingAttackMultiplier { .. }
                | AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { .. }
                | AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. }
                | AbilityEffect::MitigationAdditive(_)
                | AbilityEffect::AccuracyBonus(_) => {}
            },
            // Resolved in the engine only after all weapon sub-rounds for the same round
            // (`engine.rs`: RoundEnd is merged into `phase_effects_round` post-weapon-loop).
            TimingWindow::RoundEnd => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.trace_add_round_end_mod(source, timing, "AttackMultiplier", modifier);
                }
                AbilityEffect::PierceBonus(value) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::RoundEndDamage,
                        value,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegen,
                        v,
                        timing,
                        source,
                        "ShieldRegen",
                    );
                }
                AbilityEffect::ShieldRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "ShieldRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegen,
                        v,
                        timing,
                        source,
                        "HullRegen",
                    );
                }
                AbilityEffect::HullRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "HullRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::MitigationAdditive(_) => {}
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.trace_add_round_end_mod(
                        source,
                        timing,
                        "DecayingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.trace_add_round_end_mod(
                        source,
                        timing,
                        "AccumulatingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. } => {}
                AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { .. } => {}
                AbilityEffect::AccuracyBonus(_) => {}
            },
            TimingWindow::ShieldBreak
            | TimingWindow::SelfShieldBreak
            | TimingWindow::Kill
            | TimingWindow::HullBreach
            | TimingWindow::ReceiveDamage
            | TimingWindow::CombatEnd => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.trace_add_pre_mod(source, timing, "AttackMultiplier", modifier);
                }
                AbilityEffect::PierceBonus(value) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::PreAttackPierceBonus,
                        value,
                        timing,
                        source,
                        "PierceBonus",
                    );
                }
                AbilityEffect::ProcAttackMultiplier { .. } => {}
                AbilityEffect::ProcPierceBonus { .. } => {}
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegen,
                        v,
                        timing,
                        source,
                        "ShieldRegen",
                    );
                }
                AbilityEffect::ShieldRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "ShieldRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegen,
                        v,
                        timing,
                        source,
                        "HullRegen",
                    );
                }
                AbilityEffect::HullRegenMaxFraction(f) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegenMaxFraction,
                        f,
                        timing,
                        source,
                        "HullRegenMaxFraction",
                    );
                }
                AbilityEffect::HullRegenPrevRoundFraction(_) => {}
                AbilityEffect::ShieldRegenPrevRoundFraction(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexShredBonus,
                        v,
                        timing,
                        source,
                        "ApexShredBonus",
                    );
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ApexBarrierBonus,
                        v,
                        timing,
                        source,
                        "ApexBarrierBonus",
                    );
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDamageBonus",
                    );
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticDefenseBonus,
                        v,
                        timing,
                        source,
                        "IsolyticDefenseBonus",
                    );
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                        timing,
                        source,
                        "IsolyticCascadeDamageBonus",
                    );
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                        timing,
                        source,
                        "ShieldMitigationBonus",
                    );
                }
                AbilityEffect::MitigationAdditive(_) => {}
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::HostileCritDamageReduction { .. }
                | AbilityEffect::ConquerorBorgBeamSuppression => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.trace_add_pre_mod(source, timing, "DecayingAttackMultiplier", value - 1.0);
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.trace_add_pre_mod(
                        source,
                        timing,
                        "AccumulatingAttackMultiplier",
                        value - 1.0,
                    );
                }
                AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. } => {}
                AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { .. } => {}
                AbilityEffect::AccuracyBonus(_) => {}
            },
        }
    }
}

pub(crate) fn sum_on_kill_hull_regen(
    effects: &[ActiveAbilityEffect],
    assimilated_active: bool,
) -> f64 {
    effects
        .iter()
        .filter_map(|e| {
            if let AbilityEffect::OnKillHullRegen(v) = scale_effect(e.effect, assimilated_active) {
                Some(v)
            } else {
                None
            }
        })
        .sum()
}

pub(crate) fn record_ability_activations(
    trace: &mut TraceCollector,
    round_index: u32,
    phase: &str,
    attacker: &Combatant,
    effects: &[ActiveAbilityEffect],
    assimilated_active: bool,
) {
    let effectiveness_multiplier = if assimilated_active {
        ASSIMILATED_EFFECTIVENESS_MULTIPLIER
    } else {
        1.0
    };

    for effect in effects {
        trace.record_if(|| CombatEvent {
            event_type: "ability_activation".to_string(),
            round_index,
            phase: phase.to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                ship_ability_id: Some(effect.ability_name.clone()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("boosted".to_string(), Value::Bool(effect.boosted)),
                (
                    "effectiveness_multiplier".to_string(),
                    Value::from(effectiveness_multiplier),
                ),
                ("assimilated".to_string(), Value::Bool(assimilated_active)),
            ]),
        });
    }
}

pub(crate) fn scale_effect(effect: AbilityEffect, assimilated_active: bool) -> AbilityEffect {
    if !assimilated_active {
        return effect;
    }

    match effect {
        AbilityEffect::AttackMultiplier(modifier) => {
            AbilityEffect::AttackMultiplier(modifier * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::PierceBonus(value) => {
            AbilityEffect::PierceBonus(value * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ProcAttackMultiplier { chance, multiplier } => {
            AbilityEffect::ProcAttackMultiplier {
                chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
                multiplier,
            }
        }
        AbilityEffect::ProcPierceBonus { chance, bonus } => AbilityEffect::ProcPierceBonus {
            chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            bonus: bonus * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
        },
        AbilityEffect::Morale(chance) => {
            AbilityEffect::Morale(chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::Assimilated {
            chance,
            duration_rounds,
        } => AbilityEffect::Assimilated {
            chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            duration_rounds,
        },
        AbilityEffect::HullBreach {
            chance,
            duration_rounds,
            requires_critical,
        } => AbilityEffect::HullBreach {
            chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            duration_rounds,
            requires_critical,
        },
        AbilityEffect::Burning {
            chance,
            duration_rounds,
        } => AbilityEffect::Burning {
            chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            duration_rounds,
        },
        AbilityEffect::ApexShredBonus(v) => {
            AbilityEffect::ApexShredBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ApexBarrierBonus(v) => {
            AbilityEffect::ApexBarrierBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ShieldRegen(v) => {
            AbilityEffect::ShieldRegen(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ShieldRegenMaxFraction(f) => {
            AbilityEffect::ShieldRegenMaxFraction(f * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::HullRegen(v) => {
            AbilityEffect::HullRegen(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::HullRegenMaxFraction(f) => {
            AbilityEffect::HullRegenMaxFraction(f * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::HullRegenPrevRoundFraction(f) => {
            AbilityEffect::HullRegenPrevRoundFraction(f * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ShieldRegenPrevRoundFraction(f) => {
            AbilityEffect::ShieldRegenPrevRoundFraction(f * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::IsolyticDamageBonus(v) => {
            AbilityEffect::IsolyticDamageBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::IsolyticDefenseBonus(v) => {
            AbilityEffect::IsolyticDefenseBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::IsolyticCascadeDamageBonus(v) => {
            AbilityEffect::IsolyticCascadeDamageBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::ShieldMitigationBonus(v) => {
            AbilityEffect::ShieldMitigationBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::OnKillHullRegen(v) => {
            AbilityEffect::OnKillHullRegen(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::DecayingAttackMultiplier {
            initial,
            decay_per_round,
            floor,
        } => AbilityEffect::DecayingAttackMultiplier {
            initial: 1.0 + (initial - 1.0) * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            decay_per_round,
            floor,
        },
        AbilityEffect::AccumulatingAttackMultiplier {
            initial,
            growth_per_round,
            ceiling,
        } => AbilityEffect::AccumulatingAttackMultiplier {
            initial: 1.0 + (initial - 1.0) * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            growth_per_round,
            ceiling,
        },
        AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
            growth_per_round,
            ceiling,
        } => AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
            growth_per_round: growth_per_round * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            ceiling,
        },
        AbilityEffect::ShotsBonus {
            chance,
            bonus_pct,
            duration_rounds,
        } => AbilityEffect::ShotsBonus {
            chance: chance * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            bonus_pct: bonus_pct * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
            duration_rounds,
        },
        AbilityEffect::CritChanceBonus(v) => {
            AbilityEffect::CritChanceBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::CritDamageMultiplier(m) => AbilityEffect::CritDamageMultiplier(
            1.0 + (m - 1.0) * ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
        ),
        AbilityEffect::MitigationAdditive(v) => {
            AbilityEffect::MitigationAdditive(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
        AbilityEffect::HostileCritDamageReduction { .. } => effect,
        AbilityEffect::CumulativeOpponentShieldMitigationDebuff { .. } => effect,
        AbilityEffect::ConquerorBorgBeamSuppression => effect,
        AbilityEffect::AccuracyBonus(_) => effect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::{TimingWindow, ActiveAbilityEffect};

    const ASSIM: f64 = ASSIMILATED_EFFECTIVENESS_MULTIPLIER; // 0.75

    fn make_active(effect: AbilityEffect) -> ActiveAbilityEffect {
        ActiveAbilityEffect {
            ability_name: "test".into(),
            officer_id: None,
            effect,
            boosted: false,
            condition: None,
        }
    }

    // ── scale_effect: identity when not assimilated ──

    #[test]
    fn scale_effect_no_assimilate_returns_unchanged() {
        let e = AbilityEffect::AttackMultiplier(0.4);
        let scaled = scale_effect(e, false);
        assert_eq!(scaled, AbilityEffect::AttackMultiplier(0.4));
    }

    // ── scale_effect: scalar variants × ASSIMILATED_EFFECTIVENESS_MULTIPLIER ──

    #[test]
    fn scale_attack_multiplier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::AttackMultiplier(0.4), true);
        assert_eq!(scaled, AbilityEffect::AttackMultiplier(0.4 * ASSIM));
    }

    #[test]
    fn scale_pierce_bonus_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::PierceBonus(0.2), true);
        assert_eq!(scaled, AbilityEffect::PierceBonus(0.2 * ASSIM));
    }

    #[test]
    fn scale_morale_chance_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::Morale(0.8), true);
        assert_eq!(scaled, AbilityEffect::Morale(0.8 * ASSIM));
    }

    #[test]
    fn scale_apex_shred_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ApexShredBonus(0.15), true);
        assert_eq!(scaled, AbilityEffect::ApexShredBonus(0.15 * ASSIM));
    }

    #[test]
    fn scale_apex_barrier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ApexBarrierBonus(1000.0), true);
        assert_eq!(scaled, AbilityEffect::ApexBarrierBonus(1000.0 * ASSIM));
    }

    #[test]
    fn scale_shield_regen_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ShieldRegen(500.0), true);
        assert_eq!(scaled, AbilityEffect::ShieldRegen(500.0 * ASSIM));
    }

    #[test]
    fn scale_hull_regen_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::HullRegen(300.0), true);
        assert_eq!(scaled, AbilityEffect::HullRegen(300.0 * ASSIM));
    }

    #[test]
    fn scale_shield_regen_max_fraction_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ShieldRegenMaxFraction(0.1), true);
        assert_eq!(scaled, AbilityEffect::ShieldRegenMaxFraction(0.1 * ASSIM));
    }

    #[test]
    fn scale_hull_regen_max_fraction_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::HullRegenMaxFraction(0.05), true);
        assert_eq!(scaled, AbilityEffect::HullRegenMaxFraction(0.05 * ASSIM));
    }

    #[test]
    fn scale_hull_regen_prev_round_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::HullRegenPrevRoundFraction(0.1), true);
        assert_eq!(scaled, AbilityEffect::HullRegenPrevRoundFraction(0.1 * ASSIM));
    }

    #[test]
    fn scale_shield_regen_prev_round_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ShieldRegenPrevRoundFraction(0.05), true);
        assert_eq!(scaled, AbilityEffect::ShieldRegenPrevRoundFraction(0.05 * ASSIM));
    }

    #[test]
    fn scale_isolytic_damage_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::IsolyticDamageBonus(0.2), true);
        assert_eq!(scaled, AbilityEffect::IsolyticDamageBonus(0.2 * ASSIM));
    }

    #[test]
    fn scale_isolytic_defense_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::IsolyticDefenseBonus(0.15), true);
        assert_eq!(scaled, AbilityEffect::IsolyticDefenseBonus(0.15 * ASSIM));
    }

    #[test]
    fn scale_isolytic_cascade_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::IsolyticCascadeDamageBonus(0.1), true);
        assert_eq!(scaled, AbilityEffect::IsolyticCascadeDamageBonus(0.1 * ASSIM));
    }

    #[test]
    fn scale_shield_mitigation_bonus_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ShieldMitigationBonus(0.3), true);
        assert_eq!(scaled, AbilityEffect::ShieldMitigationBonus(0.3 * ASSIM));
    }

    #[test]
    fn scale_on_kill_hull_regen_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::OnKillHullRegen(1000.0), true);
        assert_eq!(scaled, AbilityEffect::OnKillHullRegen(1000.0 * ASSIM));
    }

    #[test]
    fn scale_mitigation_additive_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::MitigationAdditive(0.1), true);
        assert_eq!(scaled, AbilityEffect::MitigationAdditive(0.1 * ASSIM));
    }

    #[test]
    fn scale_crit_chance_bonus_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::CritChanceBonus(0.1), true);
        assert_eq!(scaled, AbilityEffect::CritChanceBonus(0.1 * ASSIM));
    }

    // ── scale_effect: chance-based variants ──

    #[test]
    fn scale_assimilated_chance_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::Assimilated { chance: 0.5, duration_rounds: 3 }, true);
        assert_eq!(scaled, AbilityEffect::Assimilated { chance: 0.5 * ASSIM, duration_rounds: 3 });
    }

    #[test]
    fn scale_hull_breach_chance_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::HullBreach { chance: 0.6, duration_rounds: 2, requires_critical: true }, true);
        assert_eq!(scaled, AbilityEffect::HullBreach { chance: 0.6 * ASSIM, duration_rounds: 2, requires_critical: true });
    }

    #[test]
    fn scale_burning_chance_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::Burning { chance: 0.7, duration_rounds: 3 }, true);
        assert_eq!(scaled, AbilityEffect::Burning { chance: 0.7 * ASSIM, duration_rounds: 3 });
    }

    #[test]
    fn scale_shots_bonus_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ShotsBonus { chance: 1.0, bonus_pct: 0.2, duration_rounds: 5 }, true);
        assert_eq!(scaled, AbilityEffect::ShotsBonus { chance: 1.0 * ASSIM, bonus_pct: 0.2 * ASSIM, duration_rounds: 5 });
    }

    #[test]
    fn scale_proc_attack_multiplier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ProcAttackMultiplier { chance: 0.4, multiplier: 1.5 }, true);
        assert_eq!(scaled, AbilityEffect::ProcAttackMultiplier { chance: 0.4 * ASSIM, multiplier: 1.5 });
    }

    #[test]
    fn scale_proc_pierce_bonus_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::ProcPierceBonus { chance: 0.3, bonus: 0.1 }, true);
        assert_eq!(scaled, AbilityEffect::ProcPierceBonus { chance: 0.3 * ASSIM, bonus: 0.1 * ASSIM });
    }

    // ── scale_effect: multiplicative variants (1 + (x-1) * ASSIM) ──

    #[test]
    fn scale_decaying_attack_multiplier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::DecayingAttackMultiplier { initial: 1.5, decay_per_round: 0.05, floor: 1.0 }, true);
        // initial: 1.0 + (1.5 - 1.0) * 0.75 = 1.0 + 0.375 = 1.375
        assert_eq!(scaled, AbilityEffect::DecayingAttackMultiplier { initial: 1.375, decay_per_round: 0.05, floor: 1.0 });
    }

    #[test]
    fn scale_accumulating_attack_multiplier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::AccumulatingAttackMultiplier { initial: 1.2, growth_per_round: 0.1, ceiling: 3.0 }, true);
        // initial: 1.0 + (1.2 - 1.0) * 0.75 = 1.0 + 0.15 = 1.15
        assert_eq!(scaled, AbilityEffect::AccumulatingAttackMultiplier { initial: 1.15, growth_per_round: 0.1, ceiling: 3.0 });
    }

    #[test]
    fn scale_crit_damage_multiplier_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::CritDamageMultiplier(1.5), true);
        // 1.0 + (1.5 - 1.0) * 0.75 = 1.375
        assert_eq!(scaled, AbilityEffect::CritDamageMultiplier(1.375));
    }

    #[test]
    fn scale_galaxy_additive_weapon_damage_growth_under_assimilate() {
        let scaled = scale_effect(AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { growth_per_round: 0.02, ceiling: 0.5 }, true);
        assert_eq!(scaled, AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { growth_per_round: 0.02 * ASSIM, ceiling: 0.5 });
    }

    // ── scale_effect: NEVER-scaled exceptions ──

    #[test]
    fn scale_hostile_crit_damage_reduction_never_scaled() {
        let original = AbilityEffect::HostileCritDamageReduction { reduction: 0.05, duration_rounds: 3 };
        let scaled = scale_effect(original, true);
        assert_eq!(scaled, original); // identity
    }

    #[test]
    fn scale_cumulative_opponent_shield_mitigation_debuff_never_scaled() {
        let original = AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round: 0.02, cap: 0.1 };
        let scaled = scale_effect(original, true);
        assert_eq!(scaled, original); // identity
    }

    #[test]
    fn scale_conqueror_borg_beam_suppression_never_scaled() {
        let original = AbilityEffect::ConquerorBorgBeamSuppression;
        let scaled = scale_effect(original, true);
        assert_eq!(scaled, original); // identity
    }

    // ── EffectAccumulator: default state ──

    #[test]
    fn default_accumulator_has_zero_values() {
        let acc = EffectAccumulator::default();
        assert!((acc.pre_attack_multiplier() - 1.0).abs() < 1e-12);
        assert!((acc.pre_attack_pierce_bonus() - 0.0).abs() < 1e-12);
        assert!((acc.defense_mitigation_bonus() - 0.0).abs() < 1e-12);
        assert!((acc.crit_chance_bonus() - 0.0).abs() < 1e-12);
        assert!((acc.crit_damage_multiplier() - 1.0).abs() < 1e-12);
    }

    // ── EffectAccumulator: add_effect routing ──

    #[test]
    fn combat_begin_attack_multiplier_goes_to_pre_attack_modifier() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::AttackMultiplier(0.2), 100.0, 1, None);
        // pre_attack_multiplier = 1.0 + 0.2 = 1.2
        assert!((acc.pre_attack_multiplier() - 1.2).abs() < 1e-12);
    }

    #[test]
    fn combat_begin_pierce_bonus_goes_to_pre_attack_pierce_stack() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::PierceBonus(0.15), 100.0, 1, None);
        assert!((acc.pre_attack_pierce_bonus() - 0.15).abs() < 1e-12);
    }

    #[test]
    fn combat_begin_shield_regen_goes_to_shield_regen_stack() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::ShieldRegen(100.0), 100.0, 1, None);
        assert!((acc.composed_shield_regen() - 100.0).abs() < 1e-12);
    }

    #[test]
    fn combat_begin_apex_shred_goes_to_apex_shred_stack() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::ApexShredBonus(0.2), 100.0, 1, None);
        assert!((acc.composed_apex_shred_bonus() - 0.2).abs() < 1e-12);
    }

    #[test]
    fn combat_begin_decaying_attack_multiplier_adds_to_pre_mod() {
        let mut acc = EffectAccumulator::default();
        // round 3: initial 1.3 - 3*0.05 = 1.15  → modifier = 1.15 - 1.0 = 0.15
        acc.add_effect(TimingWindow::CombatBegin,
            AbilityEffect::DecayingAttackMultiplier { initial: 1.3, decay_per_round: 0.05, floor: 1.0 },
            100.0, 3, None);
        assert!((acc.pre_attack_multiplier() - 1.15).abs() < 1e-12);
    }

    #[test]
    fn combat_begin_accumulating_attack_multiplier_adds_to_pre_mod() {
        let mut acc = EffectAccumulator::default();
        // round 5: initial 1.0 + 5*0.04 = 1.2 → modifier = 1.2 - 1.0 = 0.2
        acc.add_effect(TimingWindow::CombatBegin,
            AbilityEffect::AccumulatingAttackMultiplier { initial: 1.0, growth_per_round: 0.04, ceiling: 2.0 },
            100.0, 5, None);
        assert!((acc.pre_attack_multiplier() - 1.2).abs() < 1e-12);
    }

    // ── PierceBonus at AttackPhase → value * base_attack * 0.5 ──

    #[test]
    fn attack_phase_pierce_bonus_converts_to_attack_phase_damage_flat() {
        let mut acc = EffectAccumulator::default();
        // base_attack = 1000, pierce_bonus = 0.2 → flat = 0.2 * 1000 * 0.5 = 100.0
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::PierceBonus(0.2), 1000.0, 1, None);
        // compose_attack_phase_damage(base=0) = 0 * (1+mod) + flat = flat
        let dmg = acc.compose_attack_phase_damage(0.0);
        assert!((dmg - 100.0).abs() < 1e-12);
    }

    #[test]
    fn attack_phase_pierce_bonus_zero_base_attack_yields_zero_flat() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::PierceBonus(0.2), 0.0, 1, None);
        let dmg = acc.compose_attack_phase_damage(0.0);
        assert!((dmg - 0.0).abs() < 1e-12);
    }

    // ── DefensePhase routing ──

    #[test]
    fn defense_phase_attack_multiplier_goes_to_defense_mitigation_bonus() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::DefensePhase, AbilityEffect::AttackMultiplier(0.1), 100.0, 1, None);
        assert!((acc.defense_mitigation_bonus() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn defense_phase_pierce_bonus_goes_to_defense_mitigation_bonus() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::DefensePhase, AbilityEffect::PierceBonus(0.05), 100.0, 1, None);
        assert!((acc.defense_mitigation_bonus() - 0.05).abs() < 1e-12);
    }

    // ── CritChanceBonus / CritDamageMultiplier early return ──

    #[test]
    fn combat_begin_crit_chance_bonus_adds_to_crit_chance_bonus() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::CritChanceBonus(0.05), 100.0, 1, None);
        assert!((acc.crit_chance_bonus() - 0.05).abs() < 1e-12);
        // adding another
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::CritChanceBonus(0.03), 100.0, 1, None);
        assert!((acc.crit_chance_bonus() - 0.08).abs() < 1e-12);
    }

    #[test]
    fn attack_phase_crit_damage_multiplier_goes_to_crit_damage_multiplier() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritDamageMultiplier(1.2), 100.0, 1, None);
        assert!((acc.crit_damage_multiplier() - 1.2).abs() < 1e-12);
        // chaining
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritDamageMultiplier(1.1), 100.0, 1, None);
        assert!((acc.crit_damage_multiplier() - 1.32).abs() < 1e-12);
    }

    // ── round_end routing ──

    #[test]
    fn round_end_attack_multiplier_adds_to_round_end_mod() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::RoundEnd, AbilityEffect::AttackMultiplier(0.3), 100.0, 1, None);
        // compose_round_end_damage(100) = 100 * (1 + 0.3) + 0 = 130
        let dmg = acc.compose_round_end_damage(100.0);
        assert!((dmg - 130.0).abs() < 1e-12);
    }

    #[test]
    fn round_end_pierce_bonus_goes_to_round_end_damage_stack() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::RoundEnd, AbilityEffect::PierceBonus(50.0), 100.0, 1, None);
        // 0 * (1+mod) + 50 = 50
        let dmg = acc.compose_round_end_damage(0.0);
        assert!((dmg - 50.0).abs() < 1e-12);
    }

    // ── clear / merge ──

    #[test]
    fn clear_resets_all_fields() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::AttackMultiplier(0.2), 100.0, 1, None);
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritChanceBonus(0.1), 100.0, 1, None);
        acc.clear();
        assert!((acc.pre_attack_multiplier() - 1.0).abs() < 1e-12);
        assert!((acc.crit_chance_bonus() - 0.0).abs() < 1e-12);
        assert!((acc.crit_damage_multiplier() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn merge_from_combines_stacks_and_fields() {
        let mut a = EffectAccumulator::default();
        a.add_effect(TimingWindow::CombatBegin, AbilityEffect::AttackMultiplier(0.1), 100.0, 1, None);
        a.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritChanceBonus(0.05), 100.0, 1, None);

        let mut b = EffectAccumulator::default();
        b.add_effect(TimingWindow::CombatBegin, AbilityEffect::PierceBonus(0.2), 100.0, 1, None);
        b.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritDamageMultiplier(1.1), 100.0, 1, None);

        a.merge_from(&b);
        // pre_attack_multiplier overwritten by b = 1.0
        assert!((a.pre_attack_multiplier() - 1.0).abs() < 1e-12);
        // pre_attack_pierce_stack merged: has b's pierce
        assert!((a.pre_attack_pierce_bonus() - 0.2).abs() < 1e-12);
        // crit_chance_bonus: a's 0.05 + b's 0.0 = 0.05
        assert!((a.crit_chance_bonus() - 0.05).abs() < 1e-12);
        // crit_damage_multiplier: 1.0 * 1.1 = 1.1
        assert!((a.crit_damage_multiplier() - 1.1).abs() < 1e-12);
    }

    #[test]
    fn merge_carry_additive_adds_instead_of_overwrites() {
        let mut base = EffectAccumulator::default();
        base.add_effect(TimingWindow::CombatBegin, AbilityEffect::AttackMultiplier(0.1), 100.0, 1, None);

        let mut carry = EffectAccumulator::default();
        carry.add_effect(TimingWindow::AfterSubround, AbilityEffect::AttackMultiplier(0.05), 100.0, 1, None);
        carry.add_effect(TimingWindow::AttackPhase, AbilityEffect::CritChanceBonus(0.03), 100.0, 1, None);

        base.merge_carry_additive(&carry);
        // AttackMultiplier routed to attack_phase_mod by AfterSubround timing → trace_add_attack_phase_mod
        // Actually, AfterSubround AttackMultiplier goes to trace_add_attack_phase_mod
        // Then merge_carry_additive adds attack_phase_damage_modifier_sum
        // pre_attack_modifier_sum should still be 0.1 from base, since AfterSubround adds to attack_phase_mod
        let dmg = base.compose_attack_phase_damage(100.0);
        // 100 * (1 + 0.05) + 0 = 105
        assert!((dmg - 105.0).abs() < 1e-12);
        assert!((base.crit_chance_bonus() - 0.03).abs() < 1e-12);
    }

    // ── clear_shield_hull_regen_stacks ──

    #[test]
    fn clear_regen_stacks_removes_regen_keys() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::ShieldRegen(100.0), 100.0, 1, None);
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::HullRegen(50.0), 100.0, 1, None);
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::ShieldRegenMaxFraction(0.1), 100.0, 1, None);
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::HullRegenMaxFraction(0.05), 100.0, 1, None);
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::ApexShredBonus(0.2), 100.0, 1, None); // not a regen key

        acc.clear_shield_hull_regen_stacks();
        assert!((acc.composed_shield_regen() - 0.0).abs() < 1e-12);
        assert!((acc.composed_hull_regen() - 0.0).abs() < 1e-12);
        assert!((acc.composed_shield_regen_max_fraction() - 0.0).abs() < 1e-12);
        assert!((acc.composed_hull_regen_max_fraction() - 0.0).abs() < 1e-12);
        // non-regen key preserved
        assert!((acc.composed_apex_shred_bonus() - 0.2).abs() < 1e-12);
    }

    // ── sum_on_kill_hull_regen ──

    #[test]
    fn sum_on_kill_hull_regen_sums_values() {
        let effects = vec![
            make_active(AbilityEffect::OnKillHullRegen(500.0)),
            make_active(AbilityEffect::OnKillHullRegen(300.0)),
        ];
        let total = sum_on_kill_hull_regen(&effects, false);
        assert!((total - 800.0).abs() < 1e-12);
    }

    #[test]
    fn sum_on_kill_hull_regen_ignores_non_on_kill_effects() {
        let effects = vec![
            make_active(AbilityEffect::OnKillHullRegen(500.0)),
            make_active(AbilityEffect::HullRegen(300.0)),
        ];
        let total = sum_on_kill_hull_regen(&effects, false);
        assert!((total - 500.0).abs() < 1e-12);
    }

    #[test]
    fn sum_on_kill_hull_regen_scales_with_assimilate() {
        let effects = vec![make_active(AbilityEffect::OnKillHullRegen(1000.0))];
        let total = sum_on_kill_hull_regen(&effects, true);
        assert!((total - 1000.0 * ASSIMILATED_EFFECTIVENESS_MULTIPLIER).abs() < 1e-12);
    }

    // ── CumulativeOpponentShieldMitigationDebuff at RoundStart ──

    #[test]
    fn cumulative_shield_mitigation_debuff_at_round_start() {
        let mut acc = EffectAccumulator::default();
        // round 5: per_round=0.02 * 5 = 0.1, cap=0.15 → debuff=0.1 applied as negative flat to ShieldMitigationBonus
        acc.add_effect(TimingWindow::RoundStart,
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round: 0.02, cap: 0.15 },
            100.0, 5, None);
        // composed = base(0) * (1 + modifier(0)) + flat(-0.1) = -0.1
        assert!((acc.composed_shield_mitigation_bonus() + 0.1).abs() < 1e-12);
    }

    // ── GalaxyAdditiveWeaponDamageGrowth at RoundStart ──

    #[test]
    fn galaxy_additive_weapon_damage_growth_at_round_start() {
        let mut acc = EffectAccumulator::default();
        // round 4: growth = 4 * 0.02 = 0.08, ceiling 0.5 → g = 0.08
        acc.add_effect(TimingWindow::RoundStart,
            AbilityEffect::GalaxyAdditiveWeaponDamageGrowth { growth_per_round: 0.02, ceiling: 0.5 },
            100.0, 4, None);
        assert!((acc.galaxy_additive_weapon_frac() - 0.08).abs() < 1e-12);
    }

    // ── set_pre_attack_damage_base ──

    #[test]
    fn set_pre_attack_damage_base_then_compose() {
        let mut acc = EffectAccumulator::default();
        acc.set_pre_attack_damage_base(1000.0);
        assert!((acc.composed_pre_attack_damage() - 1000.0).abs() < 1e-12);
        // apply a modifier on top
        acc.add_effect(TimingWindow::CombatBegin, AbilityEffect::AttackMultiplier(0.2), 1000.0, 1, None);
        // pre_attack_damage is in StatStacking, AttackMultiplier goes to pre_attack_modifier_sum
        // But set_pre_attack_damage_base adds to StatStacking, not to pre_attack_modifier_sum
        // So composed_pre_attack_damage should still be 1000 (stacking base)
        assert!((acc.composed_pre_attack_damage() - 1000.0).abs() < 1e-12);
    }

    // ── PreAttackDamage carry to AttackPhaseDamage ──

    #[test]
    fn compose_attack_phase_damage_uses_correct_formula() {
        let mut acc = EffectAccumulator::default();
        // AttackPhase PierceBonus adds flat to AttackPhaseDamage stack
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::PierceBonus(0.2), 500.0, 1, None);
        // flat = 0.2 * 500 * 0.5 = 50
        // compose_attack_phase_damage(1000) = 1000 * (1 + 0) + 50 = 1050
        let dmg = acc.compose_attack_phase_damage(1000.0);
        assert!((dmg - 1050.0).abs() < 1e-12);
    }

    #[test]
    fn attack_phase_attack_multiplier_applies_to_damage_channel() {
        let mut acc = EffectAccumulator::default();
        acc.add_effect(TimingWindow::AttackPhase, AbilityEffect::AttackMultiplier(0.3), 100.0, 1, None);
        // compose_attack_phase_damage(1000) = 1000 * (1 + 0.3) + 0 = 1300
        let dmg = acc.compose_attack_phase_damage(1000.0);
        assert!((dmg - 1300.0).abs() < 1e-12);
    }
}
