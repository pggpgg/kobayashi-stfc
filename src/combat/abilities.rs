use crate::combat::types::{EnemyType, EnemyTypes, OpponentFactionTag, ShipType};

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
    /// After each weapon sub-round (outbound shots for that weapon and defender counter); stacks carry into later weapons this round only.
    AfterSubround,
    DefensePhase,
    RoundEnd,
    /// When the **defender's** (enemy's) shields reach 0 — e.g. `on_enemy_shield_break`, or legacy
    /// `on_shield_break` with LCARS `target: enemy` (Yan'Agh-style).
    ShieldBreak,
    /// When **this crew's ship** (the attacker in PvE) loses shields — counter-fire and similar.
    /// LCARS: `on_own_shield_break` / `on_shield_break` with `target: self` (Mudd, Vemet, …).
    SelfShieldBreak,
    /// When this ship destroys a target (on_kill).
    Kill,
    /// When the defender **enters** the hull-breached state from a [`AbilityEffect::HullBreach`] proc
    /// (`on_hull_breach` timing in LCARS). Not tied to a hull HP fraction threshold.
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
    /// Chance-gated multiplier that applies to this combatant's shot calculation when evaluated.
    /// Used for hostile-side (defender) upstream abilities on return fire.
    ProcAttackMultiplier {
        chance: f64,
        multiplier: f64,
    },
    /// Chance-gated pierce bonus that applies when evaluated.
    /// Used for hostile-side (defender) upstream abilities on return fire.
    ProcPierceBonus {
        chance: f64,
        bonus: f64,
    },
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
    /// Shield HP restored as a **flat** value. [`TimingWindow::CombatBegin`] / [`TimingWindow::RoundStart`]
    /// entries are applied at the **start** of each combat round; other timings that feed the round
    /// accumulator (e.g. [`TimingWindow::RoundEnd`], [`TimingWindow::ReceiveDamage`]) apply at **round end**.
    ShieldRegen(f64),
    /// Hull HP restored as a **flat** value (reduces cumulative hull damage taken on that ship).
    /// Same timing split as [`AbilityEffect::ShieldRegen`].
    HullRegen(f64),
    /// At [`TimingWindow::RoundStart`], heal `fraction ×` hull damage the attacker **took** in the
    /// immediately preceding combat round (gross incoming hull from counter-fire, hostile round-end
    /// hull effects, burning ticks, etc.; excludes prior heals). Round 1 has no prior round, so no
    /// heal. `fraction` is typically 0..1; multiple effects **sum** before multiplying (capped at 1).
    HullRegenPrevRoundFraction(f64),
    /// At [`TimingWindow::RoundStart`], restore shield HP `fraction ×` shield damage the attacker **took**
    /// in the immediately preceding combat round (gross shield loss on counter-fire, etc.). Round 1: none.
    /// Multiple effects **sum** (capped at 1) before multiplying gross shield damage.
    ShieldRegenPrevRoundFraction(f64),
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
    /// Additive fraction merged into the **player** ship’s mitigation when the hostile returns fire
    /// (counter-attack path). Used as an LCARS proxy for `armor` / “all defenses” rows that are not
    /// folded into [`Combatant::mitigation`] at scenario build. Values are **mitigation fractions**
    /// in `0..1`; the resolver normalizes sheet-style magnitudes (`> 1`) as percent points (`÷ 100`).
    MitigationAdditive(f64),
    /// Additive critical hit chance for this shot stack (absolute probability, e.g. 0.05 = +5%).
    /// Applied at crit roll after [`Combatant::crit_chance`], then clamped to [0, 1].
    CritChanceBonus(f64),
    /// Multiplicative factor on [`Combatant::crit_multiplier`] for this shot stack when a crit lands.
    /// Values chain as a product (e.g. 1.1 then 1.2 → ×1.32). Ignored when non-finite or ≤ 0.
    CritDamageMultiplier(f64),
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
    /// Galaxy-class hull growth (e.g. U.S.S. Enterprise-D): cumulative fraction `g` (same units as
    /// profile `weapon_damage` bonus `p`) applied as `×(1 + g/(1+p))` on outgoing weapon damage so it
    /// stacks **additively** with research weapon_damage (`∝ 1+p+g`) instead of as another factor in
    /// `pre_attack_multiplier` (`∝ (1+p)(1+g)` when alone).
    GalaxyAdditiveWeaponDamageGrowth {
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
    /// Reduces damage when the **defender** (e.g. hostile) scores a critical hit on return fire.
    /// `reduction` is a fraction (0.02 = 2% less damage on that crit). Applied as `crit_mult *= 1.0 - reduction`.
    /// `duration_rounds`: 1-based combat rounds `1..=duration_rounds` (e.g. Crozier "first 5 rounds").
    HostileCritDamageReduction {
        reduction: f64,
        duration_rounds: u32,
    },
    /// Reduces the **defender's** shield-mitigation fraction cumulatively each round (Slipstream-style).
    /// Engine applies `min(per_round * round_index, cap)` as a negative add to defender shield mitigation.
    CumulativeOpponentShieldMitigationDebuff {
        per_round: f64,
        cap: f64,
    },
    /// Marker: Borg Sphere **Quantum Nullification Pulse** vs Conqueror Borg — disables the
    /// defender’s **Quantum Resonance Beam** (Suppressor) and **Hyperthermic Resonance Beam**
    /// (Obliterator) for instant-loss resolution; see [`crate::combat::conqueror_borg_beams`].
    ConquerorBorgBeamSuppression,
}

