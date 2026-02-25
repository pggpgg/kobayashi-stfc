use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::combat::abilities::{
    active_effects_for_timing, AbilityEffect, ActiveAbilityEffect, CrewConfiguration, TimingWindow,
};
use crate::combat::rng::Rng;
use crate::combat::stacking::{StackContribution, StatStacking};

/// Combat mitigation parity implementation migrated from
/// `tools/combat_engine/mitigation.py`.
///
/// The formulas and clamps in this module intentionally mirror the Python
/// reference behavior exactly.
#[derive(Debug, Clone)]
pub struct FightResult {
    pub won: bool,
}

pub const EPSILON: f64 = 1e-9;
pub const MAX_COMBAT_ROUNDS: u32 = 100;
pub const MORALE_PRIMARY_PIERCING_BONUS: f64 = 0.10;
pub const HULL_BREACH_CRIT_BONUS: f64 = 1.5;
pub const BURNING_HULL_DAMAGE_PER_ROUND: f64 = 0.01;
pub const ASSIMILATED_EFFECTIVENESS_MULTIPLIER: f64 = 0.75;

pub const SURVEY_COEFFICIENTS: (f64, f64, f64) = (0.3, 0.3, 0.3);
pub const BATTLESHIP_COEFFICIENTS: (f64, f64, f64) = (0.55, 0.2, 0.2);
pub const EXPLORER_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.55, 0.2);
pub const INTERCEPTOR_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.2, 0.55);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipType {
    Survey,
    Armada,
    Battleship,
    Explorer,
    Interceptor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefenderStats {
    pub armor: f64,
    pub shield_deflection: f64,
    pub dodge: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackerStats {
    pub armor_piercing: f64,
    pub shield_piercing: f64,
    pub accuracy: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub officer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship_ability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostile_ability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_bonus_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatEvent {
    pub event_type: String,
    pub round_index: u32,
    pub phase: String,
    pub source: EventSource,
    #[serde(default)]
    pub values: Map<String, Value>,
    /// Sub-round (weapon) index when tracing multi-weapon resolution. Omitted when None.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub weapon_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceMode {
    Off,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub rounds: u32,
    pub seed: u64,
    pub trace_mode: TraceMode,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            rounds: 3,
            seed: 7,
            trace_mode: TraceMode::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub total_damage: f64,
    pub attacker_won: bool,
    pub winner_by_round_limit: bool,
    pub rounds_simulated: u32,
    pub attacker_hull_remaining: f64,
    pub defender_hull_remaining: f64,
    /// Defender shield HP remaining at end of combat (0 when shields were depleted).
    #[serde(default)]
    pub defender_shield_remaining: f64,
    pub events: Vec<CombatEvent>,
}

/// Per-weapon stats for sub-round resolution. Combatant-level pierce/crit/proc apply to all weapons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponStats {
    pub attack: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combatant {
    pub id: String,
    pub attack: f64,
    pub mitigation: f64,
    pub pierce: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub proc_chance: f64,
    pub proc_multiplier: f64,
    pub end_of_round_damage: f64,
    pub hull_health: f64,
    /// Maximum shield hit points. When defending, damage is split between shield and hull by shield_mitigation until shields are depleted.
    #[serde(default)]
    pub shield_health: f64,
    /// Fraction of incoming (unmitigated, post-apex) damage that goes to shield; rest goes to hull. Base 0.8 (80% shields, 20% hull). When shields are depleted, all damage goes to hull.
    #[serde(default = "default_shield_mitigation")]
    pub shield_mitigation: f64,
    /// Defender stat: reduces damage after other mitigation. Effective barrier is divided by (1 + attacker apex_shred).
    #[serde(default)]
    pub apex_barrier: f64,
    /// Attacker stat: reduces defender's effective apex_barrier. Stored as decimal (1.0 = 100%).
    #[serde(default)]
    pub apex_shred: f64,
    /// Attacker: isolytic damage bonus (decimal, e.g. 0.15 = 15% of regular damage as isolytic). Used in isolytic_damage().
    #[serde(default)]
    pub isolytic_damage: f64,
    /// Defender: flat reduction to isolytic damage taken (or mitigation-style; applied after isolytic_damage()).
    #[serde(default)]
    pub isolytic_defense: f64,
    /// Per-weapon attack values for sub-round resolution. If empty, one weapon with scalar `attack` is used (backward compat).
    #[serde(default)]
    pub weapons: Vec<WeaponStats>,
}

fn default_shield_mitigation() -> f64 {
    0.8
}

impl Combatant {
    /// Number of weapons (sub-rounds per round). Empty weapons list is treated as one weapon using scalar `attack`.
    pub fn weapon_count(&self) -> usize {
        self.weapons.len().max(1)
    }

    /// Attack value for weapon at index. Returns None if index >= weapon_count (caller should not fire).
    pub fn weapon_attack(&self, weapon_index: usize) -> Option<f64> {
        if self.weapons.is_empty() {
            if weapon_index == 0 {
                Some(self.attack)
            } else {
                None
            }
        } else {
            self.weapons.get(weapon_index).map(|w| w.attack)
        }
    }
}

#[derive(Debug, Default)]
pub struct TraceCollector {
    enabled: bool,
    events: Vec<CombatEvent>,
}

impl TraceCollector {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            events: Vec::new(),
        }
    }

    pub fn record(&mut self, event: CombatEvent) {
        if self.enabled {
            self.events.push(event);
        }
    }

    pub fn events(self) -> Vec<CombatEvent> {
        self.events
    }
}

impl ShipType {
    pub const fn coefficients(self) -> (f64, f64, f64) {
        match self {
            Self::Survey => SURVEY_COEFFICIENTS,
            Self::Armada => SURVEY_COEFFICIENTS,
            Self::Battleship => BATTLESHIP_COEFFICIENTS,
            Self::Explorer => EXPLORER_COEFFICIENTS,
            Self::Interceptor => INTERCEPTOR_COEFFICIENTS,
        }
    }
}

/// Compute component mitigation f(x) = 1 / (1 + 4^(1.1 - x)).
pub fn component_mitigation(defense: f64, piercing: f64) -> f64 {
    let safe_defense = defense.max(0.0);
    let safe_piercing = piercing.max(EPSILON);
    let x = safe_defense / safe_piercing;
    1.0 / (1.0 + 4.0_f64.powf(1.1 - x))
}

/// Maximum pierce damage-through bonus (additive to (1 - mitigation)).
pub const PIERCE_CAP: f64 = 0.25;

/// Pierce damage-through bonus derived from defender/attacker stats and ship type.
/// Uses the same defense/piercing ratios as mitigation (STFC Toolbox). Formula:
/// `pierce = 0.25 * (1 - mitigation(defender, attacker, ship_type))`, clamped to [0, PIERCE_CAP].
/// So when mitigation is low (high pierce vs defense), pierce bonus is high; when mitigation is high, pierce is low.
pub fn pierce_damage_through_bonus(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
) -> f64 {
    let mit = mitigation(defender, attacker, ship_type);
    (PIERCE_CAP * (1.0 - mit)).clamp(0.0, PIERCE_CAP)
}

/// Compute total mitigation using weighted multiplicative composition.
pub fn mitigation(defender: DefenderStats, attacker: AttackerStats, ship_type: ShipType) -> f64 {
    let (c_armor, c_shield, c_dodge) = ship_type.coefficients();

    let f_armor = component_mitigation(defender.armor, attacker.armor_piercing);
    let f_shield = component_mitigation(defender.shield_deflection, attacker.shield_piercing);
    let f_dodge = component_mitigation(defender.dodge, attacker.accuracy);

    let total =
        1.0 - (1.0 - c_armor * f_armor) * (1.0 - c_shield * f_shield) * (1.0 - c_dodge * f_dodge);
    total.clamp(0.0, 1.0)
}

pub fn mitigation_with_morale(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    morale_active: bool,
) -> f64 {
    let attacker = if morale_active {
        apply_morale_primary_piercing(attacker, ship_type)
    } else {
        attacker
    };
    mitigation(defender, attacker, ship_type)
}

/// Compute isolytic damage from already-resolved regular attack damage.
///
/// Formula:
/// `regular_attack_damage * (isolytic_damage_bonus + (1 + isolytic_damage_bonus) * isolytic_cascade_damage_bonus)`
///
/// This includes both base isolytic bonus and isolytic cascade bonus (for example,
/// officer effects like Enterprise-E Data that add isolytic cascade damage).
pub fn isolytic_damage(
    regular_attack_damage: f64,
    isolytic_damage_bonus: f64,
    isolytic_cascade_damage_bonus: f64,
) -> f64 {
    regular_attack_damage.max(0.0)
        * (isolytic_damage_bonus + (1.0 + isolytic_damage_bonus) * isolytic_cascade_damage_bonus)
}

pub fn apply_morale_primary_piercing(
    attacker: AttackerStats,
    ship_type: ShipType,
) -> AttackerStats {
    let mut adjusted = attacker;
    match ship_type {
        ShipType::Battleship => {
            adjusted.shield_piercing *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Interceptor => {
            adjusted.armor_piercing *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Explorer => {
            adjusted.accuracy *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
        }
        ShipType::Survey => {}
        ShipType::Armada => {}
    }

    adjusted
}

pub fn serialize_events_json(events: &[CombatEvent]) -> Result<String, serde_json::Error> {
    let payload: Vec<Value> = events
        .iter()
        .map(|event| {
            let mut object = Map::new();
            object.insert(
                "event_type".to_string(),
                Value::String(event.event_type.clone()),
            );
            object.insert("round_index".to_string(), Value::from(event.round_index));
            object.insert("phase".to_string(), Value::String(event.phase.clone()));
            object.insert("source".to_string(), serialize_source(&event.source));
            object.insert("values".to_string(), Value::Object(event.values.clone()));
            if let Some(wi) = event.weapon_index {
                object.insert("weapon_index".to_string(), Value::from(wi));
            }
            Value::Object(object)
        })
        .collect();

    to_canonical_json(&payload)
}

pub fn simulate_combat(
    attacker: &Combatant,
    defender: &Combatant,
    config: SimulationConfig,
    attacker_crew: &CrewConfiguration,
) -> SimulationResult {
    let mut rng = Rng::new(config.seed);
    let mut trace = TraceCollector::new(matches!(config.trace_mode, TraceMode::Events));
    let mut total_hull_damage = 0.0;
    let mut total_shield_damage = 0.0;
    let mut defender_shield_remaining = defender.shield_health.max(0.0);
    let mut attacker_shield_remaining = attacker.shield_health.max(0.0);
    let mut total_attacker_hull_damage = 0.0;
    let mut hull_breach_rounds_remaining = 0_u32;
    let mut burning_rounds_remaining = 0_u32;
    let mut assimilated_rounds_remaining = 0_u32;
    let combat_begin_effects = active_effects_for_timing(attacker_crew, TimingWindow::CombatBegin);

    let combat_begin_assimilated = assimilated_rounds_remaining > 0;
    record_ability_activations(
        &mut trace,
        0,
        "combat_begin",
        attacker,
        &combat_begin_effects,
        combat_begin_assimilated,
    );

    let rounds_to_simulate = config.rounds.min(MAX_COMBAT_ROUNDS);
    let mut rounds_completed = 0u32;

    for round_index in 1..=rounds_to_simulate {
        rounds_completed = round_index;
        let round_start_effects =
            active_effects_for_timing(attacker_crew, TimingWindow::RoundStart);
        let attack_phase_effects =
            active_effects_for_timing(attacker_crew, TimingWindow::AttackPhase);
        let defense_phase_effects =
            active_effects_for_timing(attacker_crew, TimingWindow::DefensePhase);
        let round_end_effects = active_effects_for_timing(attacker_crew, TimingWindow::RoundEnd);

        let mut phase_effects = EffectAccumulator::default();
        phase_effects.add_effects(
            TimingWindow::CombatBegin,
            &combat_begin_effects,
            attacker.attack,
            assimilated_rounds_remaining > 0,
        );

        trace.record(CombatEvent {
            event_type: "round_start".to_string(),
            round_index,
            phase: "round".to_string(),
            source: EventSource {
                ship_ability_id: Some("baseline_round".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("attacker".to_string(), Value::String(attacker.id.clone())),
                ("defender".to_string(), Value::String(defender.id.clone())),
                (
                    "active_round_start_effects".to_string(),
                    Value::from(round_start_effects.len() as u64),
                ),
            ]),
        });

        let round_start_assimilated = assimilated_rounds_remaining > 0;
        record_ability_activations(
            &mut trace,
            round_index,
            "round_start",
            attacker,
            &round_start_effects,
            round_start_assimilated,
        );
        phase_effects.add_effects(
            TimingWindow::RoundStart,
            &round_start_effects,
            attacker.attack,
            round_start_assimilated,
        );

        for effect in &round_start_effects {
            let effective_effect = scale_effect(effect.effect, round_start_assimilated);

            if let AbilityEffect::Assimilated {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let assimilated_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    assimilated_rounds_remaining =
                        assimilated_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "assimilated_trigger".to_string(),
                    round_index,
                    phase: "round_start".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(assimilated_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }

            if let AbilityEffect::HullBreach {
                chance,
                duration_rounds,
                requires_critical,
            } = effective_effect
            {
                if requires_critical {
                    continue;
                }

                let hull_breach_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = hull_breach_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    hull_breach_rounds_remaining =
                        hull_breach_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "hull_breach_trigger".to_string(),
                    round_index,
                    phase: "round_start".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(hull_breach_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }

            if let AbilityEffect::Burning {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let burning_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = burning_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    burning_rounds_remaining = burning_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "burning_trigger".to_string(),
                    round_index,
                    phase: "round_start".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(burning_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }
        }

        let round_end_assimilated_early = assimilated_rounds_remaining > 0;
        phase_effects.add_effects(
            TimingWindow::RoundEnd,
            &round_end_effects,
            attacker.attack,
            round_end_assimilated_early,
        );
        let phase_effects_round = phase_effects.clone();
        let num_sub_rounds = attacker.weapon_count().max(defender.weapon_count());

        let mut effective_pierce = attacker.pierce + phase_effects_round.pre_attack_pierce_bonus();
        let morale_source = round_start_effects.iter().find_map(|effect| {
            if let AbilityEffect::Morale(chance) =
                scale_effect(effect.effect, round_start_assimilated)
            {
                Some((effect.ability_name.clone(), chance.clamp(0.0, 1.0)))
            } else {
                None
            }
        });
        if let Some((morale_source, morale_chance)) = morale_source {
            let morale_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            let morale_triggered = morale_roll < morale_chance;
            if morale_triggered {
                effective_pierce *= 1.0 + MORALE_PRIMARY_PIERCING_BONUS;
            }
            trace.record(CombatEvent {
                event_type: "morale_activation".to_string(),
                round_index,
                phase: "round_start".to_string(),
                source: EventSource {
                    ship_ability_id: Some(morale_source),
                    ..EventSource::default()
                },
                weapon_index: None,
                values: Map::from_iter([
                    ("triggered".to_string(), Value::Bool(morale_triggered)),
                    ("roll".to_string(), Value::from(round_f64(morale_roll))),
                    ("chance".to_string(), Value::from(round_f64(morale_chance))),
                    (
                        "applied_to".to_string(),
                        Value::String("primary_piercing".to_string()),
                    ),
                    (
                        "multiplier".to_string(),
                        Value::from(1.0 + MORALE_PRIMARY_PIERCING_BONUS),
                    ),
                ]),
            });
        }

        let attack_phase_assimilated = assimilated_rounds_remaining > 0;
        record_ability_activations(
            &mut trace,
            round_index,
            "attack",
            attacker,
            &attack_phase_effects,
            attack_phase_assimilated,
        );
        let defense_phase_assimilated = assimilated_rounds_remaining > 0;
        record_ability_activations(
            &mut trace,
            round_index,
            "defense",
            attacker,
            &defense_phase_effects,
            defense_phase_assimilated,
        );

        for weapon_index in 0..num_sub_rounds {
            let mut phase_effects = phase_effects_round.clone();
            let weapon_base = attacker.weapon_attack(weapon_index).unwrap_or(attacker.attack);
            phase_effects.add_effects(
                TimingWindow::AttackPhase,
                &attack_phase_effects,
                weapon_base,
                attack_phase_assimilated,
            );
            phase_effects.add_effects(
                TimingWindow::DefensePhase,
                &defense_phase_effects,
                weapon_base,
                defense_phase_assimilated,
            );

            let effective_apex_shred = (attacker.apex_shred + phase_effects.composed_apex_shred_bonus())
            .max(0.0);
            let effective_apex_barrier = (defender.apex_barrier + phase_effects.composed_apex_barrier_bonus())
                .max(0.0);
            let effective_barrier = effective_apex_barrier
                / (1.0 + effective_apex_shred).max(EPSILON);
            let apex_damage_factor = 10000.0 / (10000.0 + effective_barrier);

            let weapon_index_u = weapon_index as u32;
            if let Some(attacker_weapon_attack) = attacker.weapon_attack(weapon_index) {
            let effective_attack = attacker_weapon_attack * phase_effects.pre_attack_multiplier();

            let roll = (rng.next_u64() as f64) / (u64::MAX as f64);
            trace.record(CombatEvent {
                event_type: "attack_roll".to_string(),
                round_index,
                phase: "attack".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ..EventSource::default()
                },
                weapon_index: Some(weapon_index_u),
                values: Map::from_iter([
                    ("roll".to_string(), Value::from(round_f64(roll))),
                    ("base_attack".to_string(), Value::from(attacker_weapon_attack)),
                    (
                        "effective_attack".to_string(),
                        Value::from(round_f64(effective_attack)),
                    ),
                ]),
            });

            let mitigation_multiplier = (1.0 - defender.mitigation).max(0.0);
            trace.record(CombatEvent {
                event_type: "mitigation_calc".to_string(),
                round_index,
                phase: "defense".to_string(),
                source: EventSource {
                    hostile_ability_id: Some(format!("{}_mitigation", defender.id)),
                    ..EventSource::default()
                },
                weapon_index: Some(weapon_index_u),
                values: Map::from_iter([
                ("mitigation".to_string(), Value::from(defender.mitigation)),
                (
                    "multiplier".to_string(),
                    Value::from(round_f64(mitigation_multiplier)),
                ),
            ]),
        });

        // Damage-through factor: fraction of attack that gets through (can exceed 1.0 with pierce).
        // Pierce is additive to (1 - mitigation); no cap for pierce-bypass behavior.
        let damage_through_factor =
            (mitigation_multiplier + effective_pierce + phase_effects.defense_mitigation_bonus())
                .max(0.0);
        trace.record(CombatEvent {
            event_type: "pierce_calc".to_string(),
            round_index,
            phase: "attack".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                player_bonus_source: Some("attack_pierce_bonus".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("pierce".to_string(), Value::from(effective_pierce)),
                (
                    "damage_through_factor".to_string(),
                    Value::from(round_f64(damage_through_factor)),
                ),
            ]),
        });

        let hull_breach_active = hull_breach_rounds_remaining > 0;
        let crit_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let is_crit = crit_roll < attacker.crit_chance;
        let crit_multiplier = if is_crit {
            let base_crit_multiplier = attacker.crit_multiplier;
            if hull_breach_active {
                base_crit_multiplier * (1.0 + HULL_BREACH_CRIT_BONUS)
            } else {
                base_crit_multiplier
            }
        } else {
            1.0
        };
        trace.record(CombatEvent {
            event_type: "crit_resolution".to_string(),
            round_index,
            phase: "attack".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                ship_ability_id: Some("crit_matrix".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("roll".to_string(), Value::from(round_f64(crit_roll))),
                ("is_crit".to_string(), Value::Bool(is_crit)),
                ("multiplier".to_string(), Value::from(crit_multiplier)),
                (
                    "hull_breach_active".to_string(),
                    Value::Bool(hull_breach_active),
                ),
            ]),
        });

        for effect in &attack_phase_effects {
            let effective_effect = scale_effect(effect.effect, attack_phase_assimilated);

            if let AbilityEffect::Assimilated {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let assimilated_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = assimilated_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    assimilated_rounds_remaining =
                        assimilated_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "assimilated_trigger".to_string(),
                    round_index,
                    phase: "attack".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(assimilated_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }

            if let AbilityEffect::HullBreach {
                chance,
                duration_rounds,
                requires_critical,
            } = effective_effect
            {
                if requires_critical && !is_crit {
                    continue;
                }

                let hull_breach_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = hull_breach_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    hull_breach_rounds_remaining =
                        hull_breach_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "hull_breach_trigger".to_string(),
                    round_index,
                    phase: "attack".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(hull_breach_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                        (
                            "requires_critical".to_string(),
                            Value::Bool(requires_critical),
                        ),
                    ]),
                });
            }

            if let AbilityEffect::Burning {
                chance,
                duration_rounds,
            } = effective_effect
            {
                let burning_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
                let triggered = burning_roll < chance.clamp(0.0, 1.0);
                if triggered {
                    burning_rounds_remaining = burning_rounds_remaining.max(duration_rounds.max(1));
                }
                trace.record(CombatEvent {
                    event_type: "burning_trigger".to_string(),
                    round_index,
                    phase: "attack".to_string(),
                    source: EventSource {
                        officer_id: Some(attacker.id.clone()),
                        ship_ability_id: Some(effect.ability_name.clone()),
                        ..EventSource::default()
                    },
                    weapon_index: None,
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(burning_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }
        }

        let proc_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let did_proc = proc_roll < attacker.proc_chance;
        let proc_multiplier = if did_proc {
            attacker.proc_multiplier
        } else {
            1.0
        };
        trace.record(CombatEvent {
            event_type: "proc_triggers".to_string(),
            round_index,
            phase: "proc".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                ship_ability_id: Some("officer_proc".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                ("roll".to_string(), Value::from(round_f64(proc_roll))),
                ("triggered".to_string(), Value::Bool(did_proc)),
                ("multiplier".to_string(), Value::from(proc_multiplier)),
            ]),
        });

        let pre_attack_damage =
            effective_attack * damage_through_factor * crit_multiplier * proc_multiplier;
        phase_effects.set_pre_attack_damage_base(pre_attack_damage);
        let pre_attack_damage = phase_effects.composed_pre_attack_damage();
        let damage = phase_effects.compose_attack_phase_damage(pre_attack_damage);
        let damage_after_apex = damage * apex_damage_factor;

        // Isolytic: extra damage from attacker isolytic_damage bonus, reduced by defender isolytic_defense.
        // Officer/ability effects add via EffectAccumulator (composed_isolytic_damage_bonus, composed_isolytic_defense_bonus).
        let effective_isolytic_damage = (attacker.isolytic_damage + phase_effects.composed_isolytic_damage_bonus()).max(0.0);
        let effective_isolytic_defense = (defender.isolytic_defense + phase_effects.composed_isolytic_defense_bonus()).max(0.0);
        let isolytic_component = isolytic_damage(
            damage_after_apex,
            effective_isolytic_damage,
            0.0,
        );
        let isolytic_net = (isolytic_component - effective_isolytic_defense).max(0.0);
        let damage_after_apex = damage_after_apex + isolytic_net;

        // Shield mitigation: S * damage to shield, (1-S) * damage to hull (STFC Toolbox game-mechanics).
        // https://stfc-toolbox.vercel.app/game-mechanics — "Shield mitigation": shp_damage_taken = S * total_unmitigated_damage, hhp_damage_taken = (1-S) * total_unmitigated_damage. Base S ≈ 0.8 (80% to shields). When shields are depleted, all damage goes to hull.
        // Officer/ability effects add via EffectAccumulator (composed_shield_mitigation_bonus).
        let effective_shield_mitigation = (defender.shield_mitigation + phase_effects.composed_shield_mitigation_bonus()).clamp(0.0, 1.0);
        let shield_mitigation = if defender_shield_remaining > 0.0 {
            effective_shield_mitigation
        } else {
            0.0
        };
        let shield_portion = damage_after_apex * shield_mitigation;
        let hull_portion = damage_after_apex * (1.0 - shield_mitigation);
        let actual_shield_damage = shield_portion.min(defender_shield_remaining);
        let shield_overflow = shield_portion - actual_shield_damage;
        let hull_damage_this_round = hull_portion + shield_overflow;

        defender_shield_remaining = (defender_shield_remaining - actual_shield_damage).max(0.0);
        total_hull_damage += hull_damage_this_round;
        total_shield_damage += actual_shield_damage;

        let shield_broke_this_round = actual_shield_damage > 0.0 && defender_shield_remaining <= 0.0;

        trace.record(CombatEvent {
            event_type: "damage_application".to_string(),
            round_index,
            phase: "damage".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                hostile_ability_id: Some(format!("{}_hull", defender.id)),
                ..EventSource::default()
            },
            weapon_index: Some(weapon_index_u),
            values: Map::from_iter([
                ("damage_after_apex".to_string(), Value::from(round_f64(damage_after_apex))),
                ("shield_mitigation".to_string(), Value::from(round_f64(shield_mitigation))),
                ("shield_damage".to_string(), Value::from(round_f64(actual_shield_damage))),
                ("hull_damage".to_string(), Value::from(round_f64(hull_damage_this_round))),
                (
                    "running_hull_damage".to_string(),
                    Value::from(round_f64(total_hull_damage)),
                ),
                (
                    "defender_shield_remaining".to_string(),
                    Value::from(round_f64(defender_shield_remaining)),
                ),
                (
                    "shield_broke".to_string(),
                    Value::Bool(shield_broke_this_round),
                ),
                (
                    "assimilated_active".to_string(),
                    Value::Bool(assimilated_rounds_remaining > 0),
                ),
            ]),
        });
            }

            if let Some(defender_weapon_attack) = defender.weapon_attack(weapon_index) {
        // Defender counter-attack: defender deals damage to attacker (ship runs out of HHP ends fight).
        let def_mitigation_mult = (1.0 - attacker.mitigation).max(0.0);
        let def_damage_through = (def_mitigation_mult + defender.pierce).max(0.0);
        let def_crit_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let def_crit_mult = if def_crit_roll < defender.crit_chance {
            defender.crit_multiplier
        } else {
            1.0
        };
        let def_proc_roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        let def_proc_mult = if def_proc_roll < defender.proc_chance {
            defender.proc_multiplier
        } else {
            1.0
        };
        let def_to_attacker_damage =
            defender_weapon_attack * def_damage_through * def_crit_mult * def_proc_mult;
        let att_shield_mitigation = if attacker_shield_remaining > 0.0 {
            attacker.shield_mitigation.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let att_shield_portion = def_to_attacker_damage * att_shield_mitigation;
        let att_hull_portion = def_to_attacker_damage * (1.0 - att_shield_mitigation);
        let att_actual_shield_damage = att_shield_portion.min(attacker_shield_remaining);
        let att_shield_overflow = att_shield_portion - att_actual_shield_damage;
        let att_hull_damage_this_round = att_hull_portion + att_shield_overflow;
        attacker_shield_remaining = (attacker_shield_remaining - att_actual_shield_damage).max(0.0);
        total_attacker_hull_damage += att_hull_damage_this_round;
            }
        }

        record_ability_activations(
            &mut trace,
            round_index,
            "round_end",
            attacker,
            &round_end_effects,
            round_end_assimilated_early,
        );

        let round_end_apex_shred = (attacker.apex_shred + phase_effects_round.composed_apex_shred_bonus()).max(0.0);
        let round_end_apex_barrier = (defender.apex_barrier + phase_effects_round.composed_apex_barrier_bonus()).max(0.0);
        let round_end_apex_factor = 10000.0 / (10000.0 + round_end_apex_barrier / (1.0 + round_end_apex_shred).max(EPSILON));
        let bonus_damage = phase_effects_round.compose_round_end_damage(attacker.end_of_round_damage);
        // Burning: 1% of total (max) hull per round, not remaining (per STFC).
        let burning_damage = if burning_rounds_remaining > 0 {
            defender.hull_health.max(0.0) * BURNING_HULL_DAMAGE_PER_ROUND
        } else {
            0.0
        };
        // Round-end and burning apply to hull only (shields do not absorb these).
        total_hull_damage += (bonus_damage + burning_damage) * round_end_apex_factor;
        total_attacker_hull_damage += defender.end_of_round_damage;

        // Regen: shield and hull restoration at round end (from officer/data regen effects).
        let shield_regen = phase_effects_round.composed_shield_regen();
        let hull_regen = phase_effects_round.composed_hull_regen();
        defender_shield_remaining = (defender_shield_remaining + shield_regen)
            .min(defender.shield_health.max(0.0));
        total_hull_damage = (total_hull_damage - hull_regen).max(0.0);

        if burning_rounds_remaining > 0 {
            burning_rounds_remaining -= 1;
        }
        if hull_breach_rounds_remaining > 0 {
            hull_breach_rounds_remaining -= 1;
        }
        if assimilated_rounds_remaining > 0 {
            assimilated_rounds_remaining -= 1;
        }

        trace.record(CombatEvent {
            event_type: "end_of_round_effects".to_string(),
            round_index,
            phase: "end".to_string(),
            source: EventSource {
                player_bonus_source: Some("round_end_bonus".to_string()),
                ..EventSource::default()
            },
            weapon_index: None,
            values: Map::from_iter([
                (
                    "bonus_damage".to_string(),
                    Value::from(round_f64(bonus_damage)),
                ),
                (
                    "burning_damage".to_string(),
                    Value::from(round_f64(burning_damage)),
                ),
                (
                    "running_hull_damage".to_string(),
                    Value::from(round_f64(total_hull_damage)),
                ),
            ]),
        });

        // Fight ends when defender or attacker runs out of hull (HHP).
        let defender_hull_now = (defender.hull_health - total_hull_damage).max(0.0);
        let attacker_hull_now = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
        if defender_hull_now <= 0.0 || attacker_hull_now <= 0.0 {
            break;
        }
    }

    let total_damage = total_hull_damage + total_shield_damage;
    let attacker_hull_remaining = (attacker.hull_health - total_attacker_hull_damage).max(0.0);
    let defender_hull_remaining = (defender.hull_health - total_hull_damage).max(0.0);
    let winner_by_round_limit = rounds_completed == MAX_COMBAT_ROUNDS
        && defender_hull_remaining > 0.0
        && attacker_hull_remaining > 0.0;
    let attacker_won = if attacker_hull_remaining <= 0.0 {
        false
    } else if defender_hull_remaining <= 0.0 {
        true
    } else if winner_by_round_limit {
        attacker_hull_remaining >= defender_hull_remaining
    } else {
        false
    };

    SimulationResult {
        total_damage: round_f64(total_damage),
        attacker_won,
        winner_by_round_limit,
        rounds_simulated: rounds_completed,
        attacker_hull_remaining: round_f64(attacker_hull_remaining),
        defender_hull_remaining: round_f64(defender_hull_remaining),
        defender_shield_remaining: round_f64(defender_shield_remaining),
        events: trace.events(),
    }
}

#[derive(Debug, Clone)]
struct EffectAccumulator {
    stacks: StatStacking<EffectStatKey>,
    pre_attack_modifier_sum: f64,
    attack_phase_damage_modifier_sum: f64,
    round_end_modifier_sum: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EffectStatKey {
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
    fn pre_attack_multiplier(&self) -> f64 {
        (1.0 + self.pre_attack_modifier_sum).max(0.0)
    }

    fn pre_attack_pierce_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::PreAttackPierceBonus)
            .unwrap_or(0.0)
    }

    fn defense_mitigation_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::DefenseMitigationBonus)
            .unwrap_or(0.0)
    }

    fn composed_apex_shred_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ApexShredBonus)
            .unwrap_or(0.0)
    }

    fn composed_apex_barrier_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ApexBarrierBonus)
            .unwrap_or(0.0)
    }

    fn composed_shield_regen(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ShieldRegen)
            .unwrap_or(0.0)
    }

    fn composed_hull_regen(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::HullRegen)
            .unwrap_or(0.0)
    }

    fn composed_isolytic_damage_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::IsolyticDamageBonus)
            .unwrap_or(0.0)
    }

    fn composed_isolytic_defense_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::IsolyticDefenseBonus)
            .unwrap_or(0.0)
    }

    fn composed_shield_mitigation_bonus(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::ShieldMitigationBonus)
            .unwrap_or(0.0)
    }

    fn compose_attack_phase_damage(&self, pre_attack_damage: f64) -> f64 {
        self.compose_damage_channel(EffectStatKey::AttackPhaseDamage, pre_attack_damage)
    }

    fn compose_round_end_damage(&self, round_end_damage: f64) -> f64 {
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

    fn set_pre_attack_damage_base(&mut self, base: f64) {
        self.stacks.add(StackContribution::base(
            EffectStatKey::PreAttackDamage,
            base,
        ));
    }

    fn composed_pre_attack_damage(&self) -> f64 {
        self.stacks
            .composed_for(&EffectStatKey::PreAttackDamage)
            .unwrap_or(0.0)
    }
}

