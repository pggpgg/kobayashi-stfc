pub mod abilities;
pub mod buffs;
pub mod engine;
pub mod rng;

pub use engine::{
    component_mitigation, mitigation, serialize_events_json, simulate_combat, AttackerStats,
    CombatEvent, Combatant, DefenderStats, EventSource, ShipType, SimulationConfig,
    SimulationResult, TraceCollector, TraceMode, BATTLESHIP_COEFFICIENTS, EPSILON,
    EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS, SURVEY_COEFFICIENTS,
};
