//! Sensitivity analysis: rank in-game stats by their measured Δ on a fight-outcome metric.
//!
//! For a fixed scenario (crew + ship + hostile + research + support buffs), runs paired
//! baseline / perturbed Monte Carlo simulations using Common Random Numbers, then computes
//! per-stat mean differences with 95% confidence intervals (paired t-interval, large-N
//! normal approximation).
//!
//! See `docs/ROADMAP.md` (Stat modeling improvements) for engine limitations the current
//! v1 stat list works around (collapsed mitigation components, deferred crit damage floor,
//! deferred player_crit_damage_reduction).

use std::collections::HashMap;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::perturb::{apply_perturbation, StatKey};
use crate::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, OpponentFactionTag, SimulationConfig,
    SimulationResult as CombatSimResult, TraceMode,
};
use crate::data::data_registry::DataRegistry;
use crate::data::support_buffs;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, scenario_to_combat_input_from_shared,
    CombatSimulationInput, DefenderOpponent, SharedScenarioData,
};
use crate::server::sensitivity_jobs::SensitivityJobProgress;

/// User-chosen outcome scalar that each stat's Δ is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeMetric {
    /// Attacker hull remaining as a fraction of starting hull (0–1). Continuous, sensitive,
    /// well-defined whether the attacker wins or loses. **Default.** Recommended for PvE.
    #[default]
    HullRemaining,
    /// 1.0 on a win, 0.0 otherwise. Best for PvP where the outcome is binary.
    WinRate,
    /// Rounds until combat ended (lower = better when attacker is winning). Use to detect
    /// stats that change "how decisively" a fight ends.
    RoundsToKill,
    /// Defender hull remaining (0–1 fraction). Lower is better for attacker; sign is flipped
    /// internally so positive Δ always means "better for the attacker."
    DefenderHullRemaining,
}

impl OutcomeMetric {
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomeMetric::HullRemaining => "hull_remaining",
            OutcomeMetric::WinRate => "win_rate",
            OutcomeMetric::RoundsToKill => "rounds_to_kill",
            OutcomeMetric::DefenderHullRemaining => "defender_hull_remaining",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "hull_remaining" => Some(OutcomeMetric::HullRemaining),
            "win_rate" => Some(OutcomeMetric::WinRate),
            "rounds_to_kill" => Some(OutcomeMetric::RoundsToKill),
            "defender_hull_remaining" => Some(OutcomeMetric::DefenderHullRemaining),
            _ => None,
        }
    }

    /// Extract the metric value from a single simulation result. Positive values are always
    /// "better for the attacker" — for [`Self::RoundsToKill`] we negate, and for
    /// [`Self::DefenderHullRemaining`] we use `1 - frac` so a larger value means more damage
    /// dealt.
    pub(crate) fn extract(
        self,
        result: &CombatSimResult,
        attacker_max_hull: f64,
        defender_max_hull: f64,
    ) -> f64 {
        match self {
            OutcomeMetric::HullRemaining => {
                (result.attacker_hull_remaining / attacker_max_hull.max(1.0)).clamp(0.0, 1.0)
            }
            OutcomeMetric::WinRate => {
                if result.attacker_won {
                    1.0
                } else {
                    0.0
                }
            }
            OutcomeMetric::RoundsToKill => -(result.rounds_simulated as f64),
            OutcomeMetric::DefenderHullRemaining => {
                let frac =
                    (result.defender_hull_remaining / defender_max_hull.max(1.0)).clamp(0.0, 1.0);
                1.0 - frac
            }
        }
    }
}

