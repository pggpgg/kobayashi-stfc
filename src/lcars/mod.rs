//! LCARS: Language for Combat Ability Resolution & Simulation.
//!
//! Parses officer ability definitions from YAML and resolves them into a
//! [BuffSet] for the combat engine (static buffs + per-round/triggered effects).

mod canonical_conditions;
pub mod effect_spec_adapter;
mod parser;
mod resolver;

pub use canonical_conditions::{
    canonical_conditions_to_lcars, is_canonical_condition_mapped,
    is_canonical_officer_condition_resolved, map_canonical_condition_token,
};
pub use effect_spec_adapter::{
    collect_lcars_drops, combat_tag_to_stat, lcars_condition_to_spec, lcars_effect_resolved_value,
    lcars_effect_to_combat_effect_spec, lcars_effect_to_combat_effect_spec_with_report,
    DroppedLcarsEffect, LcarsDropReport,
};
pub use parser::{
    load_lcars_dir, load_lcars_file, LcarsAbility, LcarsCondition, LcarsDuration, LcarsEffect,
    LcarsFile, LcarsLevelStats, LcarsOfficer, LcarsScaling,
};
pub use resolver::{
    index_lcars_officers_by_id, lcars_effect_coverage, resolve_crew_to_buff_set,
    resolve_officer_ability, BuffSet, LcarsEffectCoverage, MechanicCoverageTier, ResolveOptions,
};
