//! Recorded-fight-style calibration fixtures: run the combat engine and compare results to reference bands.
//! Used by tests and suitable for future CLI "drift report" tooling.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, TimingWindow, TraceMode, WeaponStats,
};

#[derive(Debug, Clone, Deserialize)]
pub struct DriftFixtureFile {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Provenance of the reference bands: "mathematical invariant", "community-verified",
    /// "estimated from fight exports", or a placeholder note when wiring is incomplete.
    #[serde(default)]
    pub source: Option<String>,
    pub attacker: FixtureCombatant,
    pub defender: FixtureCombatant,
    pub simulation: FixtureSimulation,
    pub bands: MetricBands,
    #[serde(default)]
    pub expect_attacker_won: Option<bool>,
    /// Optional minimal crew so drift scenarios can exercise profile weapon-damage pooling (needs
    /// non-1 `pre_attack_multiplier`, e.g. from [`TimingWindow::RoundStart`] [`AbilityEffect::AttackMultiplier`]).
    #[serde(default)]
    pub synthetic_crew: Option<DriftSyntheticCrew>,
}

/// Minimal crew wiring for drift JSON without full LCARS/officer payloads.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DriftSyntheticCrew {
    /// Delta passed to [`AbilityEffect::AttackMultiplier`] at [`TimingWindow::RoundStart`]
    /// (feeds `pre_attack_modifier_sum`; uses per-round `round_index` ≥ 1 for traces).
    #[serde(default)]
    pub round_start_attack_multiplier: f64,
    /// Apply [`AbilityEffect::HullBreach`] (chance 1.0, requires_critical: false) at CombatBegin
    /// for the given duration, targeting the defender. Tests Hull Breach → crit damage interaction.
    #[serde(default)]
    pub defender_hull_breach_rounds: Option<u32>,
    /// Apply [`AbilityEffect::Burning`] (chance 1.0) at CombatBegin for the given duration,
    /// targeting the defender. Tests Burning → round-end damage tick interaction.
    #[serde(default)]
    pub defender_burning_rounds: Option<u32>,
    /// Apply [`AbilityEffect::Morale`] at RoundStart with the given trigger chance.
    /// Morale enables the primary piercing bonus and gates [`AbilityCondition::MoraleActive`].
    #[serde(default)]
    pub morale_chance: Option<f64>,
    /// Apply [`AbilityEffect::OnKillHullRegen`] at Kill timing with the given fraction of max hull
    /// healed per kill. Engine applies `on_kill_regen * attacker.hull_health` as a heal.
    /// Note: single-combat fixtures fire at most one kill; multi-kill chains require chain-grind tests.
    #[serde(default)]
    pub on_kill_hull_regen: Option<f64>,
    /// Apply [`AbilityEffect::HullBreach`] (chance 1.0, requires_critical: false) via the
    /// **defender crew** at RoundStart, targeting the **attacker**. Exercises hostile-applied
    /// status effects (counter-fire HB on the player). Duration applies from the round the
    /// defender fires.
    #[serde(default)]
    pub attacker_hull_breach_rounds: Option<u32>,
    /// Apply [`AbilityEffect::Burning`] (chance 1.0) via the **defender crew** at RoundStart,
    /// targeting the **attacker**. Exercises hostile-applied Burning damage ticks on the player.
    #[serde(default)]
    pub attacker_burning_rounds: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureSimulation {
    pub rounds: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// When set, forwarded to [`SimulationConfig::weapon_damage_profile_additive_pool`] (research/profile `weapon_damage` pooled model).
    #[serde(default)]
    pub weapon_damage_profile_additive_pool: Option<f64>,
    /// Forwarded to [`SimulationConfig::profile_weapon_damage_fraction`] (dilutes galaxy growth; layered model baseline).
    #[serde(default)]
    pub profile_weapon_damage_fraction: f64,
    /// Forwarded to [`SimulationConfig::initial_attacker_hull_damage`].
    #[serde(default)]
    pub initial_attacker_hull_damage: f64,
    /// Bitmask for hostile tags on the defender (e.g. [`crate::combat::hostile_tags::HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR`]).
    /// Passed through to [`SimulationConfig::defender_hostile_tag_mask`]. Default 0.
    #[serde(default)]
    pub defender_hostile_tag_mask: u32,
}

