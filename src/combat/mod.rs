pub mod abilities;
pub mod buffs;
pub mod engine;
pub mod export_csv;
pub mod log_ingest;
pub mod rng;
pub mod stacking;

pub use abilities::{
    active_effects_for_timing, can_activate_in_seat, Ability, AbilityClass, AbilityEffect,
    ActiveAbilityEffect, CrewConfiguration, CrewSeat, CrewSeatContext, TimingWindow,
};
pub use engine::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_with_morale, pierce_damage_through_bonus, serialize_events_json, simulate_combat,
    AttackerStats, CombatEvent, Combatant, DefenderStats, EventSource, ShipType, SimulationConfig,
    SimulationResult, TraceCollector, TraceMode, BATTLESHIP_COEFFICIENTS, EPSILON,
    EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS, MORALE_PRIMARY_PIERCING_BONUS, PIERCE_CAP,
    SURVEY_COEFFICIENTS,
};
pub use export_csv::{
    export_to_combatants, export_to_attacker, export_to_defender, parse_fight_export,
    ship_type_from_name, FightExport, FightExportEvent,
};
pub use log_ingest::{
    ingested_events_to_combat_events, ingested_to_comparable, parse_combat_log_json,
    parity_within_tolerance, IngestedCombatLog, IngestedEvent,
};
pub use stacking::{
    aggregate_contributions, compose_totals, CategoryTotals, StackCategory, StackContribution,
    StatStacking,
};
