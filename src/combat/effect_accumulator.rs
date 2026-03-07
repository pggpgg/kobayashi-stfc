//! Effect scaling and stacking for the combat loop.

use serde_json::{Map, Value};

use crate::combat::abilities::{AbilityEffect, ActiveAbilityEffect, TimingWindow};
use crate::combat::stacking::{StackContribution, StatStacking};
use crate::combat::types::{
    Combatant, CombatEvent, EventSource, TraceCollector, ASSIMILATED_EFFECTIVENESS_MULTIPLIER,
};

#[derive(Debug, Clone)]
pub(crate) struct EffectAccumulator {
    stacks: StatStacking<EffectStatKey>,
    pre_attack_modifier_sum: f64,
    attack_phase_damage_modifier_sum: f64,
    round_end_modifier_sum: f64,
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
        stacks.add(StackContribution::base(EffectStatKey::ApexBarrierBonus, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::ShieldRegen, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::HullRegen, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::IsolyticDamageBonus, 0.0));
        stacks.add(StackContribution::base(EffectStatKey::IsolyticDefenseBonus, 0.0));
        stacks.add(StackContribution::base(
            EffectStatKey::IsolyticCascadeDamageBonus,
            0.0,
        ));
        stacks.add(StackContribution::base(EffectStatKey::ShieldMitigationBonus, 0.0));

        Self {
            stacks,
            pre_attack_modifier_sum: 0.0,
            attack_phase_damage_modifier_sum: 0.0,
            round_end_modifier_sum: 0.0,
        }
    }
}

impl EffectAccumulator {
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
    }

    pub(crate) fn merge_from(&mut self, other: &EffectAccumulator) {
        self.stacks.merge_from(&other.stacks);
        self.pre_attack_modifier_sum = other.pre_attack_modifier_sum;
        self.attack_phase_damage_modifier_sum = other.attack_phase_damage_modifier_sum;
        self.round_end_modifier_sum = other.round_end_modifier_sum;
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
            self.add_effect(
                timing,
                scale_effect(effect.effect, assimilated_active),
                base_attack,
                round_index,
            );
        }
    }

    fn add_effect(
        &mut self,
        timing: TimingWindow,
        effect: AbilityEffect,
        base_attack: f64,
        round_index: u32,
    ) {
        match timing {
            TimingWindow::CombatBegin | TimingWindow::RoundStart => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.pre_attack_modifier_sum += modifier;
                }
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::PreAttackPierceBonus,
                    value,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexShredBonus, v));
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexBarrierBonus, v));
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDamageBonus, v));
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDefenseBonus, v));
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                    ));
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                    ));
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.pre_attack_modifier_sum += value - 1.0;
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.pre_attack_modifier_sum += value - 1.0;
                }
            },
            TimingWindow::AttackPhase => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.attack_phase_damage_modifier_sum += modifier;
                }
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::AttackPhaseDamage,
                    value * base_attack * 0.5,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexShredBonus, v));
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexBarrierBonus, v));
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDamageBonus, v));
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDefenseBonus, v));
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                    ));
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                    ));
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.attack_phase_damage_modifier_sum += value - 1.0;
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.attack_phase_damage_modifier_sum += value - 1.0;
                }
            },
            TimingWindow::DefensePhase => match effect {
                AbilityEffect::AttackMultiplier(modifier) => self.stacks.add(
                    StackContribution::flat(EffectStatKey::DefenseMitigationBonus, modifier),
                ),
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::DefenseMitigationBonus,
                    value,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(_) => {}
                AbilityEffect::HullRegen(_) => {}
                AbilityEffect::ApexShredBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexShredBonus, v));
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexBarrierBonus, v));
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDamageBonus, v));
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDefenseBonus, v));
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                    ));
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                    ));
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::DecayingAttackMultiplier { .. }
                | AbilityEffect::AccumulatingAttackMultiplier { .. } => {}
            },
            TimingWindow::RoundEnd => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.round_end_modifier_sum += modifier;
                }
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::RoundEndDamage,
                    value,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldRegen, v));
                }
                AbilityEffect::HullRegen(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::HullRegen, v));
                }
                AbilityEffect::ApexShredBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexShredBonus, v));
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexBarrierBonus, v));
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDamageBonus, v));
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDefenseBonus, v));
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                    ));
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                    ));
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.round_end_modifier_sum += value - 1.0;
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.round_end_modifier_sum += value - 1.0;
                }
            },
            TimingWindow::ShieldBreak
            | TimingWindow::Kill
            | TimingWindow::HullBreach
            | TimingWindow::ReceiveDamage
            | TimingWindow::CombatEnd => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.pre_attack_modifier_sum += modifier;
                }
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::PreAttackPierceBonus,
                    value,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
                AbilityEffect::ShotsBonus { .. } => {}
                AbilityEffect::ShieldRegen(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldRegen, v));
                }
                AbilityEffect::HullRegen(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::HullRegen, v));
                }
                AbilityEffect::ApexShredBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexShredBonus, v));
                }
                AbilityEffect::ApexBarrierBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ApexBarrierBonus, v));
                }
                AbilityEffect::IsolyticDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDamageBonus, v));
                }
                AbilityEffect::IsolyticDefenseBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::IsolyticDefenseBonus, v));
                }
                AbilityEffect::IsolyticCascadeDamageBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::IsolyticCascadeDamageBonus,
                        v,
                    ));
                }
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(
                        EffectStatKey::ShieldMitigationBonus,
                        v,
                    ));
                }
                AbilityEffect::OnKillHullRegen(_) => {}
                AbilityEffect::DecayingAttackMultiplier {
                    initial,
                    decay_per_round,
                    floor,
                } => {
                    let r = round_index as f64;
                    let value = (initial - r * decay_per_round).max(floor);
                    self.pre_attack_modifier_sum += value - 1.0;
                }
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial,
                    growth_per_round,
                    ceiling,
                } => {
                    let r = round_index as f64;
                    let value = (initial + r * growth_per_round).min(ceiling);
                    self.pre_attack_modifier_sum += value - 1.0;
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
    }
}
