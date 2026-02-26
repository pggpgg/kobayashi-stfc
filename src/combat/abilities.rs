#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityClass {
    CaptainManeuver,
    BridgeAbility,
    BelowDeck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingWindow {
    CombatBegin,
    RoundStart,
    AttackPhase,
    DefensePhase,
    RoundEnd,
    /// When target's shields reach 0 (on_shield_break).
    ShieldBreak,
    /// When this ship destroys a target (on_kill).
    Kill,
    /// When target's hull drops below threshold (on_hull_breach).
    HullBreach,
    /// When this ship takes damage (on_receive_damage).
    ReceiveDamage,
    /// Once after fight resolves (on_combat_end).
    CombatEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrewSeat {
    Captain,
    Bridge,
    BelowDeck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbilityEffect {
    AttackMultiplier(f64),
    PierceBonus(f64),
    Morale(f64),
    Assimilated {
        chance: f64,
        duration_rounds: u32,
    },
    HullBreach {
        chance: f64,
        duration_rounds: u32,
        requires_critical: bool,
    },
    Burning {
        chance: f64,
        duration_rounds: u32,
    },
    /// Shield HP restored per round (round end). Flat value.
    ShieldRegen(f64),
    /// Hull HP restored per round (round end). Reduces effective hull damage taken.
    HullRegen(f64),
    /// Officer-granted Apex Shred; value is decimal (0.15 = +15%).
    ApexShredBonus(f64),
    /// Officer-granted Apex Barrier; value is flat integer (e.g. 1000).
    ApexBarrierBonus(f64),
    /// Officer-granted isolytic damage bonus (decimal, e.g. 0.1 = +10%).
    IsolyticDamageBonus(f64),
    /// Officer-granted isolytic defense; flat reduction to isolytic damage taken.
    IsolyticDefenseBonus(f64),
    /// Officer-granted isolytic cascade damage bonus (decimal). Multiplied by (1 + isolytic_damage_bonus) in isolytic_damage().
    IsolyticCascadeDamageBonus(f64),
    /// Officer-granted shield mitigation; additive to base (clamped 0..1).
    ShieldMitigationBonus(f64),
    /// Hull HP restored when this ship gets a kill (on_kill). Reduces total_attacker_hull_damage.
    OnKillHullRegen(f64),
    /// Attack multiplier that decays each round. initial - round * decay_per_round, floored.
    DecayingAttackMultiplier {
        initial: f64,
        decay_per_round: f64,
        floor: f64,
    },
    /// Attack multiplier that accumulates each round. initial + round * growth_per_round, ceiling.
    AccumulatingAttackMultiplier {
        initial: f64,
        growth_per_round: f64,
        ceiling: f64,
    },
}

/// Combat context for condition evaluation at runtime.
#[derive(Debug, Clone)]
pub struct CombatContext {
    pub round_index: u32,
    pub defender_hull_pct: f64,
    pub defender_shield_pct: f64,
    pub attacker_hull_pct: f64,
    pub attacker_shield_pct: f64,
}

/// Condition that gates effect activation. Evaluated at runtime in the combat loop.
#[derive(Debug, Clone, PartialEq)]
pub enum AbilityCondition {
    StatBelow { stat: String, threshold_pct: f64 },
    StatAbove { stat: String, threshold_pct: f64 },
    RoundRange { min: u32, max: u32 },
    And(Vec<AbilityCondition>),
    Or(Vec<AbilityCondition>),
}

impl AbilityCondition {
    pub fn evaluate(&self, ctx: &CombatContext) -> bool {
        match self {
            Self::StatBelow { stat, threshold_pct } => {
                let pct = match stat.as_str() {
                    "shield_hp" | "shield" => ctx.defender_shield_pct,
                    "hull_hp" | "hull" => ctx.defender_hull_pct,
                    "attacker_shield_hp" => ctx.attacker_shield_pct,
                    "attacker_hull_hp" => ctx.attacker_hull_pct,
                    _ => return false,
                };
                pct < *threshold_pct
            }
            Self::StatAbove { stat, threshold_pct } => {
                let pct = match stat.as_str() {
                    "shield_hp" | "shield" => ctx.defender_shield_pct,
                    "hull_hp" | "hull" => ctx.defender_hull_pct,
                    "attacker_shield_hp" => ctx.attacker_shield_pct,
                    "attacker_hull_hp" => ctx.attacker_hull_pct,
                    _ => return false,
                };
                pct > *threshold_pct
            }
            Self::RoundRange { min, max } => ctx.round_index >= *min && ctx.round_index <= *max,
            Self::And(conds) => conds.iter().all(|c| c.evaluate(ctx)),
            Self::Or(conds) => conds.iter().any(|c| c.evaluate(ctx)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ability {
    pub name: String,
    pub class: AbilityClass,
    pub timing: TimingWindow,
    pub boostable: bool,
    pub effect: AbilityEffect,
    pub condition: Option<AbilityCondition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrewSeatContext {
    pub seat: CrewSeat,
    pub ability: Ability,
    pub boosted: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrewConfiguration {
    pub seats: Vec<CrewSeatContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAbilityEffect {
    pub ability_name: String,
    pub effect: AbilityEffect,
    pub boosted: bool,
    pub condition: Option<AbilityCondition>,
}

impl AbilityClass {
    pub const fn allowed_seat(self) -> CrewSeat {
        match self {
            Self::CaptainManeuver => CrewSeat::Captain,
            Self::BridgeAbility => CrewSeat::Bridge,
            Self::BelowDeck => CrewSeat::BelowDeck,
        }
    }
}

pub fn can_activate_in_seat(context: &CrewSeatContext) -> bool {
    context.seat == context.ability.class.allowed_seat()
        && (context.ability.boostable || !context.boosted)
}

pub fn active_effects_for_timing(
    crew: &CrewConfiguration,
    timing: TimingWindow,
) -> Vec<ActiveAbilityEffect> {
    crew.seats
        .iter()
        .filter(|seat_context| {
            can_activate_in_seat(seat_context) && seat_context.ability.timing == timing
        })
        .map(|seat_context| ActiveAbilityEffect {
            ability_name: seat_context.ability.name.clone(),
            effect: seat_context.ability.effect,
            boosted: seat_context.boosted,
            condition: seat_context.ability.condition.clone(),
        })
        .collect()
}

/// Filter effects by condition. Effects without a condition always pass.
pub fn filter_effects_by_condition(
    effects: &[ActiveAbilityEffect],
    ctx: &CombatContext,
) -> Vec<ActiveAbilityEffect> {
    effects
        .iter()
        .filter(|e| {
            e.condition
                .as_ref()
                .map_or(true, |c| c.evaluate(ctx))
        })
        .cloned()
        .collect()
}
