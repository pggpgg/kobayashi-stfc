//! Shared combat types and constants.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;

/// Combat mitigation parity implementation migrated from
/// `tools/combat_engine/mitigation.py`.
///
/// The formulas and clamps in this module intentionally mirror the Python
/// reference behavior exactly.
#[derive(Debug, Clone)]
pub struct FightResult {
    pub won: bool,
}

pub const EPSILON: f64 = 1e-9;

/// Bankers rounding (round half to even). Used for shots per weapon: n_w(r) = round_half_even(n_w0 * (1 + B_shots)).
#[inline]
pub fn round_half_even(x: f64) -> u32 {
    let fl = x.floor();
    let frac = x - fl;
    let fl_u = fl as u32;
    if frac < 0.5 {
        fl_u
    } else if frac > 0.5 {
        fl_u + 1
    } else {
        // tie: round to nearest even
        if fl_u % 2 == 0 {
            fl_u
        } else {
            fl_u + 1
        }
    }
}
pub const MAX_COMBAT_ROUNDS: u32 = 100;
pub const MORALE_PRIMARY_PIERCING_BONUS: f64 = 0.10;
/// When target has Hull Breach, critical damage is multiplied by this factor (per game rules).
pub const HULL_BREACH_CRIT_BONUS: f64 = 1.5;
pub const BURNING_HULL_DAMAGE_PER_ROUND: f64 = 0.01;
pub const ASSIMILATED_EFFECTIVENESS_MULTIPLIER: f64 = 0.75;

pub const SURVEY_COEFFICIENTS: (f64, f64, f64) = (0.3, 0.3, 0.3);
pub const BATTLESHIP_COEFFICIENTS: (f64, f64, f64) = (0.55, 0.2, 0.2);
pub const EXPLORER_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.55, 0.2);
pub const INTERCEPTOR_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.2, 0.55);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipType {
    Survey,
    Armada,
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
    /// Sub-round (weapon) index when tracing multi-weapon resolution. Omitted when None.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub weapon_index: Option<u32>,
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
    pub attacker_won: bool,
    pub winner_by_round_limit: bool,
    pub rounds_simulated: u32,
    pub attacker_hull_remaining: f64,
    pub defender_hull_remaining: f64,
    /// Defender shield HP remaining at end of combat (0 when shields were depleted).
    #[serde(default)]
    pub defender_shield_remaining: f64,
    pub events: Vec<CombatEvent>,
}

/// Per-weapon stats for sub-round resolution. Combatant-level pierce/crit/proc apply to all weapons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponStats {
    pub attack: f64,
    /// Base shots per weapon per round (n_w,0). When absent, 1. Effective shots = round_half_even(shots * (1 + B_shots)).
    #[serde(default)]
    pub shots: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Combatant {
    pub id: String,
    pub attack: f64,
    pub mitigation: f64,
    pub pierce: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub proc_chance: f64,
    pub proc_multiplier: f64,
    pub end_of_round_damage: f64,
    pub hull_health: f64,
    /// Maximum shield hit points. When defending, damage is split between shield and hull by shield_mitigation until shields are depleted.
    #[serde(default)]
    pub shield_health: f64,
    /// Fraction of incoming (unmitigated, post-apex) damage that goes to shield; rest goes to hull. Base 0.8 (80% shields, 20% hull). When shields are depleted, all damage goes to hull.
    #[serde(default = "default_shield_mitigation")]
    pub shield_mitigation: f64,
    /// Defender stat: reduces damage after other mitigation. Effective barrier is divided by (1 + attacker apex_shred).
    #[serde(default)]
    pub apex_barrier: f64,
    /// Attacker stat: reduces defender's effective apex_barrier. Stored as decimal (1.0 = 100%).
    #[serde(default)]
    pub apex_shred: f64,
    /// Attacker: isolytic damage bonus (decimal, e.g. 0.15 = 15% of regular damage as isolytic). Used in isolytic_damage().
    #[serde(default)]
    pub isolytic_damage: f64,
    /// Defender: multiplicative isolytic mitigation. Isolytic taken = Isolytic Damage / (1 + isolytic_defense). Applied after isolytic_damage().
    #[serde(default)]
    pub isolytic_defense: f64,
    /// Per-weapon attack values for sub-round resolution. If empty, one weapon with scalar `attack` is used (backward compat).
    #[serde(default)]
    pub weapons: Vec<WeaponStats>,
}

fn default_shield_mitigation() -> f64 {
    0.8
}

impl Combatant {
    /// Number of weapons (sub-rounds per round). Empty weapons list is treated as one weapon using scalar `attack`.
    pub fn weapon_count(&self) -> usize {
        self.weapons.len().max(1)
    }

    /// Base shots per weapon per round (n_w,0). Default 1 when not set.
    pub fn weapon_base_shots(&self, weapon_index: usize) -> u32 {
        if self.weapons.is_empty() {
            if weapon_index == 0 {
                1
            } else {
                0
            }
        } else if let Some(w) = self.weapons.get(weapon_index) {
            w.shots.unwrap_or(1)
        } else {
            0
        }
    }

    /// Attack value for weapon at index. Returns None if index >= weapon_count (caller should not fire).
    pub fn weapon_attack(&self, weapon_index: usize) -> Option<f64> {
        if self.weapons.is_empty() {
            if weapon_index == 0 {
                Some(self.attack)
            } else {
                None
            }
        } else {
            self.weapons.get(weapon_index).map(|w| w.attack)
        }
    }
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

    /// Records an event only when tracing is enabled. The closure is not called when disabled,
    /// avoiding allocation and construction of CombatEvent when TraceMode::Off.
    pub fn record_if(&mut self, f: impl FnOnce() -> CombatEvent) {
        if self.enabled {
            self.events.push(f());
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
            Self::Armada => SURVEY_COEFFICIENTS,
            Self::Battleship => BATTLESHIP_COEFFICIENTS,
            Self::Explorer => EXPLORER_COEFFICIENTS,
            Self::Interceptor => INTERCEPTOR_COEFFICIENTS,
        }
    }
}
