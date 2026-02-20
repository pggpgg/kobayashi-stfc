use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::combat::abilities::{active_effects_for_timing, AbilityEffect, CrewConfiguration, TimingWindow};
use crate::combat::rng::Rng;

/// Combat mitigation parity implementation migrated from
/// `tools/combat_engine/mitigation.py`.
///
/// The formulas and clamps in this module intentionally mirror the Python
/// reference behavior exactly.
#[derive(Debug, Clone, Copy)]
pub struct FightResult {
    pub won: bool,
}

pub const EPSILON: f64 = 1e-9;

pub const SURVEY_COEFFICIENTS: (f64, f64, f64) = (0.3, 0.3, 0.3);
pub const BATTLESHIP_COEFFICIENTS: (f64, f64, f64) = (0.55, 0.2, 0.2);
pub const EXPLORER_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.55, 0.2);
pub const INTERCEPTOR_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.2, 0.55);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipType {
    Survey,
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
    pub events: Vec<CombatEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combatant {
    pub id: String,
    pub attack: f64,
    pub mitigation: f64,
    pub pierce: f64,
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

    for round_index in 1..=config.rounds {
        let round_start_effects = active_effects_for_timing(attacker_crew, TimingWindow::RoundStart);
        let attack_phase_effects = active_effects_for_timing(attacker_crew, TimingWindow::AttackPhase);

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

        for effect in &attack_phase_effects {
            trace.record(CombatEvent {
                event_type: "ability_activation".to_string(),
                round_index,
                phase: "attack".to_string(),
                source: EventSource {
                    officer_id: Some(attacker.id.clone()),
                    ship_ability_id: Some(effect.ability_name.clone()),
                    ..EventSource::default()
                },
                values: Map::from_iter([("boosted".to_string(), Value::Bool(effect.boosted))]),
            });
        }

        let (attack_multiplier, pierce_bonus) = aggregate_attack_modifiers(&attack_phase_effects);
        let effective_attack = attacker.attack * attack_multiplier;
        let effective_pierce = attacker.pierce + pierce_bonus;

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

        let effective_mitigation = (mitigation_multiplier + effective_pierce).max(0.0);
        trace.record(CombatEvent {
            event_type: "pierce_calc".to_string(),
            round_index,
            phase: "attack".to_string(),
            source: EventSource {
                officer_id: Some(attacker.id.clone()),
                player_bonus_source: Some("research:weapon_tech".to_string()),
                ..EventSource::default()
            },
            values: Map::from_iter([
                ("pierce".to_string(), Value::from(effective_pierce)),
                (
                    "effective_mitigation".to_string(),
                    Value::from(round_f64(effective_mitigation)),
                ),
            ]),
        });

        total_damage += effective_attack * effective_mitigation;
    }

    SimulationResult {
        total_damage: round_f64(total_damage),
        events: trace.events(),
    }
}

fn aggregate_attack_modifiers(effects: &[crate::combat::abilities::ActiveAbilityEffect]) -> (f64, f64) {
    let mut attack_multiplier = 1.0;
    let mut pierce_bonus = 0.0;

    for effect in effects {
        match effect.effect {
            AbilityEffect::AttackMultiplier(modifier) => {
                attack_multiplier *= 1.0 + modifier;
            }
            AbilityEffect::PierceBonus(value) => {
                pierce_bonus += value;
            }
        }
    }

    (attack_multiplier.max(0.0), pierce_bonus)
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
