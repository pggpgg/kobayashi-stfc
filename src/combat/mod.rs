pub mod abilities;
pub mod buffs;
pub mod engine;
pub mod rng;
pub mod stacking;

pub use abilities::{
    active_effects_for_timing, can_activate_in_seat, Ability, AbilityClass, AbilityEffect,
    ActiveAbilityEffect, CrewConfiguration, CrewSeat, CrewSeatContext, TimingWindow,
};
pub use engine::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_with_morale, serialize_events_json, simulate_combat, AttackerStats, CombatEvent,
    Combatant, DefenderStats, EventSource, ShipType, SimulationConfig, SimulationResult,
    TraceCollector, TraceMode, BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS,
    INTERCEPTOR_COEFFICIENTS, MORALE_PRIMARY_PIERCING_BONUS, SURVEY_COEFFICIENTS,
};
pub use stacking::{
    aggregate_contributions, compose_totals, CategoryTotals, StackCategory, StackContribution,
    StatStacking,
};
