pub mod abilities;
pub mod buffs;
pub mod damage;
pub mod effect_accumulator;
pub mod engine;
pub mod events;
pub mod export_csv;
pub mod mitigation;
pub mod mitigation_sensitivity;
pub mod types;
pub mod log_ingest;
pub mod rng;
pub mod stacking;

pub use abilities::{
    active_effects_for_timing, apply_duplicate_officer_policy, can_activate_in_seat, Ability,
    AbilityClass, AbilityCondition, AbilityEffect, ActiveAbilityEffect, CombatContext,
    CrewConfiguration, CrewSeat, CrewSeatContext, TimingWindow, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
pub use engine::{
    apply_morale_primary_piercing, component_mitigation, isolytic_damage, mitigation,
    mitigation_for_hostile, mitigation_with_morale, mitigation_with_mystery,     pierce_damage_through_bonus, round_half_even, serialize_events_json, simulate_combat,
    AttackerStats, CombatEvent, Combatant, DefenderStats, EventSource, ShipType, SimulationConfig,
    SimulationResult, TraceCollector, TraceMode, WeaponStats,
    BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS,
    MITIGATION_CEILING, MITIGATION_FLOOR, MORALE_PRIMARY_PIERCING_BONUS, PIERCE_CAP,
    SURVEY_COEFFICIENTS,
};
pub use damage::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_damage_through_factor,
    compute_isolytic_taken,
};
pub use mitigation_sensitivity::{
    default_percent_sensitivity_rows, format_sensitivity_tsv, HostileMitigationBaseline,
    MitigationSensitivityRow,
};
pub use export_csv::{
    export_to_combat_input, export_to_combatants, export_to_attacker, export_to_crew,
    export_to_defender, parse_fight_export, ship_type_from_name, FightExport, FightExportEvent,
};
pub use log_ingest::{
    ingested_events_to_combat_events, ingested_to_comparable, parse_combat_log_json,
    parity_within_tolerance, IngestedCombatLog, IngestedEvent,
};
pub use stacking::{
    aggregate_contributions, compose_totals, CategoryTotals, StackCategory, StackContribution,
    StatStacking,
};