/// Crew + scenario input to a sensitivity run. Mirrors the shape of `/api/simulate` so the
/// frontend can reuse its scenario pickers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityRequest {
    pub ship: String,
    pub hostile: String,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub captain: Option<String>,
    pub bridge: Vec<String>,
    #[serde(default)]
    pub below_decks: Vec<String>,
    #[serde(default)]
    pub support_buffs: Option<Vec<String>>,
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Paired sims per stat (also used for the baseline). Default 2000.
    #[serde(default)]
    pub num_sims: Option<u32>,
    /// Base RNG seed for `s0..s0+N`. Default 0.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Combat rounds per sim. Default 3 (matches engine default).
    #[serde(default)]
    pub rounds: Option<u32>,
    /// Outcome metric to measure against. Default [`OutcomeMetric::HullRemaining`].
    #[serde(default)]
    pub metric: Option<OutcomeMetric>,
    /// Per-stat δ overrides. Stats not listed use [`StatKey::default_delta`]. Stats whose
    /// override is `0.0` are skipped (no perturbation row produced).
    #[serde(default)]
    pub deltas: Option<HashMap<String, f64>>,
}

/// One row in the ranked sensitivity report.
#[derive(Debug, Clone, Serialize)]
pub struct SensitivityRow {
    /// Stat key (snake_case, matches [`StatKey::as_str`]).
    pub stat: String,
    /// Delta applied to that stat (the override value, or [`StatKey::default_delta`]).
    pub delta_applied: f64,
    /// Mean of `(perturbed - baseline)` per-seed metric values.
    pub mean_diff: f64,
    /// Mean relative to the baseline mean (`mean_diff / mean_baseline`), as a fraction.
    /// `None` when the baseline mean is zero or non-finite.
    pub mean_diff_relative: Option<f64>,
    /// Lower / upper bounds of a 95% paired t-interval on the diff distribution.
    pub ci95_low: f64,
    pub ci95_high: f64,
    /// `true` when the CI excludes zero — UI flags these as "measurable effect."
    pub significant: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SensitivityResponse {
    pub metric: &'static str,
    pub baseline_mean: f64,
    pub num_sims: u32,
    pub base_seed: u64,
    /// One row per stat, **unsorted** — clients sort by their preferred column.
    pub rows: Vec<SensitivityRow>,
}

/// Per-stat default deltas, exposed by `GET /api/sensitivity/defaults`.
pub fn default_deltas() -> Vec<(StatKey, f64)> {
    StatKey::ALL
        .iter()
        .map(|s| (*s, s.default_delta()))
        .collect()
}

/// Run a sensitivity analysis end-to-end. Builds shared scenario data once, runs baseline N
/// sims, then for each stat runs N paired sims with the perturbation applied. Returns a
/// ranked response.
pub fn run_sensitivity(
    registry: &DataRegistry,
    request: &SensitivityRequest,
) -> Result<SensitivityResponse, String> {
    run_sensitivity_with_progress(registry, request, &SensitivityJobProgress::no_op())
}

/// OAT sensitivity run that reports progress + checks cancellation through a sink. The
/// sync [`run_sensitivity`] entry above wraps this with a no-op sink.
pub fn run_sensitivity_with_progress(
    registry: &DataRegistry,
    request: &SensitivityRequest,
    progress: &SensitivityJobProgress,
) -> Result<SensitivityResponse, String> {
    let num_sims = request.num_sims.unwrap_or(2000).max(2);
    let base_seed = request.seed.unwrap_or(0);
    let metric = request.metric.unwrap_or_default();

    let shared = build_shared_scenario_data_from_registry(
        registry,
        &request.ship,
        &request.hostile,
        request.ship_tier,
        request.ship_level,
        request.profile_id.as_deref(),
            support_buffs::SupportBuffScenarioRequest::attacker_only(request.support_buffs.as_deref()),
        DefenderOpponent::default(),
        None,
        None,
    );

    let candidate = CrewCandidate {
        captain: request.captain.clone().unwrap_or_default(),
        bridge: request.bridge.clone(),
        below_decks: request.below_decks.clone(),
    };

    let input = scenario_to_combat_input_from_shared(&shared, &candidate, base_seed);
    if let Some(r) = request.rounds {
        // Overrides the per-sim rounds; SimulationConfig consumes it.
        // Note: input.rounds is owned by the input struct and re-read in run_one_sim.
        // We adjust via a local clone below.
        let mut input2 = input.clone();
        input2.rounds = r;
        return run_with_input(
            &shared, input2, metric, num_sims, base_seed, request, progress,
        );
    }
    run_with_input(
        &shared, input, metric, num_sims, base_seed, request, progress,
    )
}

fn run_with_input(
    shared: &SharedScenarioData,
    input: CombatSimulationInput,
    metric: OutcomeMetric,
    num_sims: u32,
    base_seed: u64,
    request: &SensitivityRequest,
    progress: &SensitivityJobProgress,
) -> Result<SensitivityResponse, String> {
    let attacker_max_hull = input.attacker.hull_health.max(1.0);
    let defender_max_hull = input.defender.hull_health.max(1.0);

    // Compute total sim budget up-front so the progress %% has a denominator. OAT runs
    // `num_sims` baseline sims plus `num_sims × k_effective` perturbation sims.
    let overrides = request.deltas.clone().unwrap_or_default();
    let stat_deltas: Vec<(StatKey, f64)> = StatKey::ALL
        .iter()
        .filter_map(|stat| {
            let delta = match overrides.get(stat.as_str()) {
                Some(v) if *v == 0.0 => return None,
                Some(v) => *v,
                None => stat.default_delta(),
            };
            Some((*stat, delta))
        })
        .collect();
    progress.set_total_sims((num_sims as u64) * (1 + stat_deltas.len() as u64));

    progress.set_phase("baseline");
    let baseline: Vec<f64> = (0..num_sims)
        .into_par_iter()
        .map(|i| {
            let seed = base_seed.wrapping_add(i as u64);
            let r = run_one_sim(shared, &input, seed, StatKey::WeaponDamage, 0.0);
            let v = metric.extract(&r, attacker_max_hull, defender_max_hull);
            progress.record_sims(1);
            v
        })
        .collect();
    if progress.cancelled() {
        return Err("Cancelled".to_string());
    }
    let baseline_mean = mean(&baseline);

    progress.set_phase("per_stat_perturbation");
    let rows: Vec<SensitivityRow> = stat_deltas
        .par_iter()
        .map(|(stat, delta)| {
            let diffs: Vec<f64> = (0..num_sims)
                .into_par_iter()
                .map(|i| {
                    let seed = base_seed.wrapping_add(i as u64);
                    let r = run_one_sim(shared, &input, seed, *stat, *delta);
                    let perturbed = metric.extract(&r, attacker_max_hull, defender_max_hull);
                    progress.record_sims(1);
                    perturbed - baseline[i as usize]
                })
                .collect();
            row_from_diffs(*stat, *delta, &diffs, baseline_mean)
        })
        .collect();
    if progress.cancelled() {
        return Err("Cancelled".to_string());
    }

    Ok(SensitivityResponse {
        metric: metric.as_str(),
        baseline_mean,
        num_sims,
        base_seed,
        rows,
    })
}

/// Single Monte Carlo trial. Clones the attacker / defender / config from the input, applies
/// the perturbation, then calls the engine. A `delta` of zero is a no-op and yields the
/// baseline.
fn run_one_sim(
    shared: &SharedScenarioData,
    input: &CombatSimulationInput,
    iteration_seed: u64,
    stat: StatKey,
    delta: f64,
) -> CombatSimResult {
    run_one_sim_with_perturbations(shared, input, iteration_seed, &[(stat, delta)])
}

/// Like [`run_one_sim`] but accepts a slice of `(stat, delta)` pairs that are applied in order
/// (cumulative perturbation, used by Morris trajectories). Empty slice = baseline.
pub(crate) fn run_one_sim_with_perturbations(
    shared: &SharedScenarioData,
    input: &CombatSimulationInput,
    iteration_seed: u64,
    perturbations: &[(StatKey, f64)],
) -> CombatSimResult {
    let mut attacker = input.attacker.clone();
    let mut defender = input.defender.clone();
    let config = SimulationConfig {
        rounds: input.rounds,
        seed: iteration_seed,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: input.weapon_damage_profile_additive_pool,
        profile_weapon_damage_fraction: input.profile_weapon_damage_fraction,
        defender_hull_faction_id: shared
            .hostile_rec
            .as_ref()
            .and_then(|h| h.faction.as_ref().map(|f| f.id))
            .unwrap_or(0),
        defender_hostile_tag_mask: shared.defender_hostile_tag_mask_for_combat(),
        attacker_owner_faction: input.attacker_owner_faction,
        engagement_enemy_types: input.engagement_enemy_types.clone(),
        defender_level: input.defender_level,
        attacker_roster_officer_ids: input.attacker_roster_officer_ids.clone(),
        incoming_shield_mitigation_bonus: input.incoming_shield_mitigation_bonus,
        incoming_shield_mitigation_bonus_rounds: input.incoming_shield_mitigation_bonus_rounds,
        emit_state_snapshots: false,
    };

    for (stat, delta) in perturbations {
        apply_perturbation(&mut attacker, &mut defender, *stat, *delta);
    }

    let defender_faction = shared
        .hostile_rec
        .as_ref()
        .map(|h| h.opponent_faction_tag())
        .unwrap_or(OpponentFactionTag::Unknown);

    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &input.crew,
        defender_faction,
        shared.defender_ship_type_for_combat(),
        shared.attacker_ship_type_for_combat(),
        shared.defender_opponent.defender_is_npc_hostile(),
        shared.defender_opponent.defender_is_player_ship(),
        &input.defender_crew,
    )
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn row_from_diffs(stat: StatKey, delta: f64, diffs: &[f64], baseline_mean: f64) -> SensitivityRow {
    let n = diffs.len() as f64;
    let mean_diff = if diffs.is_empty() { 0.0 } else { mean(diffs) };
    let variance = if diffs.len() < 2 {
        0.0
    } else {
        diffs.iter().map(|d| (d - mean_diff).powi(2)).sum::<f64>() / (n - 1.0)
    };
    let stderr = (variance / n.max(1.0)).sqrt();
    // Large-N normal approximation. For N < 30 this is mildly anti-conservative; the API
    // surfaces N so callers know.
    const Z_95: f64 = 1.959_963_984_540_054;
    let half = Z_95 * stderr;
    let lo = mean_diff - half;
    let hi = mean_diff + half;
    let significant = lo > 0.0 || hi < 0.0;
    let mean_diff_relative = if baseline_mean.is_finite() && baseline_mean.abs() > 1e-12 {
        Some(mean_diff / baseline_mean)
    } else {
        None
    };
    SensitivityRow {
        stat: stat.as_str().to_string(),
        delta_applied: delta,
        mean_diff,
        mean_diff_relative,
        ci95_low: lo,
        ci95_high: hi,
        significant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_diff_ci_excludes_zero_when_diff_is_large_and_consistent() {
        // Construct synthetic per-seed diffs that are large and consistent.
        let diffs = vec![0.1_f64; 100];
        let row = row_from_diffs(StatKey::WeaponDamage, 0.05, &diffs, 0.5);
        assert!(row.significant);
        assert!((row.mean_diff - 0.1).abs() < 1e-12);
        assert!(row.ci95_low > 0.0);
    }

    #[test]
    fn paired_diff_ci_includes_zero_when_diffs_are_noise() {
        // Mixed-sign noise centred near zero — CI should straddle 0.
        let diffs: Vec<f64> = (0..100)
            .map(|i| if i % 2 == 0 { 0.01 } else { -0.01 })
            .collect();
        let row = row_from_diffs(StatKey::CritChance, 0.01, &diffs, 0.5);
        assert!(!row.significant);
        assert!(row.ci95_low < 0.0);
        assert!(row.ci95_high > 0.0);
    }

    #[test]
    fn outcome_metric_roundtrip() {
        for m in [
            OutcomeMetric::HullRemaining,
            OutcomeMetric::WinRate,
            OutcomeMetric::RoundsToKill,
            OutcomeMetric::DefenderHullRemaining,
        ] {
            assert_eq!(OutcomeMetric::parse_str(m.as_str()), Some(m));
        }
    }

    #[test]
    fn default_deltas_contains_every_stat_key() {
        let defaults = default_deltas();
        assert_eq!(defaults.len(), StatKey::ALL.len());
    }
}