impl EffectAccumulator {
    fn add_effects(
        &mut self,
        timing: TimingWindow,
        effects: &[ActiveAbilityEffect],
        base_attack: f64,
        assimilated_active: bool,
    ) {
        for effect in effects {
            self.add_effect(
                timing,
                scale_effect(effect.effect, assimilated_active),
                base_attack,
            );
        }
    }

    fn add_effect(&mut self, timing: TimingWindow, effect: AbilityEffect, base_attack: f64) {
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
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldMitigationBonus, v));
                }
            },
            TimingWindow::AttackPhase => match effect {
                AbilityEffect::AttackMultiplier(modifier) => {
                    self.attack_phase_damage_modifier_sum += modifier;
                }
                // Conversion factor 0.5: pierce bonus in attack phase contributes flat damage as a fraction of base attack (tuning placeholder until STFC formula is confirmed).
                AbilityEffect::PierceBonus(value) => self.stacks.add(StackContribution::flat(
                    EffectStatKey::AttackPhaseDamage,
                    value * base_attack * 0.5,
                )),
                AbilityEffect::Morale(_) => {}
                AbilityEffect::Assimilated { .. } => {}
                AbilityEffect::HullBreach { .. } => {}
                AbilityEffect::Burning { .. } => {}
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
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldMitigationBonus, v));
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
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldMitigationBonus, v));
                }
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
                AbilityEffect::ShieldMitigationBonus(v) => {
                    self.stacks.add(StackContribution::flat(EffectStatKey::ShieldMitigationBonus, v));
                }
            },
        }
    }
}

