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

/// Max `(state_id, weight)` pairs on [`AbilityEffect::RandomDefenderState`] (Hierarch uses 3).
pub const RANDOM_DEFENDER_STATE_OUTCOMES_CAP: usize = 8;

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
    /// Shield HP restored as `fraction × max shield HP` at the same timings as [`AbilityEffect::ShieldRegen`].
    ShieldRegenMaxFraction(f64),
    /// Hull HP restored as a **flat** value (reduces cumulative hull damage taken on that ship).
    /// Same timing split as [`AbilityEffect::ShieldRegen`].
    HullRegen(f64),
    /// Hull HP restored as `fraction × max hull HP` at the same timings as [`AbilityEffect::HullRegen`].
    HullRegenMaxFraction(f64),
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
    ///
    /// In the current engine this accumulator is consumed on the **outbound** damage path
    /// (added to the *defender's* effective `shield_mitigation`). Effects whose canonical
    /// `target` is `EnemyShip` belong here (debuff/buff the opponent — the sign of the value
    /// encodes the direction). For attacker-self semantics (officer buffs their **own**
    /// mitigation on counter-fire) see [`AbilityEffect::AttackerShieldMitigationBonus`].
    ShieldMitigationBonus(f64),
    /// **Multiplicative** shield-mitigation bypass on the defender. Engine applies as
    /// `defender_mitigation × (1 - bypass)` (e.g. Harrison "Sabotage" at canonical
    /// `op: MultiplySub` with value 0.7 → defender mitigates 30% of normal). Multiple sources
    /// stack additively; the total is clamped to `[0, 1]` so bypass cannot exceed 100%.
    ShieldMitigationBypassFraction(f64),
    /// Officer-granted shield mitigation that buffs the **attacker's own** mitigation on
    /// counter-fire / inbound damage (canonical `target: SelfShip`).
    ///
    /// The engine adds the composed value to `attacker.shield_mitigation` in
    /// [`crate::combat::engine`]'s `effective_incoming_shield_mitigation` helper. Multiple
    /// sources sum additively; the result is clamped to `[0, 1]` at the apply site.
    AttackerShieldMitigationBonus(f64),
    /// Officer-granted accuracy bonus; additive fraction (e.g. 0.05 = +5% accuracy). Applied to attacker accuracy for mitigation calculations.
    AccuracyBonus(f64),
    /// Additive fraction merged into the **player** ship’s mitigation when the hostile returns fire
    /// (counter-attack path). Used as an LCARS proxy for `armor` / “all defenses” rows that are not
    /// folded into [`Combatant::mitigation`] at scenario build. Values are **mitigation fractions**
    /// in `0..1`; the resolver normalizes sheet-style magnitudes (`> 1`) as percent points (`÷ 100`).
    MitigationAdditive(f64),
    /// Officer-granted dodge bonus; additive fraction (e.g. 0.10 = +10% dodge).
    /// Dodge is ship-type-weighted in the mitigation formula: interceptors benefit proportionally
    /// more from dodge than battleships. Applied to the player's effective mitigation on counter-fire.
    DodgeBonus(f64),
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
    /// On shield break (attacker proc): chance to skip all defender counter-attacks for `delay_rounds`
    /// combat rounds starting the round after the break (Uhura-style reload delay).
    DefenderFireDelay {
        chance: f64,
        delay_rounds: u32,
    },
    /// On round start: chance to apply one weighted random state to the defender.
    /// `state_outcomes`: `(STFC state id, relative weight)` e.g. `8→Morale`, `4→HullBreach`, `2→Burning`.
    RandomDefenderState {
        chance: f64,
        duration_rounds: u32,
        state_outcome_count: u8,
        state_outcomes: [(u32, u32); RANDOM_DEFENDER_STATE_OUTCOMES_CAP],
    },
    /// Multiplier on opponent captain-maneuver seat effects (1.0 = no change; 0.8 = 20% reduction).
    OpponentCaptainManeuverMultiplier(f64),
    /// Captain-only meta effect (Pike / McCoy / Picard `OffAbilityEffect`). Consumed at combat
    /// setup via [`sum_bridge_ability_effectiveness_add`] + [`scale_crew_bridge_ability_effects`];
    /// not applied per round. Value is the additive bonus (e.g. `0.4` for +40%).
    BridgeAbilityEffectivenessBonus(f64),
}

