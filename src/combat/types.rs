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

/// One **enemy type** label: who or what you are fighting in Star Trek Fleet Command.
///
/// Kobayashi currently simulates one style of fight (player ship vs a single defender with
/// shared round/weapon rules). This enum classifies the **opponent category** (hostile, armada,
/// PvP target, etc.); mechanics, data sources, and UI can branch on it as support lands.
///
/// For PvP variants, read “enemy” as the **opposing player entity** (ship or station).
///
/// An engagement may carry **several** labels at once (e.g. moving hostile plus wave defense). Use
/// [`EnemyTypes`] for the full list; this enum is a single tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EnemyType {
    /// Opposing player ship (space PvP).
    PvpSpace,
    /// Opposing player station.
    PvpStation,
    /// Standard system hostiles (“reds”) — the case the simulator has targeted most so far.
    RedMovingSpace,
    /// Wave defense (often combined with other [`EnemyType`] values in [`EnemyTypes`]).
    Waves,
    /// Mission bosses (“yellows”).
    MissionBosses,
    GroupArmadas,
    SoloArmadas,
    InvadingEntities,
    Assaults,
    OutpostArmadas,
    OutpostRetaliationAttackers,
}

/// Every enemy-type tag that applies to one engagement (hostile row, scenario, import, etc.).
///
/// Serialized as a JSON **array** of snake_case strings, e.g. `["red_moving_space","waves"]`.
/// Order is preserved; callers may use “first = broadest category” as a convention — not enforced
/// here. Duplicate entries are allowed; use [`EnemyTypes::dedup`] if you need uniqueness.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnemyTypes(pub Vec<EnemyType>);

impl Default for EnemyTypes {
    fn default() -> Self {
        Self(vec![EnemyType::RedMovingSpace])
    }
}

impl EnemyTypes {
    pub fn new(tags: Vec<EnemyType>) -> Self {
        Self(tags)
    }

    pub fn single(tag: EnemyType) -> Self {
        Self(vec![tag])
    }

    pub fn contains(&self, tag: EnemyType) -> bool {
        self.0.iter().any(|&t| t == tag)
    }

    /// Same tags, adjacent duplicates collapsed, original order kept.
    pub fn dedup(&mut self) {
        self.0.dedup();
    }

    /// Copy of `self` with [`EnemyTypes::dedup`] applied.
    pub fn deduplicated(mut self) -> Self {
        self.dedup();
        self
    }
}

impl std::ops::Deref for EnemyTypes {
    type Target = [EnemyType];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<EnemyType> for EnemyTypes {
    fn from(tag: EnemyType) -> Self {
        Self::single(tag)
    }
}

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

#[cfg(test)]
mod enemy_types_tests {
    use super::*;

    #[test]
    fn enemy_types_json_is_flat_array() {
        let t = EnemyTypes(vec![EnemyType::RedMovingSpace, EnemyType::Waves]);
        let j = serde_json::to_string(&t).unwrap();
        assert_eq!(j, r#"["red_moving_space","waves"]"#);
        let back: EnemyTypes = serde_json::from_str(&j).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn default_enemy_types_is_red_moving_only() {
        let d = EnemyTypes::default();
        assert_eq!(d.len(), 1);
        assert!(d.contains(EnemyType::RedMovingSpace));
    }

    #[test]
    fn contains_and_from_single() {
        let t: EnemyTypes = EnemyType::SoloArmadas.into();
        assert!(t.contains(EnemyType::SoloArmadas));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn dedup_adjacent_only() {
        let mut t = EnemyTypes(vec![
            EnemyType::Waves,
            EnemyType::Waves,
            EnemyType::RedMovingSpace,
            EnemyType::Waves,
        ]);
        t.dedup();
        assert_eq!(
            t.0,
            vec![
                EnemyType::Waves,
                EnemyType::RedMovingSpace,
                EnemyType::Waves,
            ]
        );
    }
}
