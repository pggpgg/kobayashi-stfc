//! Multi-fidelity Hyperband / Successive Halving racing with Common Random Numbers (CRN).
//!
//! Instead of spending an equal flat scouting budget on every candidate, racing screens a wide
//! candidate pool through geometrically increasing trial rungs (e.g. `16 -> 48 -> 144`) using
//! incremental [`CandidateTrialAccumulator`] advancement under the shared scenario seed panel.
//!
//! Properties:
//! - **Zero wasted trials:** surviving crews advance only the delta `[prev_rung..next_rung)` and
//!   accumulate exact counts and Welford variance over `0..next_rung`.
//! - **Common Random Numbers (CRN):** every crew in a rung experiences the exact same scenario
//!   trial seeds, sharply lowering relative ranking variance between crews.
//! - **Captain-stratum protection:** early brackets protect the strongest crew per captain stratum
//!   (up to a configurable cap) so high-variance captains are not prematurely wiped out by small-N
//!   noise before reaching deeper brackets.
//! - **Deterministic:** independent of Rayon worker count and scheduling order.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::optimizer::chain::{chain_ranking_scalar, ChainGrindParams};
use crate::optimizer::constraints::normalize_officer_name;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::scenario::SharedScenarioData;
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, CandidateTrialAccumulator, SimulationResult,
};
use crate::optimizer::ranking::rank_results;
use crate::optimizer::OptimizeProgressTick;
use crate::parallel::{batch_ranges, monte_carlo_batch_count_for_candidates};

const PARTIAL_TOP_MIN_INTERVAL: Duration = Duration::from_millis(200);

/// Configuration for multi-fidelity Successive Halving / Hyperband racing.
#[derive(Debug, Clone)]
pub struct RacingConfig {
    /// Strictly increasing cumulative trial counts per bracket (e.g. `[16, 48, 144]`).
    pub brackets: Vec<usize>,
    /// Survival divisor between consecutive brackets (default `3`: keep ~1/3 per rung).
    pub halving_factor: usize,
    /// Minimum crews kept into deeper brackets (typically `2 * top_k`).
    pub min_survivors: usize,
    /// Protect the single best crew per captain stratum in early brackets (bounded by `max_protected_strata`).
    pub protect_captain_strata: bool,
    /// Upper cap on extra crews retained solely via captain-stratum protection per bracket.
    pub max_protected_strata: usize,
    /// Upper cap on extra crews retained via confidence-interval overlap near the bracket cutline.
    pub max_ci_overlap_extra: usize,
    /// Optional hard cap on total newly executed trials across all racing brackets.
    pub total_trial_budget: Option<u64>,
}

impl RacingConfig {
    pub fn for_workload(
        n_candidates: usize,
        scout_cap: usize,
        top_k: usize,
        total_trial_budget: Option<u64>,
    ) -> Self {
        let brackets = default_racing_brackets(scout_cap);
        let min_survivors = top_k.saturating_mul(2).max(16).min(n_candidates.max(1));
        Self {
            brackets,
            halving_factor: 3,
            min_survivors,
            protect_captain_strata: true,
            max_protected_strata: top_k.saturating_mul(2).clamp(16, 48),
            max_ci_overlap_extra: top_k.saturating_mul(2).clamp(8, 32),
            total_trial_budget,
        }
    }
}

/// Build a geometric progression of cumulative trial rungs ending at `scout_cap`.
///
/// Examples:
/// - `scout_cap = 20`  -> `[20]`
/// - `scout_cap = 80`  -> `[16, 40, 80]`
/// - `scout_cap = 144` -> `[16, 48, 144]`
/// - `scout_cap = 500` -> `[24, 72, 216, 500]`
pub fn default_racing_brackets(scout_cap: usize) -> Vec<usize> {
    let cap = scout_cap.max(1);
    if cap <= 24 {
        return vec![cap];
    }
    if cap <= 96 {
        let r0 = (cap / 5).clamp(12, 20);
        let r1 = (cap / 2).max(r0 + 8);
        if r1 < cap {
            return vec![r0, r1, cap];
        }
        return vec![r0, cap];
    }
    let r0: usize = if cap >= 300 { 24 } else { 16 };
    let mut rungs = vec![r0];
    let mut cur: usize = r0;
    while cur.saturating_mul(3) < cap {
        let next = cur.saturating_mul(3);
        // Avoid leaving a tiny final step (< 1.4x) by jumping straight to `cap`.
        if next.saturating_mul(7) / 5 >= cap {
            break;
        }
        rungs.push(next);
        cur = next;
    }
    if *rungs.last().unwrap_or(&0) < cap {
        rungs.push(cap);
    }
    rungs
}