/// Active prefix of a packed [`AbilityEffect::RandomDefenderState`] outcome table.
pub fn random_defender_state_outcomes<'a>(
    count: u8,
    outcomes: &'a [(u32, u32); RANDOM_DEFENDER_STATE_OUTCOMES_CAP],
) -> &'a [(u32, u32)] {
    let n = (count as usize).min(RANDOM_DEFENDER_STATE_OUTCOMES_CAP);
    &outcomes[..n]
}

/// Pack compiled `(id, weight)` pairs into a fixed `Copy`-friendly table.
pub fn pack_random_defender_state_outcomes(
    pairs: &[(u32, u32)],
) -> (u8, [(u32, u32); RANDOM_DEFENDER_STATE_OUTCOMES_CAP]) {
    let mut outcomes = [(0_u32, 0_u32); RANDOM_DEFENDER_STATE_OUTCOMES_CAP];
    let n = pairs.len().min(RANDOM_DEFENDER_STATE_OUTCOMES_CAP);
    for (i, p) in pairs.iter().take(n).enumerate() {
        outcomes[i] = *p;
    }
    (n as u8, outcomes)
}

/// Weighted pick from `(state_id, weight)` pairs; `draw` should be in `0..total_weight`.
pub fn pick_weighted_state_id(state_weights: &[(u32, u32)], draw: u64) -> u32 {
    let total: u64 = state_weights.iter().map(|(_, w)| *w as u64).sum();
    if total == 0 {
        return state_weights.first().map(|(id, _)| *id).unwrap_or(8);
    }
    let mut pick = draw % total;
    for (id, w) in state_weights {
        if pick < *w as u64 {
            return *id;
        }
        pick -= *w as u64;
    }
    state_weights.last().map(|(id, _)| *id).unwrap_or(8)
}

/// Apply one STFC random-state id to defender timers; returns trace label.
pub fn apply_defender_random_state_id(
    state_id: u32,
    duration_rounds: u32,
    defender_burning_rounds: &mut u32,
    defender_hull_breach_rounds: &mut u32,
    defender_assimilated_rounds_remaining: &mut u32,
    defender_morale_rounds_remaining: &mut u32,
) -> &'static str {
    let dur = duration_rounds.max(1);
    match state_id {
        8 => {
            *defender_morale_rounds_remaining =
                (*defender_morale_rounds_remaining).max(dur);
            "morale"
        }
        4 => {
            *defender_hull_breach_rounds = (*defender_hull_breach_rounds).max(dur);
            "hull_breach"
        }
        2 => {
            *defender_burning_rounds = (*defender_burning_rounds).max(dur);
            "burning"
        }
        64 => {
            *defender_assimilated_rounds_remaining =
                (*defender_assimilated_rounds_remaining).max(dur);
            "assimilated"
        }
        _ => "unknown",
    }
}