fn default_seed() -> u64 {
    42
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FixtureCombatant {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub attack: f64,
    #[serde(default)]
    pub mitigation: f64,
    #[serde(default)]
    pub pierce: f64,
    #[serde(default)]
    pub crit_chance: f64,
    #[serde(default = "one")]
    pub crit_multiplier: f64,
    #[serde(default)]
    pub proc_chance: f64,
    #[serde(default = "one")]
    pub proc_multiplier: f64,
    #[serde(default)]
    pub end_of_round_damage: f64,
    #[serde(default = "thousand_hull")]
    pub hull_health: f64,
    #[serde(default)]
    pub shield_health: f64,
    #[serde(default = "shield_mit")]
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
    pub weapons: Vec<FixtureWeapon>,
}

fn one() -> f64 {
    1.0
}

fn thousand_hull() -> f64 {
    1000.0
}

fn shield_mit() -> f64 {
    0.8
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FixtureWeapon {
    #[serde(default)]
    pub attack: f64,
    pub shots: Option<u32>,
    pub pierce: Option<f64>,
    pub crit_chance: Option<f64>,
    pub crit_multiplier: Option<f64>,
    pub proc_chance: Option<f64>,
    pub proc_multiplier: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MetricBands {
    #[serde(default)]
    pub total_damage: Option<[f64; 2]>,
    #[serde(default)]
    pub rounds_simulated: Option<[f64; 2]>,
    #[serde(default)]
    pub defender_hull_remaining: Option<[f64; 2]>,
    #[serde(default)]
    pub defender_shield_remaining: Option<[f64; 2]>,
    #[serde(default)]
    pub attacker_hull_remaining: Option<[f64; 2]>,
}

fn crew_for_drift(
    synthetic: &Option<DriftSyntheticCrew>,
) -> (CrewConfiguration, CrewConfiguration) {
    let Some(c) = synthetic else {
        return (CrewConfiguration::default(), CrewConfiguration::default());
    };
    let mut attacker_seats: Vec<CrewSeatContext> = Vec::new();
    let mut defender_seats: Vec<CrewSeatContext> = Vec::new();

    if c.round_start_attack_multiplier.is_finite() && c.round_start_attack_multiplier != 0.0 {
        attacker_seats.push(CrewSeatContext::legacy(
            CrewSeat::Captain,
            Ability {
                name: "drift_synthetic_attack_mult".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(c.round_start_attack_multiplier),
                condition: None,
            },
            false,
        ));
    }

    if let Some(rounds) = c.defender_hull_breach_rounds {
        if rounds > 0 {
            attacker_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_hull_breach".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::HullBreach {
                        chance: 1.0,
                        duration_rounds: rounds,
                        requires_critical: false,
                    },
                    condition: None,
                },
                false,
            ));
        }
    }

    if let Some(rounds) = c.defender_burning_rounds {
        if rounds > 0 {
            attacker_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_burning".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::Burning {
                        chance: 1.0,
                        duration_rounds: rounds,
                    },
                    condition: None,
                },
                false,
            ));
        }
    }

    if let Some(chance) = c.morale_chance {
        if chance > 0.0 && chance.is_finite() {
            attacker_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_morale".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::RoundStart,
                    boostable: false,
                    effect: AbilityEffect::Morale(chance),
                    condition: None,
                },
                false,
            ));
        }
    }

    if let Some(regen) = c.on_kill_hull_regen {
        if regen.is_finite() && regen > 0.0 {
            attacker_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_on_kill_hull_regen".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::Kill,
                    boostable: false,
                    effect: AbilityEffect::OnKillHullRegen(regen),
                    condition: None,
                },
                false,
            ));
        }
    }

    // Attacker-targeted status effects: defender crew abilities that apply Burning/HullBreach
    // to the attacker during counter-fire. Wired as defender AttackPhase abilities so they
    // fire in the counter-fire processing path (defender shoots attacker). Chance 1.0 ensures
    // reliable application each round the defender hits.
    if let Some(rounds) = c.attacker_hull_breach_rounds {
        if rounds > 0 {
            defender_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_attacker_hull_breach".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::AttackPhase,
                    boostable: false,
                    effect: AbilityEffect::HullBreach {
                        chance: 1.0,
                        duration_rounds: rounds,
                        requires_critical: false,
                    },
                    condition: None,
                },
                false,
            ));
        }
    }

    if let Some(rounds) = c.attacker_burning_rounds {
        if rounds > 0 {
            defender_seats.push(CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "drift_synthetic_attacker_burning".to_string(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::AttackPhase,
                    boostable: false,
                    effect: AbilityEffect::Burning {
                        chance: 1.0,
                        duration_rounds: rounds,
                    },
                    condition: None,
                },
                false,
            ));
        }
    }

    let attacker_crew = if attacker_seats.is_empty() {
        CrewConfiguration::default()
    } else {
        CrewConfiguration {
            seats: attacker_seats,
        }
    };
    let defender_crew = if defender_seats.is_empty() {
        CrewConfiguration::default()
    } else {
        CrewConfiguration {
            seats: defender_seats,
        }
    };
    (attacker_crew, defender_crew)
}