/// Combat context for condition evaluation at runtime.
#[derive(Debug, Clone)]
pub struct CombatContext {
    pub round_index: u32,
    pub defender_hull_pct: f64,
    pub defender_shield_pct: f64,
    pub attacker_hull_pct: f64,
    pub attacker_shield_pct: f64,
    /// True after the round-start Morale proc succeeds for this combat round (attacker).
    pub attacker_morale_active: bool,
    /// True when the defender (hostile) still has a Burning duration from the attacker's procs.
    pub defender_burning_active: bool,
    /// True when the defender still has a Hull Breach duration from the attacker's procs.
    pub defender_hull_breach_active: bool,
    /// True when the attacker (player ship) has a Burning duration (e.g. from hostile procs / receive damage).
    pub attacker_burning_active: bool,
    /// True when the attacker still has a Hull Breach duration (e.g. from hostile counter fire).
    pub attacker_hull_breach_active: bool,
    /// True when the defender (opponent) has an active Assimilate self-debuff duration (e.g. PvP vs Borg).
    pub defender_assimilated_active: bool,
    /// Faction of the defending ship (hostile) in PvE; used for "against Klingon" style abilities.
    pub defender_faction: OpponentFactionTag,
    /// Upstream STFC `faction.id` on the defending hostile (canonical `EnemyHullFaction`); `0` when missing or unset (e.g. PvP without metadata).
    pub defender_hull_faction_id: i64,
    /// Hull class of the defending [`crate::combat::Combatant`] (hostile in PvE).
    pub defender_ship_type: ShipType,
    /// Hull class of the attacking [`crate::combat::Combatant`] (player ship in PvE).
    pub attacker_ship_type: ShipType,
    /// Attacking ship id slug (same as [`crate::combat::Combatant::id`], e.g. `uss_voyager`).
    pub attacker_ship_id: String,
    /// True when the defending side is an **NPC hostile** (canonical `EnemyHostile` / ship-vs-hostile optimizer).
    pub defender_is_npc_hostile: bool,
    /// True when the defending side is a **player ship** (PvP-shaped scenarios; canonical `EnemyPlayer`).
    pub defender_is_player_ship: bool,
    /// True when officer Tal ([`TAL_OFFICER_LCARS_ID`]) occupies a Captain or Bridge seat on the attacker crew.
    pub attacker_tal_assigned_captain_or_bridge: bool,
    /// Bitmask of tags on the defending NPC hostile (from [`crate::combat::SimulationConfig::defender_hostile_tag_mask`]).
    pub defender_hostile_tag_mask: u32,
    /// Engagement category tags from [`crate::combat::SimulationConfig::engagement_enemy_types`] (armada solo/group, etc.).
    pub engagement_enemy_types: EnemyTypes,
    /// Optional upstream STFC combat battle-type id for canonical `CombatBattleType` gating.
    /// `None` means unknown/unset for this scenario.
    pub combat_battle_type_id: Option<u32>,
    /// Optional defender level from hostile catalog for canonical `TargetMaxLevel`.
    pub defender_level: Option<u32>,
}

