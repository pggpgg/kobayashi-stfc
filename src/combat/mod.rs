pub mod abilities;
pub mod buffs;
pub mod engine;
pub mod rng;

pub use engine::{
    component_mitigation, mitigation, AttackerStats, DefenderStats, ShipType,
    BATTLESHIP_COEFFICIENTS, EPSILON, EXPLORER_COEFFICIENTS, INTERCEPTOR_COEFFICIENTS,
    SURVEY_COEFFICIENTS,
};