fn simulation_config_for_drift(spec: &DriftFixtureFile, trace: TraceMode) -> SimulationConfig {
    SimulationConfig {
        rounds: spec.simulation.rounds,
        seed: spec.simulation.seed,
        trace_mode: trace,
        initial_attacker_hull_damage: spec.simulation.initial_attacker_hull_damage,
        weapon_damage_profile_additive_pool: spec.simulation.weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction: spec.simulation.profile_weapon_damage_fraction,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: spec.simulation.defender_hostile_tag_mask,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: crate::combat::EnemyTypes::default(),
        defender_level: None,
        attacker_roster_officer_ids: Vec::new(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        emit_state_snapshots: false,
        crit_damage_reduction_perturb: 0.0,
    }
}

impl FixtureCombatant {
    fn to_combatant(&self, default_id: &str) -> Combatant {
        let weapons: Vec<WeaponStats> = self
            .weapons
            .iter()
            .map(|w| WeaponStats {
                attack: w.attack,
                shots: w.shots,
                pierce: w.pierce,
                crit_chance: w.crit_chance,
                crit_multiplier: w.crit_multiplier,
                proc_chance: w.proc_chance,
                proc_multiplier: w.proc_multiplier,
            })
            .collect();
        Combatant {
            id: self.id.clone().unwrap_or_else(|| default_id.to_string()),
            attack: self.attack,
            mitigation: self.mitigation,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: self.pierce,
            crit_chance: self.crit_chance,
            crit_multiplier: self.crit_multiplier,
            proc_chance: self.proc_chance,
            proc_multiplier: self.proc_multiplier,
            end_of_round_damage: self.end_of_round_damage,
            hull_health: self.hull_health,
            shield_health: self.shield_health,
            shield_mitigation: self.shield_mitigation,
            apex_barrier: self.apex_barrier,
            apex_shred: self.apex_shred,
            isolytic_damage: self.isolytic_damage,
            isolytic_defense: self.isolytic_defense,
            weapons,
            hostile_mitigation_params: None,
        }
    }
}

/// One scalar metric compared to an inclusive `[low, high]` band.
#[derive(Debug, Clone)]
pub struct DriftMetricRow {
    pub fixture_id: String,
    pub metric: &'static str,
    pub actual: f64,
    pub low: f64,
    pub high: f64,
    /// Midpoint of the band.
    pub reference_mid: f64,
    /// Distance from midpoint in units of half-band-width (0 = at mid, 1 = on edge).
    pub sigma_from_mid: f64,
    pub in_band: bool,
}

#[derive(Debug, Clone)]
pub struct DriftRunReport {
    pub fixture_id: String,
    pub description: Option<String>,
    pub source: Option<String>,
    pub rows: Vec<DriftMetricRow>,
    pub attacker_won_ok: Option<bool>,
    pub all_numeric_ok: bool,
    pub all_ok: bool,
}

/// Load a drift fixture JSON from disk.
pub fn load_drift_fixture(path: &Path) -> Result<DriftFixtureFile, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Run the simulator for a loaded fixture (default empty crew unless `synthetic_crew` is set).
/// Uses the full engine entry point so that defender crew effects (hostile-applied status effects)
/// and hostile-tag-gated beam mechanics (Conqueror Borg) are exercised.
pub fn simulate_drift_fixture(spec: &DriftFixtureFile) -> crate::combat::SimulationResult {
    let attacker = spec.attacker.to_combatant("drift_attacker");
    let defender = spec.defender.to_combatant("drift_defender");
    let config = simulation_config_for_drift(spec, TraceMode::Off);
    let (attacker_crew, defender_crew) = crew_for_drift(&spec.synthetic_crew);
    let defender_is_npc_hostile = spec.simulation.defender_hostile_tag_mask != 0;
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        defender_is_npc_hostile,
        false,
        &defender_crew,
    )
}