/// Condition that gates effect activation. Evaluated at runtime in the combat loop.
#[derive(Debug, Clone, PartialEq)]
pub enum AbilityCondition {
    StatBelow {
        stat: String,
        threshold_pct: f64,
    },
    StatAbove {
        stat: String,
        threshold_pct: f64,
    },
    RoundRange {
        min: u32,
        max: u32,
    },
    /// True when the attacker succeeded on the primary round-start [AbilityEffect::Morale] roll this round.
    MoraleActive,
    /// True when [CombatContext::defender_burning_active] (opponent has burning state).
    DefenderBurning,
    /// True when [CombatContext::defender_hull_breach_active].
    DefenderHullBreach,
    /// True when [CombatContext::attacker_burning_active] (player ship burning).
    AttackerBurning,
    /// True when [CombatContext::attacker_hull_breach_active] (player ship hull breached).
    AttackerHullBreach,
    /// True when [CombatContext::defender_assimilated_active] (opponent has Assimilate debuff; PvP).
    DefenderAssimilated,
    /// True when the defending hostile’s faction matches (see [`CombatContext::defender_faction`]).
    DefenderFactionIs(OpponentFactionTag),
    /// True when the defending hostile’s upstream `faction.id` matches (canonical `EnemyHullFaction` + `faction_id` in attributes).
    DefenderHullFactionIdIs(i64),
    /// True when the defending ship’s hull class matches (player hull abilities vs a hostile of that class).
    DefenderShipTypeIs(ShipType),
    /// True when the attacking ship’s hull class matches (e.g. hostile hull abilities vs the player’s class).
    AttackerShipTypeIs(ShipType),
    /// True when the attacking ship’s id slug matches (e.g. Borg Alcove bonuses on U.S.S. Voyager only).
    AttackerShipIdIs(String),
    /// True when [`CombatContext::defender_is_npc_hostile`] (opponent is an NPC hostile, not another player).
    DefenderIsNpcHostile,
    /// True when [`CombatContext::defender_is_player_ship`] (opponent is a player ship).
    DefenderIsPlayerShip,
    /// Canonical `SelfOfficerTalNotOnBridge`: true when Tal is not assigned Captain or Bridge on the attacker.
    AttackerOfficerTalNotOnBridge,
    /// Every bit in `required_mask` is set on [`CombatContext::defender_hostile_tag_mask`] (built from AND of catalog tag slugs).
    DefenderHostileTagsAllPresent {
        required_mask: u32,
    },
    /// True when [`CombatContext::engagement_enemy_types`] lists this tag (e.g. group armadas only).
    EngagementIncludes(EnemyType),
    /// True when the engagement battle-type id is in this allow-list.
    /// If battle type is unavailable in context, this condition currently evaluates leniently true.
    CombatBattleTypeAny(Vec<u32>),
    /// True when defender level is <= max_level.
    /// If defender level is unavailable in context, this condition currently evaluates leniently true.
    DefenderLevelAtMost(u32),
    /// Constant used when a canonical token has no dynamic [`CombatContext`] signal yet, but the
    /// Kobayashi **ship-vs-hostile** scenario fixes its truth value (see `docs/CANONICAL_CONDITIONS.md`).
    LiteralBool(bool),
    /// Logical negation of a single sub-condition (LCARS `not`).
    Not(Box<AbilityCondition>),
    And(Vec<AbilityCondition>),
    Or(Vec<AbilityCondition>),
}

impl AbilityCondition {
    /// Delegates to [`crate::combat::condition::evaluate_ability_condition`].
    #[inline]
    pub fn evaluate(&self, ctx: &CombatContext) -> bool {
        crate::combat::condition::evaluate_ability_condition(self, ctx)
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

/// LCARS officer id for Tal (`officers.lcars.yaml`); keep in sync with officer data.
pub const TAL_OFFICER_LCARS_ID: &str = "tal-c3e4eb";

/// True if Tal is assigned in a Captain or Bridge seat (canonical gate for `SelfOfficerTalNotOnBridge`).
pub fn attacker_crew_tal_assigned_captain_or_bridge(crew: &CrewConfiguration) -> bool {
    crew.seats.iter().any(|s| {
        matches!(s.seat, CrewSeat::Captain | CrewSeat::Bridge)
            && s.officer_id.as_deref() == Some(TAL_OFFICER_LCARS_ID)
    })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrewConfiguration {
    pub seats: Vec<CrewSeatContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAbilityEffect {
    pub ability_name: String,
    /// Canonical officer id when this row comes from an officer seat (for trace attribution).
    pub officer_id: Option<String>,
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
            officer_id: seat_context.officer_id.clone(),
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
        .filter(|e| e.condition.as_ref().is_none_or(|c| c.evaluate(ctx)))
        .cloned()
        .collect()
}

/// Sum [`AbilityEffect::MitigationAdditive`] from combat-begin (or similar) filtered rows.
/// Applied on hostile return fire so gated “defense vs survey / …” armor rows affect damage taken.
pub fn sum_mitigation_additive(effects: &[ActiveAbilityEffect]) -> f64 {
    effects
        .iter()
        .filter_map(|e| match e.effect {
            AbilityEffect::MitigationAdditive(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// Hostile crit damage reduction from ship hull abilities (e.g. U.S.S. Crozier) and gated forbidden-tech
/// seats (e.g. Borg Operating Table vs Conqueror Borg). When multiple seats match **for the same
/// [`CombatContext`]**, uses the maximum `reduction` and maximum `duration_rounds`.
pub fn hostile_crit_damage_reduction_from_crew(
    crew: &CrewConfiguration,
    ctx: &CombatContext,
) -> (f64, u32) {
    let mut reduction = 0.0_f64;
    let mut rounds = 0_u32;
    for s in &crew.seats {
        if let AbilityEffect::HostileCritDamageReduction {
            reduction: r,
            duration_rounds: d,
        } = s.ability.effect
        {
            if s.ability
                .condition
                .as_ref()
                .is_some_and(|c| !c.evaluate(ctx))
            {
                continue;
            }
            reduction = reduction.max(r);
            rounds = rounds.max(d);
        }
    }
    (reduction.clamp(0.0, 0.95), rounds)
}
