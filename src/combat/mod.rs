pub mod abilities;
pub mod buffs;
pub mod engine;
pub mod rng;
pub mod stacking;

pub use engine::{
    component_mitigation, mitigation, serialize_events_json, simulate_combat, AttackerStats,
    CombatEvent, Combatant, DefenderStats, EventSource, ShipType, SimulationConfig,
    SimulationResult, TraceCollector, TraceMode, BATTLESHIP_COEFFICIENTS, EPSILON,
    EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS, SURVEY_COEFFICIENTS,
};
pub use stacking::{
    aggregate_contributions, compose_totals, CategoryTotals, StackCategory, StackContribution,
    StatStacking,
};