/// Same as [`simulate_drift_fixture`] but records full combat trace events ([`TraceMode::Events`]).
pub fn simulate_drift_fixture_traced(spec: &DriftFixtureFile) -> crate::combat::SimulationResult {
    let attacker = spec.attacker.to_combatant("drift_attacker");
    let defender = spec.defender.to_combatant("drift_defender");
    let config = simulation_config_for_drift(spec, TraceMode::Events);
    let (attacker_crew, defender_crew) = crew_for_drift(&spec.synthetic_crew);
    let defender_is_npc_hostile = spec.simulation.defender_hostile_tag_mask != 0;
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &attacker_crew,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        defender_is_npc_hostile,
        false,
        &defender_crew,
    )
}

fn band_mid(low: f64, high: f64) -> f64 {
    0.5 * (low + high)
}

fn sigma_from_mid(actual: f64, low: f64, high: f64) -> f64 {
    let mid = band_mid(low, high);
    let half = ((high - low).abs() * 0.5).max(1e-12);
    (actual - mid).abs() / half
}

fn push_band(
    rows: &mut Vec<DriftMetricRow>,
    fixture_id: &str,
    metric: &'static str,
    actual: f64,
    band: [f64; 2],
) {
    let [low, high] = band;
    let lo = low.min(high);
    let hi = low.max(high);
    let reference_mid = band_mid(lo, hi);
    let in_band = actual >= lo && actual <= hi;
    rows.push(DriftMetricRow {
        fixture_id: fixture_id.to_string(),
        metric,
        actual,
        low: lo,
        high: hi,
        reference_mid,
        sigma_from_mid: sigma_from_mid(actual, lo, hi),
        in_band,
    });
}

