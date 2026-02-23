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
    pub events: Vec<CombatEvent>,
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
    /// Defender stat: reduces damage after other mitigation. Effective barrier is divided by (1 + attacker apex_shred).
    #[serde(default)]
    pub apex_barrier: f64,
    /// Attacker stat: reduces defender's effective apex_barrier. Stored as decimal (1.0 = 100%).
    #[serde(default)]
    pub apex_shred: f64,
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
    let mut total_damage = 0.0;
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
    let effective_barrier = defender.apex_barrier
        / (1.0 + attacker.apex_shred).max(EPSILON);
    let apex_damage_factor = 10000.0 / (10000.0 + effective_barrier);

    for round_index in 1..=rounds_to_simulate {
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
                    values: Map::from_iter([
                        ("roll".to_string(), Value::from(round_f64(burning_roll))),
                        ("triggered".to_string(), Value::Bool(triggered)),
                        ("chance".to_string(), Value::from(round_f64(chance))),
                        ("duration_rounds".to_string(), Value::from(duration_rounds)),
                    ]),
                });
            }
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
        phase_effects.add_effects(
            TimingWindow::AttackPhase,
            &attack_phase_effects,
            attacker.attack,
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
        phase_effects.add_effects(
            TimingWindow::DefensePhase,
            &defense_phase_effects,
            attacker.attack,
            defense_phase_assimilated,
        );

        let round_end_assimilated = assimilated_rounds_remaining > 0;
        record_ability_activations(
            &mut trace,
            round_index,
            "round_end",
            attacker,
            &round_end_effects,
            round_end_assimilated,
        );
        phase_effects.add_effects(
            TimingWindow::RoundEnd,
            &round_end_effects,
            attacker.attack,
            round_end_assimilated,
        );

        let effective_attack = attacker.attack * phase_effects.pre_attack_multiplier();
        let morale_source = round_start_effects.iter().find_map(|effect| {
            if let AbilityEffect::Morale(chance) =
                scale_effect(effect.effect, round_start_assimilated)
            {
                Some((effect.ability_name.clone(), chance.clamp(0.0, 1.0)))
            } else {
                None
            }
        });
        let mut effective_pierce = attacker.pierce + phase_effects.pre_attack_pierce_bonus();
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

        let roll = (rng.next_u64() as f64) / (u64::MAX as f64);
        trace.record(CombatEvent {
            event_type: "attack_roll".to_string(),
            round_index,
            phase: "attack".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                ..EventSource::default()
            },
            values: Map::from_iter([
                ("roll".to_string(), Value::from(round_f64(roll))),
                ("base_attack".to_string(), Value::from(attacker.attack)),
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
        total_damage += damage_after_apex;
        trace.record(CombatEvent {
            event_type: "damage_application".to_string(),
            round_index,
            phase: "damage".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                hostile_ability_id: Some(format!("{}_hull", defender.id)),
                ..EventSource::default()
            },
            values: Map::from_iter([
                ("final_damage".to_string(), Value::from(round_f64(damage_after_apex))),
                (
                    "running_total".to_string(),
                    Value::from(round_f64(total_damage)),
                ),
                (
                    "assimilated_active".to_string(),
                    Value::Bool(assimilated_rounds_remaining > 0),
                ),
            ]),
        });

        let bonus_damage = phase_effects.compose_round_end_damage(attacker.end_of_round_damage);
        // Burning: 1% of total (max) hull per round, not remaining (per STFC).
        let burning_damage = if burning_rounds_remaining > 0 {
            defender.hull_health.max(0.0) * BURNING_HULL_DAMAGE_PER_ROUND
        } else {
            0.0
        };
        total_damage += (bonus_damage + burning_damage) * apex_damage_factor;

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
                    "running_total".to_string(),
                    Value::from(round_f64(total_damage)),
                ),
            ]),
        });
    }

    let attacker_hull_remaining = attacker.hull_health.max(0.0);
    let defender_hull_remaining = (defender.hull_health - total_damage).max(0.0);
    let winner_by_round_limit =
        rounds_to_simulate == MAX_COMBAT_ROUNDS && defender_hull_remaining > 0.0;
    let attacker_won = if winner_by_round_limit {
        attacker_hull_remaining >= defender_hull_remaining
    } else {
        defender_hull_remaining <= 0.0
    };

    SimulationResult {
        total_damage: round_f64(total_damage),
        attacker_won,
        winner_by_round_limit,
        rounds_simulated: rounds_to_simulate,
        attacker_hull_remaining: round_f64(attacker_hull_remaining),
        defender_hull_remaining: round_f64(defender_hull_remaining),
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
