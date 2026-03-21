#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityClass {
    CaptainManeuver,
    BridgeAbility,
    BelowDeck,
    /// Ship hull ability (e.g. when hit, increase armor/shield piercing). Evaluated per round like officer abilities.
    ShipAbility,
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
    /// Virtual seat for ship hull abilities (from data.stfc.space ability array). Not officer-driven.
    Ship,
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
    /// Increase shots per weapon for a duration. Formula: n_w(r) = RoundHalfEven(n_w0 * (1 + B_shots)); this effect adds to B_shots when it triggers.
    /// chance: 1.0 = deterministic (e.g. "at start of each round, +X% shots for Y rounds").
    ShotsBonus {
        chance: f64,
        bonus_pct: f64,
        duration_rounds: u32,
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

/// Sentinel batch id: legacy or non-officer contexts group by consecutive matching [CrewSeatContext::officer_id].
pub const NO_EXPLICIT_CONTRIBUTION_BATCH: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct CrewSeatContext {
    pub seat: CrewSeat,
    pub ability: Ability,
    pub boosted: bool,
    /// Canonical officer id when this row comes from an officer slot (LCARS / scenario resolution).
    pub officer_id: Option<String>,
    /// Rows from one officer **slot** share the same batch (captain, one bridge slot, one below slot).
    /// Use [NO_EXPLICIT_CONTRIBUTION_BATCH] for ship abilities or hand-built tests without batch metadata.
    pub contribution_batch: u32,
}

impl CrewSeatContext {
    /// Crew row without officer/batch metadata (tests, ship abilities, legacy name-based crew).
    pub fn legacy(seat: CrewSeat, ability: Ability, boosted: bool) -> Self {
        Self {
            seat,
            ability,
            boosted,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }
    }
}

/// Drop later seat groups that share an `officer_id` with an earlier group (defense in depth if a
/// crew row set ever contains duplicates). Grouping uses `contribution_batch` when set; otherwise
/// consecutive rows with the same `officer_id` form one group. Rows with `officer_id: None` are
/// never dropped.
pub fn apply_duplicate_officer_policy(crew: &CrewConfiguration) -> CrewConfiguration {
    if crew.seats.is_empty() {
        return crew.clone();
    }

    let seats = &crew.seats;
    let mut out = Vec::with_capacity(seats.len());
    let mut seen_officers: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut i = 0usize;

    while i < seats.len() {
        let batch = seats[i].contribution_batch;
        let j = if batch != NO_EXPLICIT_CONTRIBUTION_BATCH {
            let mut j = i + 1;
            while j < seats.len() && seats[j].contribution_batch == batch {
                j += 1;
            }
            j
        } else if seats[i].officer_id.is_none() {
            i + 1
        } else {
            let oid = seats[i].officer_id.as_deref().unwrap();
            let mut j = i + 1;
            while j < seats.len()
                && seats[j].contribution_batch == NO_EXPLICIT_CONTRIBUTION_BATCH
                && seats[j].officer_id.as_deref() == Some(oid)
            {
                j += 1;
            }
            j
        };

        let group = &seats[i..j];
        let include = match group.first().and_then(|s| s.officer_id.as_deref()) {
            Some(oid) => {
                if seen_officers.contains(oid) {
                    false
                } else {
                    seen_officers.insert(oid.to_string());
                    true
                }
            }
            None => true,
        };
        if include {
            out.extend_from_slice(group);
        }
        i = j;
    }

    CrewConfiguration { seats: out }
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
            Self::ShipAbility => CrewSeat::Ship,
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
