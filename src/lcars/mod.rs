//! LCARS: Language for Combat Ability Resolution & Simulation.
//!
//! Parses officer ability definitions from YAML and resolves them into a
//! [BuffSet] for the combat engine (static buffs + per-round/triggered effects).

mod parser;
mod resolver;

pub use parser::{load_lcars_dir, load_lcars_file, LcarsAbility, LcarsEffect, LcarsFile, LcarsOfficer};
pub use resolver::{
    index_lcars_officers_by_id, resolve_crew_to_buff_set, resolve_officer_ability, BuffSet,
    ResolveOptions,
};