/// Telemetry emitted for each completed racing bracket.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, schemars::JsonSchema)]
pub struct RacingBracketTelemetry {
    pub bracket_index: usize,
    pub target_trials: usize,
    pub crews_evaluated: usize,
    pub trials_executed: u64,
    pub survivors_kept: usize,
    pub strata_protected: usize,
    pub ci_overlap_kept: usize,
}

/// Full output of [`run_hyperband_racing_with_shared`].
#[derive(Debug, Clone)]
pub struct RacingOutcome {
    /// Deepest evaluation for every input candidate (plus any preconfirmed rows).
    pub all_results: Vec<SimulationResult>,
    /// Indices into `all_results` (sorted best-first) of crews surviving the final bracket.
    pub finalist_hashes: Vec<u64>,
    /// Exact newly executed Monte Carlo trials across all racing brackets.
    pub executed_trials: u64,
    /// Per-bracket telemetry in execution order.
    pub brackets: Vec<RacingBracketTelemetry>,
    /// True when cooperative cancellation ended racing early.
    pub cancelled: bool,
}

/// Ranking proxy aligned with [`rank_results`] for bracket sorting.
#[inline]
fn racing_point_score(r: &SimulationResult) -> f64 {
    if let Some(ref c) = r.chain {
        chain_ranking_scalar(
            r.win_rate,
            r.avg_hull_remaining,
            c.secondary_objective,
            c.secondary_mean_given_primary,
        )
    } else {
        r.win_rate * 0.8 + r.avg_hull_remaining * 0.2
    }
}

/// Pessimistic lower bound on the ranking proxy using 95% intervals.
#[inline]
fn racing_lower_bound(r: &SimulationResult) -> f64 {
    if let Some(ref c) = r.chain {
        chain_ranking_scalar(
            r.win_rate_ci_low,
            r.avg_hull_remaining_ci_low,
            c.secondary_objective,
            c.secondary_ci_low,
        )
    } else {
        r.win_rate_ci_low * 0.8 + r.avg_hull_remaining_ci_low * 0.2
    }
}

/// Optimistic upper bound on the ranking proxy using 95% intervals.
#[inline]
fn racing_upper_bound(r: &SimulationResult) -> f64 {
    if let Some(ref c) = r.chain {
        chain_ranking_scalar(
            r.win_rate_ci_high,
            r.avg_hull_remaining_ci_high,
            c.secondary_objective,
            c.secondary_ci_high,
        )
    } else {
        r.win_rate_ci_high * 0.8 + r.avg_hull_remaining_ci_high * 0.2
    }
}