/// Counter-fire pierce with defender Morale (primary pierce channel by hull class).
pub fn defender_morale_adjusted_pierce(
    base_pierce: f64,
    ship_type: crate::combat::ShipType,
    morale_active: bool,
) -> f64 {
    if !morale_active {
        return base_pierce;
    }
    use crate::combat::types::MORALE_PRIMARY_PIERCING_BONUS;
    match ship_type {
        crate::combat::ShipType::Battleship | crate::combat::ShipType::Interceptor => {
            base_pierce * (1.0 + MORALE_PRIMARY_PIERCING_BONUS)
        }
        // Explorer morale is accuracy in-game; aggregate pierce unchanged.
        _ => base_pierce,
    }
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
    /// True when the defender has an active Morale duration (e.g. random state id 8 from AddRandomState).
    pub defender_morale_active: bool,
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
    /// Owner faction of the attacking player hull (`ShipRecord::faction` → [`OpponentFactionTag`]).
    pub attacker_owner_faction: OpponentFactionTag,
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
    /// True when the attacking player hull’s owner faction matches (`ShipRecord::faction`, research `attacker_faction`).
    AttackerOwnerFactionIs(OpponentFactionTag),
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

/// Sum [`AbilityEffect::AccuracyBonus`] from combat-begin (or similar) filtered rows.
/// Added to the attacker's accuracy stat before computing hostile mitigation,
/// which reduces the defender's dodge contribution (higher accuracy → lower mitigation).
pub fn sum_accuracy_bonus(effects: &[ActiveAbilityEffect]) -> f64 {
    effects
        .iter()
        .filter_map(|e| match e.effect {
            AbilityEffect::AccuracyBonus(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// Sum [`AbilityEffect::DodgeBonus`] from combat-begin (or similar) filtered rows.
/// Ship-type-weighted and added to player mitigation on hostile counter-fire.
pub fn sum_dodge_bonus(effects: &[ActiveAbilityEffect]) -> f64 {
    effects
        .iter()
        .filter_map(|e| match e.effect {
            AbilityEffect::DodgeBonus(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// Product of [`AbilityEffect::OpponentCaptainManeuverMultiplier`] from combat-begin rows (default 1.0).
pub fn opponent_captain_maneuver_multiplier_from_effects(effects: &[ActiveAbilityEffect]) -> f64 {
    let mut mult = 1.0_f64;
    for e in effects {
        if let AbilityEffect::OpponentCaptainManeuverMultiplier(m) = e.effect {
            mult *= m.clamp(0.0, 1.0);
        }
    }
    mult
}

/// Sum additive bridge-ability effectiveness bonuses from combat-begin rows (Pike captain, etc.).
pub fn sum_bridge_ability_effectiveness_add(effects: &[ActiveAbilityEffect]) -> f64 {
    effects
        .iter()
        .filter_map(|e| match e.effect {
            AbilityEffect::BridgeAbilityEffectivenessBonus(v) if v.is_finite() && v > 0.0 => {
                Some(v)
            }
            _ => None,
        })
        .sum()
}

/// Scale bridge-officer ability magnitudes: `effective = base × (1 + bonus_add)`, capped at `1.0`
/// for probability-style stats (shield bypass, proc chances, etc.).
pub fn scale_crew_bridge_ability_effects(crew: &mut CrewConfiguration, bonus_add: f64) {
    if !bonus_add.is_finite() || bonus_add <= 0.0 {
        return;
    }
    for seat in &mut crew.seats {
        if seat.seat != CrewSeat::Bridge || seat.ability.class != AbilityClass::BridgeAbility {
            continue;
        }
        scale_bridge_officer_ability_effect(&mut seat.ability.effect, bonus_add);
    }
}

/// Apply Pike-style officer-ability effectiveness to one compiled bridge effect.
pub fn scale_bridge_officer_ability_effect(effect: &mut AbilityEffect, bonus_add: f64) {
    if !bonus_add.is_finite() || bonus_add <= 0.0 {
        return;
    }
    let factor = 1.0 + bonus_add;
    let cap_one = |v: f64| (v * factor).clamp(0.0, 1.0);
    let scale = |v: f64| v * factor;
    match effect {
        AbilityEffect::ShieldMitigationBypassFraction(v) => *v = cap_one(*v),
        AbilityEffect::Morale(chance) => *chance = cap_one(*chance),
        AbilityEffect::Assimilated {
            chance,
            duration_rounds: _,
        } => *chance = cap_one(*chance),
        AbilityEffect::HullBreach {
            chance,
            duration_rounds: _,
            requires_critical: _,
        } => *chance = cap_one(*chance),
        AbilityEffect::Burning {
            chance,
            duration_rounds: _,
        } => *chance = cap_one(*chance),
        AbilityEffect::ProcAttackMultiplier { chance, multiplier } => {
            *chance = cap_one(*chance);
            *multiplier = scale(*multiplier);
        }
        AbilityEffect::ProcPierceBonus { chance, bonus } => {
            *chance = cap_one(*chance);
            *bonus = scale(*bonus);
        }
        AbilityEffect::CritChanceBonus(c) => *c = cap_one(*c),
        AbilityEffect::AttackMultiplier(m) => *m = scale(*m),
        AbilityEffect::PierceBonus(p) => *p = scale(*p),
        AbilityEffect::CritDamageMultiplier(m) => *m = scale(*m),
        AbilityEffect::ShieldMitigationBonus(v) => *v = scale(*v),
        AbilityEffect::AttackerShieldMitigationBonus(v) => *v = scale(*v),
        AbilityEffect::MitigationAdditive(v) => *v = scale(*v),
        AbilityEffect::DodgeBonus(v) => *v = cap_one(*v),
        AbilityEffect::IsolyticDamageBonus(v) => *v = scale(*v),
        AbilityEffect::IsolyticDefenseBonus(v) => *v = scale(*v),
        AbilityEffect::IsolyticCascadeDamageBonus(v) => *v = scale(*v),
        AbilityEffect::ApexShredBonus(v) => *v = scale(*v),
        AbilityEffect::ApexBarrierBonus(v) => *v = scale(*v),
        AbilityEffect::ShieldRegen(v) => *v = scale(*v),
        AbilityEffect::ShieldRegenMaxFraction(f) => *f = cap_one(*f),
        AbilityEffect::HullRegen(v) => *v = scale(*v),
        AbilityEffect::HullRegenMaxFraction(f) => *f = cap_one(*f),
        AbilityEffect::HullRegenPrevRoundFraction(f) => *f = cap_one(*f),
        AbilityEffect::ShieldRegenPrevRoundFraction(f) => *f = cap_one(*f),
        AbilityEffect::DecayingAttackMultiplier {
            initial,
            decay_per_round,
            floor,
        } => {
            *initial = scale(*initial);
            *decay_per_round = scale(*decay_per_round);
            *floor = scale(*floor);
        }
        AbilityEffect::AccumulatingAttackMultiplier {
            initial,
            growth_per_round,
            ceiling,
        } => {
            *initial = scale(*initial);
            *growth_per_round = scale(*growth_per_round);
            *ceiling = scale(*ceiling);
        }
        AbilityEffect::OnKillHullRegen(v) => *v = scale(*v),
        AbilityEffect::AccuracyBonus(v) => *v = scale(*v),
        AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
            growth_per_round,
            ceiling,
        } => {
            *growth_per_round = scale(*growth_per_round);
            *ceiling = scale(*ceiling);
        }
        AbilityEffect::ShotsBonus {
            chance,
            bonus_pct,
            duration_rounds: _,
        } => {
            *chance = cap_one(*chance);
            *bonus_pct = scale(*bonus_pct);
        }
        AbilityEffect::HostileCritDamageReduction {
            reduction,
            duration_rounds: _,
        } => *reduction = cap_one(*reduction),
        AbilityEffect::CumulativeOpponentShieldMitigationDebuff { per_round, cap } => {
            *per_round = scale(*per_round);
            *cap = scale(*cap);
        }
        AbilityEffect::BridgeAbilityEffectivenessBonus(_)
        | AbilityEffect::OpponentCaptainManeuverMultiplier(_)
        | AbilityEffect::DefenderFireDelay { .. }
        | AbilityEffect::RandomDefenderState { .. }
        | AbilityEffect::ConquerorBorgBeamSuppression => {}
    }
}

/// Scale captain-maneuver seat effects on `crew` (PvP defender debuff from attacker LCARS).
pub fn scale_crew_captain_maneuver_effects(crew: &mut CrewConfiguration, multiplier: f64) {
    if multiplier >= 1.0 - 1e-12 {
        return;
    }
    for seat in &mut crew.seats {
        if seat.ability.class != AbilityClass::CaptainManeuver {
            continue;
        }
        match &mut seat.ability.effect {
            AbilityEffect::AttackMultiplier(m) => *m *= multiplier,
            AbilityEffect::PierceBonus(p) => *p *= multiplier,
            AbilityEffect::CritChanceBonus(c) => *c *= multiplier,
            AbilityEffect::CritDamageMultiplier(c) => *c *= multiplier,
            _ => {}
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──

    fn ctx_default() -> CombatContext {
        CombatContext {
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
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Battleship,
            attacker_ship_id: String::new(),
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            attacker_tal_assigned_captain_or_bridge: false,
            defender_hostile_tag_mask: 0,
            engagement_enemy_types: EnemyTypes::default(),
            combat_battle_type_id: None,
            defender_level: None,
        }
    }

    fn make_ability(
        name: &str,
        class: AbilityClass,
        timing: TimingWindow,
        effect: AbilityEffect,
    ) -> Ability {
        Ability {
            name: name.to_string(),
            class,
            timing,
            boostable: true,
            effect,
            condition: None,
        }
    }

    fn make_seat(seat: CrewSeat, ability: Ability, officer_id: Option<&str>) -> CrewSeatContext {
        CrewSeatContext {
            seat,
            ability,
            boosted: false,
            officer_id: officer_id.map(|s| s.to_string()),
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }
    }

    // ── can_activate_in_seat ──

    #[test]
    fn captain_activates_in_captain_seat() {
        let ab = make_ability(
            "test",
            AbilityClass::CaptainManeuver,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let seat = make_seat(CrewSeat::Captain, ab, None);
        assert!(can_activate_in_seat(&seat));
    }

    #[test]
    fn bridge_ability_activates_in_bridge_seat() {
        let ab = make_ability(
            "test",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let seat = make_seat(CrewSeat::Bridge, ab, None);
        assert!(can_activate_in_seat(&seat));
    }

    #[test]
    fn below_deck_activates_in_below_deck_seat() {
        let ab = make_ability(
            "test",
            AbilityClass::BelowDeck,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let seat = make_seat(CrewSeat::BelowDeck, ab, None);
        assert!(can_activate_in_seat(&seat));
    }

    #[test]
    fn captain_does_not_activate_in_bridge_seat() {
        let ab = make_ability(
            "test",
            AbilityClass::CaptainManeuver,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let seat = make_seat(CrewSeat::Bridge, ab, None);
        assert!(!can_activate_in_seat(&seat));
    }

    #[test]
    fn boosted_non_boostable_is_filtered_out() {
        let ab = Ability {
            boostable: false,
            ..make_ability(
                "test",
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::AttackMultiplier(0.1),
            )
        };
        let mut seat = make_seat(CrewSeat::Bridge, ab, None);
        seat.boosted = true;
        assert!(!can_activate_in_seat(&seat));
    }

    #[test]
    fn boosted_boostable_is_allowed() {
        let ab = make_ability(
            "test",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let mut seat = make_seat(CrewSeat::Bridge, ab, None);
        seat.boosted = true;
        assert!(can_activate_in_seat(&seat));
    }

    #[test]
    fn ship_ability_activates_in_ship_seat() {
        let ab = make_ability(
            "test",
            AbilityClass::ShipAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let seat = make_seat(CrewSeat::Ship, ab, None);
        assert!(can_activate_in_seat(&seat));
    }

    // ── active_effects_for_timing ──

    #[test]
    fn active_effects_filters_by_timing() {
        let ab1 = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let ab2 = make_ability(
            "b",
            AbilityClass::BridgeAbility,
            TimingWindow::RoundStart,
            AbilityEffect::PierceBonus(0.05),
        );
        let crew = CrewConfiguration {
            seats: vec![
                make_seat(CrewSeat::Bridge, ab1, None),
                make_seat(CrewSeat::Bridge, ab2, None),
            ],
        };
        let effects = active_effects_for_timing(&crew, TimingWindow::CombatBegin);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].ability_name, "a");
    }

    #[test]
    fn active_effects_returns_empty_when_no_match() {
        let ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
        };
        let effects = active_effects_for_timing(&crew, TimingWindow::RoundEnd);
        assert!(effects.is_empty());
    }

    #[test]
    fn active_effects_respects_seat_activation_rules() {
        let ab_captain = make_ability(
            "cap",
            AbilityClass::CaptainManeuver,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![make_seat(CrewSeat::Bridge, ab_captain, None)], // wrong seat
        };
        let effects = active_effects_for_timing(&crew, TimingWindow::CombatBegin);
        assert!(effects.is_empty());
    }

    // ── filter_effects_by_condition ──

    #[test]
    fn filter_no_condition_always_passes() {
        let ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let filtered = filter_effects_by_condition(&effects, &ctx_default());
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_literal_true_passes() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::LiteralBool(true));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let filtered = filter_effects_by_condition(&effects, &ctx_default());
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_literal_false_filters_out() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::LiteralBool(false));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let filtered = filter_effects_by_condition(&effects, &ctx_default());
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_morale_active_gates_when_morale_off() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::RoundStart,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::MoraleActive);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::RoundStart,
        );
        let ctx = ctx_default(); // morale_active = false
        let filtered = filter_effects_by_condition(&effects, &ctx);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_morale_active_passes_when_morale_on() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::RoundStart,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::MoraleActive);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::RoundStart,
        );
        let mut ctx = ctx_default();
        ctx.attacker_morale_active = true;
        let filtered = filter_effects_by_condition(&effects, &ctx);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_defender_burning_gates_correctly() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderBurning);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        // off
        assert!(filter_effects_by_condition(&effects, &ctx_default()).is_empty());
        // on
        let mut ctx = ctx_default();
        ctx.defender_burning_active = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_defender_hull_breach_gates_correctly() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderHullBreach);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_hull_breach_active = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_defender_assimilated_gates_correctly() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderAssimilated);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_assimilated_active = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_attacker_burning_gates_correctly() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::AttackerBurning);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.attacker_burning_active = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_attacker_hull_breach_gates_correctly() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::AttackerHullBreach);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.attacker_hull_breach_active = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_round_range_inside_passes() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::RoundRange { min: 1, max: 5 });
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.round_index = 3;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_round_range_outside_filters_out() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::RoundRange { min: 1, max: 5 });
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.round_index = 6;
        assert!(filter_effects_by_condition(&effects, &ctx).is_empty());
    }

    #[test]
    fn filter_defender_faction_is_matches() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderFactionIs(
            OpponentFactionTag::Klingon,
        ));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_faction = OpponentFactionTag::Klingon;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_defender_faction_is_mismatch_filters_out() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderFactionIs(
            OpponentFactionTag::Klingon,
        ));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert!(filter_effects_by_condition(&effects, &ctx_default()).is_empty());
    }

    #[test]
    fn filter_defender_ship_type_is_matches() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderShipTypeIs(ShipType::Explorer));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_ship_type = ShipType::Explorer;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_and_combines_conditions() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::And(vec![
            AbilityCondition::LiteralBool(true),
            AbilityCondition::LiteralBool(true),
        ]));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        );
    }

    #[test]
    fn filter_and_fails_when_one_is_false() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::And(vec![
            AbilityCondition::LiteralBool(true),
            AbilityCondition::LiteralBool(false),
        ]));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert!(filter_effects_by_condition(&effects, &ctx_default()).is_empty());
    }

    #[test]
    fn filter_or_passes_when_one_is_true() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::Or(vec![
            AbilityCondition::LiteralBool(false),
            AbilityCondition::LiteralBool(true),
        ]));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        );
    }

    #[test]
    fn filter_not_inverts_condition() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::Not(Box::new(
            AbilityCondition::LiteralBool(false),
        )));
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        );
    }

    #[test]
    fn filter_defender_is_npc_hostile_gates() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderIsNpcHostile);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        // default ctx has defender_is_npc_hostile = true
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        );
        let mut ctx = ctx_default();
        ctx.defender_is_npc_hostile = false;
        assert!(filter_effects_by_condition(&effects, &ctx).is_empty());
    }

    #[test]
    fn filter_defender_is_player_ship_gates() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::DefenderIsPlayerShip);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_is_player_ship = true;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_attacker_officer_tal_not_on_bridge_gates() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::AttackerOfficerTalNotOnBridge);
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        // default: tal not on bridge → true
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        );
        let mut ctx = ctx_default();
        ctx.attacker_tal_assigned_captain_or_bridge = true;
        assert!(filter_effects_by_condition(&effects, &ctx).is_empty());
    }

    #[test]
    fn filter_stat_below_hull_pct_passes_when_below() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::StatBelow {
            stat: "hull_hp".to_string(),
            threshold_pct: 0.5,
        });
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        let mut ctx = ctx_default();
        ctx.defender_hull_pct = 0.3;
        assert_eq!(filter_effects_by_condition(&effects, &ctx).len(), 1);
    }

    #[test]
    fn filter_stat_below_hull_pct_fails_when_above() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::StatBelow {
            stat: "hull_hp".to_string(),
            threshold_pct: 0.5,
        });
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert!(filter_effects_by_condition(&effects, &ctx_default()).is_empty());
        // 1.0 > 0.5
    }

    #[test]
    fn filter_stat_above_shield_pct_passes_when_above() {
        let mut ab = make_ability(
            "a",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        ab.condition = Some(AbilityCondition::StatAbove {
            stat: "shield_hp".to_string(),
            threshold_pct: 0.7,
        });
        let effects = active_effects_for_timing(
            &CrewConfiguration {
                seats: vec![make_seat(CrewSeat::Bridge, ab, None)],
            },
            TimingWindow::CombatBegin,
        );
        assert_eq!(
            filter_effects_by_condition(&effects, &ctx_default()).len(),
            1
        ); // 1.0 > 0.7
    }

    // ── apply_duplicate_officer_policy ──

    #[test]
    fn duplicate_policy_empty_crew_returns_empty() {
        let crew = CrewConfiguration { seats: vec![] };
        let result = apply_duplicate_officer_policy(&crew);
        assert!(result.seats.is_empty());
    }

    #[test]
    fn duplicate_policy_no_duplicates_preserves_all() {
        let ab = make_ability(
            "test",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![
                make_seat(CrewSeat::Bridge, ab.clone(), Some("officer_a")),
                make_seat(CrewSeat::Bridge, ab.clone(), Some("officer_b")),
            ],
        };
        let result = apply_duplicate_officer_policy(&crew);
        assert_eq!(result.seats.len(), 2);
    }

    #[test]
    fn duplicate_policy_removes_later_group_with_same_officer() {
        let ab = make_ability(
            "test",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![
                // First officer "dup" (group 1)
                make_seat(CrewSeat::Bridge, ab.clone(), Some("dup")),
                // Different officer (group 2)
                make_seat(CrewSeat::Bridge, ab.clone(), Some("other")),
                // Same "dup" officer again (group 3) — should be dropped
                make_seat(CrewSeat::Bridge, ab.clone(), Some("dup")),
            ],
        };
        let result = apply_duplicate_officer_policy(&crew);
        assert_eq!(
            result.seats.len(),
            2,
            "third seat with duplicate 'dup' should be dropped"
        );
        assert_eq!(result.seats[0].officer_id.as_deref(), Some("dup"));
        assert_eq!(result.seats[1].officer_id.as_deref(), Some("other"));
    }

    #[test]
    fn duplicate_policy_none_officer_id_always_included() {
        let ab = make_ability(
            "test",
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![
                make_seat(CrewSeat::Bridge, ab.clone(), None),
                make_seat(CrewSeat::Bridge, ab.clone(), None),
            ],
        };
        let result = apply_duplicate_officer_policy(&crew);
        assert_eq!(result.seats.len(), 2);
    }

    // ── attacker_crew_tal_assigned_captain_or_bridge ──

    #[test]
    fn tal_on_captain_is_detected() {
        let ab = make_ability(
            "tal_ability",
            AbilityClass::CaptainManeuver,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![make_seat(CrewSeat::Captain, ab, Some(TAL_OFFICER_LCARS_ID))],
        };
        assert!(attacker_crew_tal_assigned_captain_or_bridge(&crew));
    }

    #[test]
    fn tal_on_below_deck_is_not_detected() {
        let ab = make_ability(
            "tal_ability",
            AbilityClass::BelowDeck,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![make_seat(
                CrewSeat::BelowDeck,
                ab,
                Some(TAL_OFFICER_LCARS_ID),
            )],
        };
        assert!(!attacker_crew_tal_assigned_captain_or_bridge(&crew));
    }

    #[test]
    fn no_tal_returns_false() {
        let ab = make_ability(
            "test",
            AbilityClass::CaptainManeuver,
            TimingWindow::CombatBegin,
            AbilityEffect::AttackMultiplier(0.1),
        );
        let crew = CrewConfiguration {
            seats: vec![make_seat(CrewSeat::Captain, ab, Some("other_officer"))],
        };
        assert!(!attacker_crew_tal_assigned_captain_or_bridge(&crew));
    }

    // ── sum_mitigation_additive ──

    #[test]
    fn sum_mitigation_additive_sums_values() {
        let effects = vec![
            ActiveAbilityEffect {
                ability_name: "a".into(),
                officer_id: None,
                effect: AbilityEffect::MitigationAdditive(0.1),
                boosted: false,
                condition: None,
            },
            ActiveAbilityEffect {
                ability_name: "b".into(),
                officer_id: None,
                effect: AbilityEffect::MitigationAdditive(0.05),
                boosted: false,
                condition: None,
            },
        ];
        assert!((sum_mitigation_additive(&effects) - 0.15).abs() < 1e-12);
    }

    #[test]
    fn sum_mitigation_additive_ignores_non_mitigation_effects() {
        let effects = vec![
            ActiveAbilityEffect {
                ability_name: "a".into(),
                officer_id: None,
                effect: AbilityEffect::AttackMultiplier(0.1),
                boosted: false,
                condition: None,
            },
            ActiveAbilityEffect {
                ability_name: "b".into(),
                officer_id: None,
                effect: AbilityEffect::MitigationAdditive(0.05),
                boosted: false,
                condition: None,
            },
        ];
        assert!((sum_mitigation_additive(&effects) - 0.05).abs() < 1e-12);
    }

    // ── hostile_crit_damage_reduction_from_crew ──

    #[test]
    fn hostile_crit_reduction_returns_max_reduction_and_duration() {
        let ab1 = {
            let mut ab = make_ability(
                "a",
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.05,
                    duration_rounds: 3,
                },
            );
            ab.condition = None;
            ab
        };
        let ab2 = {
            let mut ab = make_ability(
                "b",
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.08,
                    duration_rounds: 5,
                },
            );
            ab.condition = None;
            ab
        };
        let crew = CrewConfiguration {
            seats: vec![
                make_seat(CrewSeat::Ship, ab1, None),
                make_seat(CrewSeat::Ship, ab2, None),
            ],
        };
        let (reduction, rounds) = hostile_crit_damage_reduction_from_crew(&crew, &ctx_default());
        assert!((reduction - 0.08).abs() < 1e-12);
        assert_eq!(rounds, 5);
    }

    #[test]
    fn hostile_crit_reduction_respects_condition_gating() {
        let mut ab = make_ability(
            "a",
            AbilityClass::ShipAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::HostileCritDamageReduction {
                reduction: 0.05,
                duration_rounds: 3,
            },
        );
        ab.condition = Some(AbilityCondition::LiteralBool(false));
        let crew = CrewConfiguration {
            seats: vec![make_seat(CrewSeat::Ship, ab, None)],
        };
        let (reduction, rounds) = hostile_crit_damage_reduction_from_crew(&crew, &ctx_default());
        assert!((reduction - 0.0).abs() < 1e-12);
        assert_eq!(rounds, 0);
    }
}