fn record_ability_activations(
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
        trace.record(CombatEvent {
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

fn scale_effect(effect: AbilityEffect, assimilated_active: bool) -> AbilityEffect {
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
        AbilityEffect::ShieldMitigationBonus(v) => {
            AbilityEffect::ShieldMitigationBonus(v * ASSIMILATED_EFFECTIVENESS_MULTIPLIER)
        }
    }
}

pub fn simulate_once() -> FightResult {
    FightResult { won: true }
}

fn round_f64(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn serialize_source(source: &EventSource) -> Value {
    let mut object = Map::new();
    if let Some(officer_id) = &source.officer_id {
        object.insert("officer_id".to_string(), Value::String(officer_id.clone()));
    }
    if let Some(ship_ability_id) = &source.ship_ability_id {
        object.insert(
            "ship_ability_id".to_string(),
            Value::String(ship_ability_id.clone()),
        );
    }
    if let Some(hostile_ability_id) = &source.hostile_ability_id {
        object.insert(
            "hostile_ability_id".to_string(),
            Value::String(hostile_ability_id.clone()),
        );
    }
    if let Some(player_bonus_source) = &source.player_bonus_source {
        object.insert(
            "player_bonus_source".to_string(),
            Value::String(player_bonus_source.clone()),
        );
    }
    Value::Object(object)
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> =
                map.into_iter().map(|(k, v)| (k, sort_json(v))).collect();
            let ordered = sorted.into_iter().collect::<Map<String, Value>>();
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json).collect()),
        _ => value,
    }
}

fn to_canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let sorted = sort_json(serde_json::to_value(value)?);
    serde_json::to_string_pretty(&sorted)
}
