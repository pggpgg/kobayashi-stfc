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
        self.attack_phase_damage_modifier_sum = 0.0;
        self.round_end_modifier_sum = 0.0;
        self.crit_chance_bonus = 0.0;
        self.crit_damage_multiplier = 1.0;
        self.contribution_lines.clear();
    }

    pub(crate) fn merge_from(&mut self, other: &EffectAccumulator) {
        self.stacks.merge_from(&other.stacks);
        self.pre_attack_modifier_sum = other.pre_attack_modifier_sum;
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
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
                AbilityEffect::CritChanceBonus(_) => {}
                AbilityEffect::CritDamageMultiplier(_) => {}
                AbilityEffect::DecayingAttackMultiplier { .. }
                | AbilityEffect::AccumulatingAttackMultiplier { .. } => {}
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
                AbilityEffect::HullRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegen,
                        v,
                        timing,
                        source,
                        "HullRegen",
                    );
                }
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
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
                AbilityEffect::HullRegen(v) => {
                    self.add_stack_flat_traced(
                        EffectStatKey::HullRegen,
                        v,
                        timing,
                        source,
                        "HullRegen",
                    );
                }
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
                AbilityEffect::HostileCritDamageReduction { .. } => {}
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
        AbilityEffect::HullRegen(v) => {
            AbilityEffect::HullRegen(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
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
        AbilityEffect::HostileCritDamageReduction { .. } => effect,
    }
}
