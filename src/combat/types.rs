//! Shared combat types and constants.

use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;

/// Combat mitigation parity implementation migrated from
/// `tools/combat_engine/mitigation.py`.
///
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
        if fl_u.is_multiple_of(2) {
            fl_u
        } else {
            fl_u + 1
        }
    }
}

/// Effective shots for one weapon in a combat round: `round_half_even(n_w0 * (1 + B_shots))`.
///
/// `shots_bonus_sum` is the summed fractional bonus from active [`AbilityEffect::ShotsBonus`] entries
/// (e.g. `0.1` => +10%). Pass `0.0` when modeling a combatant with no shots bonus (e.g. hostile
/// counter-fire today, where only the player crew contributes `B_shots` in the combat engine).
#[inline]
pub fn effective_shots_for_weapon(base_shots: u32, shots_bonus_sum: f64) -> u32 {
    round_half_even(base_shots as f64 * (1.0 + shots_bonus_sum))
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
        self.0.contains(&tag)
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
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

/// Major hostile / opponent faction for gating ship abilities ("against Klingon", etc.).
/// Derived from data.stfc.space hostile `faction` + `translations-factions` `faction_name` loca ids.
/// Unmapped or missing faction → [`OpponentFactionTag::Unknown`] (faction-gated abilities do not fire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpponentFactionTag {
    #[default]
    Unknown,
    Federation,
    Klingon,
    Romulan,
    Borg,
    Cardassian,
    Augment,
    Dominion,
    MirrorUniverse,
    Assimilated,
    ExBorg,
    Swarm,
    Actian,
    GornHuntingPack,
    Xindi,
    Breen,
}