/// Compare simulation output to fixture bands; build a drift report (numeric + optional win flag).
pub fn drift_report(
    spec: &DriftFixtureFile,
    result: &crate::combat::SimulationResult,
) -> DriftRunReport {
    let id = spec.id.clone();
    let mut rows = Vec::new();

    if let Some(b) = spec.bands.total_damage {
        push_band(&mut rows, &id, "total_damage", result.total_damage, b);
    }
    if let Some(b) = spec.bands.rounds_simulated {
        push_band(
            &mut rows,
            &id,
            "rounds_simulated",
            result.rounds_simulated as f64,
            b,
        );
    }
    if let Some(b) = spec.bands.defender_hull_remaining {
        push_band(
            &mut rows,
            &id,
            "defender_hull_remaining",
            result.defender_hull_remaining,
            b,
        );
    }
    if let Some(b) = spec.bands.defender_shield_remaining {
        push_band(
            &mut rows,
            &id,
            "defender_shield_remaining",
            result.defender_shield_remaining,
            b,
        );
    }
    if let Some(b) = spec.bands.attacker_hull_remaining {
        push_band(
            &mut rows,
            &id,
            "attacker_hull_remaining",
            result.attacker_hull_remaining,
            b,
        );
    }

    let all_numeric_ok = rows.iter().all(|r| r.in_band);
    let attacker_won_ok = spec
        .expect_attacker_won
        .map(|expected| result.attacker_won == expected);
    let all_ok = all_numeric_ok && attacker_won_ok.unwrap_or(true);

    DriftRunReport {
        fixture_id: id,
        description: spec.description.clone(),
        source: spec.source.clone(),
        rows,
        attacker_won_ok,
        all_numeric_ok,
        all_ok,
    }
}

/// Load, run, and report in one step.
pub fn run_drift_fixture_path(
    path: &Path,
) -> Result<(DriftRunReport, crate::combat::SimulationResult), String> {
    let spec = load_drift_fixture(path)?;
    let result = simulate_drift_fixture(&spec);
    let report = drift_report(&spec, &result);
    Ok((report, result))
}

/// Multi-fixture summary: which metrics are nearest/farthest from band midpoints (σ), and pass/fail counts.
pub fn format_drift_summary(reports: &[DriftRunReport]) -> String {
    let mut out = String::new();
    let mut pass = 0usize;
    let mut fail = 0usize;
    for r in reports {
        if r.all_ok {
            pass += 1;
        } else {
            fail += 1;
        }
        out.push_str(&format!("=== {} ===\n", r.fixture_id));
        if let Some(ref s) = r.source {
            out.push_str(&format!("  source: {s}\n"));
        }
        if let Some(ref d) = r.description {
            out.push_str(d);
            out.push('\n');
        }
        for row in &r.rows {
            let status = if row.in_band { "ok" } else { "OUT_OF_BAND" };
            out.push_str(&format!(
                "  {:<28} actual={:.4} band=[{:.4},{:.4}] mid={:.4} sigma={:.3} {}\n",
                row.metric,
                row.actual,
                row.low,
                row.high,
                row.reference_mid,
                row.sigma_from_mid,
                status
            ));
        }
        if let Some(ok) = r.attacker_won_ok {
            out.push_str(&format!(
                "  attacker_won check: {}\n",
                if ok { "ok" } else { "MISMATCH" }
            ));
        }
        out.push_str(&format!(
            "  fixture_ok: {}\n\n",
            if r.all_ok { "yes" } else { "no" }
        ));
    }
    out.push_str(&format!("fixtures_passed={pass} fixtures_failed={fail}\n"));

    // Farthest from mid (regression "moved" the most within or outside band)
    let mut flat: Vec<&DriftMetricRow> = reports.iter().flat_map(|r| r.rows.iter()).collect();
    flat.sort_by(|a, b| {
        b.sigma_from_mid
            .partial_cmp(&a.sigma_from_mid)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.push_str("largest |sigma_from_mid| (farthest from band center):\n");
    for row in flat.iter().take(12) {
        out.push_str(&format!(
            "  {} {} sigma={:.3} actual={:.4}\n",
            row.fixture_id, row.metric, row.sigma_from_mid, row.actual
        ));
    }

    out
}

/// List `drift_*.json` files in a directory (non-recursive).
pub fn list_drift_fixture_paths(dir: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with("drift_") && s.ends_with(".json"))
        })
        .collect();
    paths.sort();
    Ok(paths)
}