/// Run multi-fidelity Hyperband / Successive Halving racing over `candidates`.
///
/// - `captain_stratum_by_name`: maps normalized captain name to its stratum/group key for early
///   stratum protection (missing entries fall back to the normalized captain name itself).
/// - `preconfirmed`: optional cached full results; preconfirmed crews skip racing simulation
///   (charging 0 executed trials) and participate in ranking/finalist selection.
/// - `on_progress`: cooperative progress & cancellation callback; returning `false` cancels.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_hyperband_racing_with_shared<F>(
    shared: &SharedScenarioData,
    candidates: &[CrewCandidate],
    seed: u64,
    chain_grind: Option<&ChainGrindParams>,
    config: &RacingConfig,
    captain_stratum_by_name: &HashMap<String, String>,
    preconfirmed: Option<&HashMap<u64, SimulationResult>>,
    mut on_progress: F,
) -> RacingOutcome
where
    F: FnMut(OptimizeProgressTick) -> bool,
{
    let n = candidates.len();
    if n == 0 {
        return RacingOutcome {
            all_results: Vec::new(),
            finalist_hashes: Vec::new(),
            executed_trials: 0,
            brackets: Vec::new(),
            cancelled: false,
        };
    }

    let brackets = if config.brackets.is_empty() {
        vec![16]
    } else {
        config.brackets.clone()
    };

    let hashes: Vec<u64> = candidates.iter().map(crew_candidate_stable_hash).collect();
    let mut accumulators: Vec<CandidateTrialAccumulator> = candidates
        .iter()
        .cloned()
        .map(CandidateTrialAccumulator::new)
        .collect();
    let cached_results: Vec<Option<SimulationResult>> = hashes
        .iter()
        .map(|h| preconfirmed.and_then(|m| m.get(h)).cloned())
        .collect();

    // Active indices into `candidates` / `accumulators`.
    let mut active: Vec<usize> = (0..n).collect();
    let mut total_executed_trials = 0u64;
    let mut bracket_telemetry = Vec::with_capacity(brackets.len());
    let mut cancelled = false;
    let mut last_partial_emit: Option<Instant> = None;

    for (bracket_index, &rung_target) in brackets.iter().enumerate() {
        if active.is_empty() {
            break;
        }
        let target_trials = rung_target.max(1);

        // Identify which active indices actually need new trials in this rung.
        let mut to_run: Vec<usize> = active
            .iter()
            .copied()
            .filter(|&idx| {
                cached_results[idx].is_none() && accumulators[idx].trials_done < target_trials
            })
            .collect();

        // Enforce optional hard total trial budget by capping how many crews advance this rung
        // (preserving higher-ranked active order from the previous bracket) or trimming target.
        let mut effective_target = target_trials;
        if let Some(budget) = config.total_trial_budget {
            let remaining = budget.saturating_sub(total_executed_trials);
            if remaining == 0 {
                break;
            }
            if !to_run.is_empty() {
                let needed_full: u64 = to_run
                    .iter()
                    .map(|&idx| (target_trials - accumulators[idx].trials_done) as u64)
                    .sum();
                if needed_full > remaining {
                    if bracket_index == 0 {
                        // First bracket: try to evaluate all candidates at a slightly shallower rung
                        // if >= 8 trials/crew fits; otherwise evaluate as many as fit at 8+ trials.
                        let per_crew = (remaining / to_run.len() as u64) as usize;
                        if per_crew >= 8 {
                            effective_target = per_crew.min(target_trials);
                        } else {
                            let min_r = target_trials.clamp(1, 8);
                            effective_target = min_r;
                            let max_crews = (remaining / min_r as u64).max(1) as usize;
                            to_run.truncate(max_crews);
                            // Also restrict `active` for subsequent brackets to evaluated + cached.
                            let run_set: HashSet<usize> = to_run.iter().copied().collect();
                            active.retain(|idx| {
                                cached_results[*idx].is_some() || run_set.contains(idx)
                            });
                        }
                    } else {
                        // Later bracket: keep the top prefix that fits inside remaining budget.
                        let mut acc_cost = 0u64;
                        let mut fit_count = 0usize;
                        for &idx in &to_run {
                            let delta = (target_trials - accumulators[idx].trials_done) as u64;
                            if acc_cost + delta <= remaining {
                                acc_cost += delta;
                                fit_count += 1;
                            } else {
                                break;
                            }
                        }
                        if fit_count == 0 {
                            // Spend remaining budget smoothly across the leading survivors.
                            let lead_count = to_run.len().min(config.min_survivors.max(1));
                            let min_done = to_run[..lead_count]
                                .iter()
                                .map(|&i| accumulators[i].trials_done)
                                .max()
                                .unwrap_or(0);
                            let extra_each = (remaining / lead_count as u64) as usize;
                            if extra_each == 0 {
                                break;
                            }
                            effective_target = min_done + extra_each;
                            to_run.truncate(lead_count);
                        } else {
                            to_run.truncate(fit_count);
                        }
                    }
                }
            }
        }

        let num_batches = monte_carlo_batch_count_for_candidates(to_run.len().max(1)).min(16);
        let ranges = batch_ranges(to_run.len(), num_batches);
        let mut bracket_trials = 0u64;
        let mut evaluated_in_bracket = 0usize;
        let phase_label: &'static str = if bracket_index == 0 {
            "portfolio_race_coarse"
        } else {
            "portfolio_race_refine"
        };

        for (b_idx, &(start, end)) in ranges.iter().enumerate() {
            if start >= end {
                continue;
            }
            let slice_indices = &to_run[start..end];
            // Extract owned accumulators for disjoint parallel advancement, then put back.
            let mut batch_accs: Vec<(usize, CandidateTrialAccumulator)> = slice_indices
                .iter()
                .map(|&idx| (idx, accumulators[idx].clone()))
                .collect();

            let batch_trials: u64 = batch_accs
                .par_iter_mut()
                .map(|(_, acc)| acc.advance_to(shared, seed, effective_target, chain_grind) as u64)
                .sum();

            for (idx, acc) in batch_accs {
                accumulators[idx] = acc;
            }
            bracket_trials += batch_trials;
            total_executed_trials += batch_trials;
            evaluated_in_bracket += end - start;

            let is_last_batch = b_idx + 1 == ranges.len();
            let emit_partial = is_last_batch
                || last_partial_emit.is_none_or(|t| t.elapsed() >= PARTIAL_TOP_MIN_INTERVAL);
            let partial_top = if emit_partial {
                last_partial_emit = Some(Instant::now());
                let snap: Vec<SimulationResult> = active
                    .iter()
                    .filter_map(|&idx| {
                        if let Some(ref c) = cached_results[idx] {
                            Some(c.clone())
                        } else if accumulators[idx].trials_done > 0 {
                            Some(accumulators[idx].to_simulation_result(chain_grind))
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut ranked = rank_results(snap);
                ranked.truncate(5);
                Some(ranked)
            } else {
                None
            };

            if !on_progress(OptimizeProgressTick {
                crews_done: evaluated_in_bracket as u32,
                total_crews: to_run.len() as u32,
                phase: phase_label,
                partial_top,
            }) {
                cancelled = true;
                break;
            }
        }

        if cancelled {
            bracket_telemetry.push(RacingBracketTelemetry {
                bracket_index,
                target_trials: effective_target,
                crews_evaluated: evaluated_in_bracket,
                trials_executed: bracket_trials,
                survivors_kept: active.len(),
                strata_protected: 0,
                ci_overlap_kept: 0,
            });
            break;
        }

        // Snapshot current active results and sort deterministically best-first.
        let mut scored_active: Vec<(usize, SimulationResult, f64, f64, f64, u64)> = active
            .iter()
            .map(|&idx| {
                let res = cached_results[idx]
                    .clone()
                    .unwrap_or_else(|| accumulators[idx].to_simulation_result(chain_grind));
                let pt = racing_point_score(&res);
                let lo = racing_lower_bound(&res);
                let hi = racing_upper_bound(&res);
                (idx, res, pt, lo, hi, hashes[idx])
            })
            .collect();
        scored_active.sort_by(|a, b| {
            let a_sec =
                a.1.chain
                    .as_ref()
                    .map_or(a.1.avg_hull_remaining, |c| c.secondary_mean_given_primary);
            let b_sec =
                b.1.chain
                    .as_ref()
                    .map_or(b.1.avg_hull_remaining, |c| c.secondary_mean_given_primary);
            b.2.total_cmp(&a.2)
                .then_with(|| b.1.win_rate.total_cmp(&a.1.win_rate))
                .then_with(|| b_sec.total_cmp(&a_sec))
                .then_with(|| b.1.avg_hull_remaining.total_cmp(&a.1.avg_hull_remaining))
                .then_with(|| a.5.cmp(&b.5))
        });

        let is_final_bracket = bracket_index + 1 == brackets.len();
        if is_final_bracket {
            active = scored_active.into_iter().map(|(idx, ..)| idx).collect();
            bracket_telemetry.push(RacingBracketTelemetry {
                bracket_index,
                target_trials: effective_target,
                crews_evaluated: evaluated_in_bracket,
                trials_executed: bracket_trials,
                survivors_kept: active.len(),
                strata_protected: 0,
                ci_overlap_kept: 0,
            });
            break;
        }

        // Select survivors for the next bracket:
        // 1. Top `keep_target` globally by racing score.
        // 2. Confidence-interval overlap contenders whose upper bound clears the cutline's lower bound.
        // 3. Captain-stratum protection: best crew per distinct captain stratum (early brackets only).
        let divisor = config.halving_factor.max(2);
        let keep_target = (scored_active.len() / divisor)
            .max(config.min_survivors)
            .min(scored_active.len());

        let mut kept_indices: Vec<usize> = Vec::with_capacity(keep_target + 32);
        let mut kept_set: HashSet<usize> = HashSet::with_capacity(keep_target + 32);
        let mut covered_strata: HashSet<String> = HashSet::new();

        let stratum_of = |idx: usize| -> String {
            let norm = normalize_officer_name(&candidates[idx].captain);
            captain_stratum_by_name.get(&norm).cloned().unwrap_or(norm)
        };

        for row in scored_active.iter().take(keep_target) {
            kept_indices.push(row.0);
            kept_set.insert(row.0);
            covered_strata.insert(stratum_of(row.0));
        }

        // Confidence-interval overlap retention near the cutline (bounded).
        let cut_lower = scored_active
            .get(keep_target.saturating_sub(1))
            .map(|r| r.3)
            .unwrap_or(0.0);
        let mut ci_overlap_kept = 0usize;
        if config.max_ci_overlap_extra > 0 {
            for row in scored_active.iter().skip(keep_target) {
                if ci_overlap_kept >= config.max_ci_overlap_extra {
                    break;
                }
                if row.4 >= cut_lower && kept_set.insert(row.0) {
                    kept_indices.push(row.0);
                    covered_strata.insert(stratum_of(row.0));
                    ci_overlap_kept += 1;
                }
            }
        }

        // Captain-stratum protection (only on early brackets so the final bracket concentrates
        // depth on top contenders + diverse archive candidates).
        let mut strata_protected = 0usize;
        if config.protect_captain_strata && bracket_index == 0 && config.max_protected_strata > 0 {
            for row in scored_active.iter().skip(keep_target) {
                if strata_protected >= config.max_protected_strata {
                    break;
                }
                let st = stratum_of(row.0);
                if covered_strata.insert(st) && kept_set.insert(row.0) {
                    kept_indices.push(row.0);
                    strata_protected += 1;
                }
            }
        }

        bracket_telemetry.push(RacingBracketTelemetry {
            bracket_index,
            target_trials: effective_target,
            crews_evaluated: evaluated_in_bracket,
            trials_executed: bracket_trials,
            survivors_kept: kept_indices.len(),
            strata_protected,
            ci_overlap_kept,
        });
        active = kept_indices;
    }

    let all_results: Vec<SimulationResult> = (0..n)
        .filter_map(|idx| {
            if let Some(ref c) = cached_results[idx] {
                Some(c.clone())
            } else if accumulators[idx].trials_done > 0 {
                Some(accumulators[idx].to_simulation_result(chain_grind))
            } else {
                None
            }
        })
        .collect();

    let finalist_hashes: Vec<u64> = active.into_iter().map(|idx| hashes[idx]).collect();

    RacingOutcome {
        all_results,
        finalist_hashes,
        executed_trials: total_executed_trials,
        brackets: bracket_telemetry,
        cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data_registry::DataRegistry;
    use crate::data::profile_index::DEMO_PROFILE_ID;
    use crate::data::support_buffs::SupportBuffScenarioRequest;
    use crate::optimizer::monte_carlo::run_monte_carlo_scout_phase_with_shared;
    use crate::optimizer::monte_carlo::scenario::{
        build_shared_scenario_data_from_registry, DefenderOpponent,
    };

    #[test]
    fn default_racing_brackets_are_strictly_increasing_and_hit_cap() {
        assert_eq!(default_racing_brackets(16), vec![16]);
        assert_eq!(default_racing_brackets(80), vec![16, 40, 80]);
        assert_eq!(default_racing_brackets(144), vec![16, 48, 144]);
        let b500 = default_racing_brackets(500);
        assert_eq!(*b500.last().unwrap(), 500);
        assert!(b500.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn incremental_accumulator_matches_single_contiguous_scout_bit_for_bit() {
        let registry = DataRegistry::load().expect("registry loads");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "uss_saladin",
            "2918121098",
            None,
            None,
            Some(DEMO_PROFILE_ID),
            SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let crew = CrewCandidate {
            captain: "James T. Kirk".to_string(),
            bridge: vec!["Spock".to_string(), "Bones".to_string()],
            below_decks: vec![
                "Uhura".to_string(),
                "Sulu".to_string(),
                "Scotty".to_string(),
            ],
        };
        let mut acc = CandidateTrialAccumulator::new(crew.clone());
        let d1 = acc.advance_to(&shared, 42, 16, None);
        let d2 = acc.advance_to(&shared, 42, 48, None);
        assert_eq!(d1, 16);
        assert_eq!(d2, 32);
        let inc = acc.to_simulation_result(None);

        let direct =
            run_monte_carlo_scout_phase_with_shared(shared.clone(), &[crew], 48, 42, false, None);
        assert_eq!(direct.len(), 1);
        let d = &direct[0];
        assert_eq!(inc.trials_run, d.trials_run);
        assert_eq!(inc.win_rate.to_bits(), d.win_rate.to_bits());
        assert_eq!(inc.stall_rate.to_bits(), d.stall_rate.to_bits());
        assert_eq!(inc.loss_rate.to_bits(), d.loss_rate.to_bits());
        assert_eq!(inc.r1_kill_rate.to_bits(), d.r1_kill_rate.to_bits());
        assert_eq!(
            inc.avg_hull_remaining.to_bits(),
            d.avg_hull_remaining.to_bits()
        );
        assert_eq!(
            inc.avg_hull_remaining_ci_low.to_bits(),
            d.avg_hull_remaining_ci_low.to_bits()
        );
        assert_eq!(
            inc.avg_defender_hull_remaining.to_bits(),
            d.avg_defender_hull_remaining.to_bits()
        );
    }

    #[test]
    fn hyperband_racing_respects_budget_and_is_deterministic() {
        let registry = DataRegistry::load().expect("registry loads");
        let shared = build_shared_scenario_data_from_registry(
            &registry,
            "uss_saladin",
            "2918121098",
            None,
            None,
            Some(DEMO_PROFILE_ID),
            SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
        );
        let crews = vec![
            CrewCandidate {
                captain: "James T. Kirk".to_string(),
                bridge: vec!["Spock".to_string(), "Bones".to_string()],
                below_decks: vec!["Uhura".to_string(), "Sulu".to_string()],
            },
            CrewCandidate {
                captain: "Spock".to_string(),
                bridge: vec!["James T. Kirk".to_string(), "Bones".to_string()],
                below_decks: vec!["Uhura".to_string(), "Sulu".to_string()],
            },
            CrewCandidate {
                captain: "Bones".to_string(),
                bridge: vec!["James T. Kirk".to_string(), "Spock".to_string()],
                below_decks: vec!["Uhura".to_string(), "Sulu".to_string()],
            },
            CrewCandidate {
                captain: "Uhura".to_string(),
                bridge: vec!["James T. Kirk".to_string(), "Spock".to_string()],
                below_decks: vec!["Bones".to_string(), "Sulu".to_string()],
            },
        ];
        let cfg = RacingConfig {
            brackets: vec![12, 36],
            halving_factor: 2,
            min_survivors: 2,
            protect_captain_strata: false,
            max_protected_strata: 0,
            max_ci_overlap_extra: 0,
            total_trial_budget: Some(120),
        };
        let strata = HashMap::new();
        let r1 =
            run_hyperband_racing_with_shared(&shared, &crews, 7, None, &cfg, &strata, None, |_| {
                true
            });
        let r2 =
            run_hyperband_racing_with_shared(&shared, &crews, 7, None, &cfg, &strata, None, |_| {
                true
            });
        assert!(r1.executed_trials <= 120);
        assert_eq!(r1.executed_trials, r2.executed_trials);
        assert_eq!(r1.finalist_hashes, r2.finalist_hashes);
        assert_eq!(r1.all_results.len(), 4);
    }
}