impl OpponentFactionTag {
    /// Parse hostile / ship-catalog / LCARS faction slug (`klingon`, `mirror_universe`, `gorn`, …).
    /// Unknown slugs return `None` (callers skip faction gating rather than matching every defender).
    pub fn from_data_slug(s: &str) -> Option<Self> {
        let k = s.trim().to_lowercase().replace('-', "_");
        match k.as_str() {
            "unknown" => Some(Self::Unknown),
            "federation" => Some(Self::Federation),
            "klingon" => Some(Self::Klingon),
            "romulan" => Some(Self::Romulan),
            "borg" => Some(Self::Borg),
            "cardassian" => Some(Self::Cardassian),
            "augment" => Some(Self::Augment),
            "dominion" => Some(Self::Dominion),
            "mirror_universe" => Some(Self::MirrorUniverse),
            "assimilated" => Some(Self::Assimilated),
            "ex_borg" | "exborg" => Some(Self::ExBorg),
            "swarm" => Some(Self::Swarm),
            "actian" => Some(Self::Actian),
            "gorn_hunting_pack" | "gorn" => Some(Self::GornHuntingPack),
            "xindi" => Some(Self::Xindi),
            "breen" => Some(Self::Breen),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceMode {
    Off,
    Events,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub rounds: u32,
    pub seed: u64,
    pub trace_mode: TraceMode,
    /// Hull damage already applied to the attacker before round 1 (chain grinding: carry-over HHP).
    /// Clamped to `[0, attacker.hull_health]` in the engine. Default 0.
    #[serde(default)]
    pub initial_attacker_hull_damage: f64,
    /// **Experimental:** When `Some(p)`, `p` is the profile `weapon_damage` additive bonus (same units
    /// as [`crate::data::profile::get_bonus`] — fraction added by `apply_profile_to_attacker`, e.g. `0.65` = +65%).
    /// Outgoing shot damage uses `weapon_attack * (p + pre_attack_multiplier) / (1 + p)` instead of
    /// `weapon_attack * pre_attack_multiplier`, i.e. profile bonus and dynamic `pre_attack_modifier_sum`
    /// share one pool `(1 + p + sum)` on the pre-profile weapon base. See `docs/GALAXY_CLASS_DAMAGE_STACKING_FINDINGS.md`.
    #[serde(default)]
    pub weapon_damage_profile_additive_pool: Option<f64>,
    /// Profile `weapon_damage` additive bonus `p` (fraction, e.g. `0.65` = +65%), same units as
    /// [`crate::data::profile::apply_profile_to_attacker`]. Used to dilute
    /// [`crate::combat::abilities::AbilityEffect::GalaxyAdditiveWeaponDamageGrowth`] as `×(1+g/(1+p))`.
    #[serde(default)]
    pub profile_weapon_damage_fraction: f64,
    /// Upstream defender hostile `faction.id` for canonical `EnemyHullFaction` / [`crate::combat::abilities::AbilityCondition::DefenderHullFactionIdIs`]. `0` when unknown or not loaded.
    #[serde(default)]
    pub defender_hull_faction_id: i64,
    /// Bitmask from [`crate::data::hostile::HostileRecord::hostile_tag_mask`] / [`crate::combat::hostile_tags`]; `0` when unset or non-hostile defender.
    #[serde(default)]
    pub defender_hostile_tag_mask: u32,
    /// Player hull owner faction for [`crate::combat::abilities::AbilityCondition::AttackerOwnerFactionIs`] and dual-gated research seats.
    #[serde(default)]
    pub attacker_owner_faction: OpponentFactionTag,
    /// STFC engagement category tags (solo vs group armada, wave defense, etc.) for officer [`crate::combat::abilities::AbilityCondition::EngagementIncludes`].
    /// Default is only [`EnemyType::RedMovingSpace`]; set from [`crate::data::hostile::HostileRecord::engagement_enemy_types`] when curated.
    #[serde(default)]
    pub engagement_enemy_types: EnemyTypes,
    /// Optional hostile level for canonical `TargetMaxLevel` (`AbilityCondition::DefenderLevelAtMost`).
    #[serde(default)]
    pub defender_level: Option<u32>,
    /// Canonical officer ids assigned to the attacker (captain, bridge, below) for
    /// **Evolutionary Assimilation** vs Conqueror Borg: when any id matches the curated forbidden
    /// roster and beam suppression is inactive, combat ends in instant hull loss. Empty: do not
    /// apply this mechanic.
    #[serde(default)]
    pub attacker_roster_officer_ids: Vec<String>,
    /// Additive shield mitigation fraction for damage **to the player ship** on hostile counter-fire
    /// (incoming), for combat rounds `1..=incoming_shield_mitigation_bonus_rounds` inclusive.
    /// Does not affect outbound shots or catalog/profile flat `shield_mitigation` alone.
    #[serde(default)]
    pub incoming_shield_mitigation_bonus: f64,
    /// When zero, [`Self::incoming_shield_mitigation_bonus`] is ignored. Otherwise rounds are 1-based
    /// (round 1 = first combat round).
    #[serde(default)]
    pub incoming_shield_mitigation_bonus_rounds: u32,
    /// When `true` and [`Self::trace_mode`] is [`TraceMode::Events`], emit `state_snapshot` trace rows
    /// ([`crate::combat::snapshot::CombatStateSnapshot`]). Ignored when tracing is off.
    #[serde(default)]
    pub emit_state_snapshots: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            rounds: 3,
            seed: 7,
            trace_mode: TraceMode::Off,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id: 0,
            defender_hostile_tag_mask: 0,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: EnemyTypes::default(),
            defender_level: None,
            attacker_roster_officer_ids: Vec::new(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            emit_state_snapshots: false,
        }
    }
}

/// Parse `snake_case` slug from LCARS `engagement_includes` / JSON (same strings as [`EnemyType`] serde).
pub fn enemy_type_from_engagement_slug(s: &str) -> Option<EnemyType> {
    let slug = s.trim().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(slug)).ok()
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
    /// Attacker shield HP remaining at end of combat (replay/debug; chain mode resets SHP each link).
    #[serde(default)]
    pub attacker_shield_remaining: f64,
    pub events: Vec<CombatEvent>,
    /// When true, attacker combat-begin effects included [`AbilityEffect::ConquerorBorgBeamSuppression`] vs a tagged Conqueror Borg defender. Instant-kill beam resolution reads this when implemented.
    #[serde(default)]
    pub conqueror_borg_beam_suppression: bool,
}

/// Per-weapon stats for sub-round resolution. Optional fields override [`Combatant`] ship-level
/// pierce/crit/proc for that weapon index only; unset → use combatant defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WeaponStats {
    pub attack: f64,
    /// Base shots per weapon per round (n_w,0). When absent, 1. Effective shots = [`effective_shots_for_weapon`] with the round's `B_shots` sum.
    #[serde(default)]
    pub shots: Option<u32>,
    /// Damage-through pierce bonus for this weapon (same units as [`Combatant::pierce`]).
    #[serde(default)]
    pub pierce: Option<f64>,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    /// Per-weapon crit damage tier when set; else [`Combatant::crit_multiplier`].
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub proc_chance: Option<f64>,
    #[serde(default)]
    pub proc_multiplier: Option<f64>,
}

