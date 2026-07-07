pub mod abilities;
pub mod condition;
pub mod conqueror_borg_beams;
pub mod crit;
pub mod damage;
pub mod effect_accumulator;
pub mod effect_spec_compile;
pub mod engine;
pub mod events;
pub mod evolutionary_assimilation;
pub mod export_csv;
pub mod hostile_tags;
pub mod log_import_normalize;
pub mod log_ingest;
pub mod log_validate;
pub mod mitigation;
pub mod mitigation_sensitivity;
pub mod perturb;
pub mod proc;
pub mod rng;
pub mod simd_damage_kernel;
pub mod snapshot;
pub mod stacking;
pub mod types;

pub use abilities::{
    active_effects_for_timing, apply_duplicate_officer_policy,
    attacker_crew_tal_assigned_captain_or_bridge, can_activate_in_seat,
    defender_shield_drain_per_round_from_crew, hostile_apex_barrier_bonus_from_defender_crew,
    hostile_crit_damage_floor_bonus_from_defender_crew,
    hostile_defender_mitigation_additive_factor_from_defender_crew,
    hostile_hyperthermic_decay_fraction_from_defender_crew, Ability, AbilityClass,
    AbilityCondition, AbilityEffect, ActiveAbilityEffect, CombatContext, CrewConfiguration,
    CrewSeat, CrewSeatContext, TimingWindow, WeaponTypeScope, NO_EXPLICIT_CONTRIBUTION_BATCH,
    TAL_OFFICER_LCARS_ID,
};
pub use damage::{
    apply_shield_hull_split, compute_apex_damage_factor, compute_damage_through_factor,
    compute_isolytic_taken,
};
pub use engine::{
    apply_morale_primary_piercing, build_combat_setup, build_combat_setup_with_officer_stat,
    component_mitigation, effective_shots_for_weapon, isolytic_damage, mitigation,
    mitigation_for_hostile, mitigation_with_morale, mitigation_with_mystery,
    pierce_damage_through_bonus, round_half_even, serialize_events_json, simulate_combat,
    simulate_combat_batch, simulate_combat_from_setup, simulate_combat_with_defender_faction,
    simulate_combat_with_defender_faction_and_defender_crew, AttackerStats, CombatEvent, Combatant,
    CrewOfficerStatTotals, DefenderStats, EventSource, OpponentFactionTag, PreCombatSetup,
    ShipType, SimulationConfig, SimulationResult, TraceCollector, TraceMode, WeaponStats,
    BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS,
    MITIGATION_CEILING, MITIGATION_FLOOR, MORALE_PRIMARY_PIERCING_BONUS, PIERCE_CAP,
    SURVEY_COEFFICIENTS,
};
pub use evolutionary_assimilation::EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS;
pub use export_csv::{
    export_to_attacker, export_to_combat_input, export_to_combatants, export_to_crew,
    export_to_defender, parse_fight_export, ship_type_from_name, FightExport, FightExportEvent,
};
pub use hostile_tags::{
    HOSTILE_TAG_MASK_AGGREGATION_HOSTILE, HOSTILE_TAG_MASK_CONQUEROR_BORG,
    HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR, HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR,
    HOSTILE_TAG_MASK_GORN_HUNTER,
};
pub use log_import_normalize::{
    expand_collapsed_repeat_events, tag_stats_snapshot_sources_client_default,
};
pub use log_ingest::{
    compare_ingested_trace_to_simulator, hydrate_ingested_state_snapshots_from_values,
    ingested_events_to_combat_events, ingested_to_comparable, parity_within_tolerance,
    parse_combat_log_json, trace_event_matches_skeleton, try_event_state_snapshot,
    IngestedCombatLog, IngestedEvent, TraceCompareOptions,
};
pub use log_validate::{validate_canonical_timeline, TimelineValidationOutcome};
pub use mitigation_sensitivity::{
    default_percent_sensitivity_rows, direct_scalar_row, format_sensitivity_tsv,
    HostileMitigationBaseline, MitigationSensitivityRow,
};
pub use perturb::{apply_perturbation, StatKey};
pub use snapshot::{
    state_snapshot_as_combat_event, CombatSnapshotFlags, CombatStateSnapshot,
    CombatantSnapshotResources, SnapshotAnchor,
};
pub use stacking::{
    aggregate_contributions, compose_totals, CategoryTotals, StackCategory, StackContribution,
    StatStacking,
};
pub use types::{EnemyType, EnemyTypes, HostileMitigationParams, WeaponType};