/// Parameters for dynamic hostile mitigation computation at combat time.
/// When present on a [`Combatant`], the engine calls [`crate::combat::mitigation::mitigation_for_hostile`]
/// per-shot with morale-adjusted attacker stats instead of using the pre-computed `mitigation` scalar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostileMitigationParams {
    pub defender_stats: DefenderStats,
    pub base_attacker_stats: AttackerStats,
    pub ship_type: ShipType,
    pub mystery_mitigation_factor: f64,
    pub floor: f64,
    pub ceiling: f64,
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
    #[serde(default)]
    pub shield_health: f64,
    #[serde(default = "default_shield_mitigation")]
    pub shield_mitigation: f64,
    #[serde(default)]
    pub apex_barrier: f64,
    #[serde(default)]
    pub apex_shred: f64,
    #[serde(default)]
    pub isolytic_damage: f64,
    #[serde(default)]
    pub isolytic_defense: f64,
    #[serde(default)]
    pub weapons: Vec<WeaponStats>,
    /// When set, enables per-shot dynamic mitigation using `mitigation_for_hostile`.
    /// Morale piercing bonus and MitigationAdditive effects are applied at combat time.
    #[serde(skip)]
    pub hostile_mitigation_params: Option<HostileMitigationParams>,
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

    fn weapon_row(&self, weapon_index: usize) -> Option<&WeaponStats> {
        self.weapons.get(weapon_index)
    }

    pub fn weapon_pierce(&self, weapon_index: usize) -> f64 {
        self.weapon_row(weapon_index)
            .and_then(|w| w.pierce)
            .unwrap_or(self.pierce)
    }

    pub fn weapon_crit_chance(&self, weapon_index: usize) -> f64 {
        self.weapon_row(weapon_index)
            .and_then(|w| w.crit_chance)
            .unwrap_or(self.crit_chance)
    }

    pub fn weapon_crit_multiplier(&self, weapon_index: usize) -> f64 {
        self.weapon_row(weapon_index)
            .and_then(|w| w.crit_multiplier)
            .unwrap_or(self.crit_multiplier)
    }

    pub fn weapon_proc_chance(&self, weapon_index: usize) -> f64 {
        self.weapon_row(weapon_index)
            .and_then(|w| w.proc_chance)
            .unwrap_or(self.proc_chance)
    }

    pub fn weapon_proc_multiplier(&self, weapon_index: usize) -> f64 {
        self.weapon_row(weapon_index)
            .and_then(|w| w.proc_multiplier)
            .unwrap_or(self.proc_multiplier)
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

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn events(self) -> Vec<CombatEvent> {
        self.events
    }
}

impl ShipType {
    /// Parse slug from ship catalogs, hull ability conditions, or LCARS (`battleship`, `explorer`, …).
    pub fn from_data_slug(s: &str) -> Option<Self> {
        let k = s.trim().to_lowercase().replace('-', "_");
        match k.as_str() {
            "battleship" => Some(Self::Battleship),
            "explorer" => Some(Self::Explorer),
            "interceptor" => Some(Self::Interceptor),
            "survey" => Some(Self::Survey),
            "armada" => Some(Self::Armada),
            _ => None,
        }
    }

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
