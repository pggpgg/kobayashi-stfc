//! Counterfactual ablations and substitute crew planner (roadmap §3.4 & §4.2).
//!
//! Answers two causal questions for any candidate crew using **Common Random Numbers (CRN)**:
//!
//! 1. **Why does this crew work?** — Leave-one-out ablation removes each officer individually on
//!    the exact same scenario seed panel to measure marginal score/win/hull impact with paired
//!    95% confidence intervals, paired with static-folded LCARS trigger-dependency attribution
//!    (e.g. Burning/Morale/Hull Breach producer→consumer links, Pike bridge amplification, and
//!    captain synergy partnerships).
//! 2. **What if I don't have Officer X (or need them on another ship)?** — Finds and CRN-ranks
//!    the best legal single-slot drop-in replacements for every seat and, when one or more
//!    officers are marked unavailable, synthesizes and CRN-confirms legal multi-seat substitute
//!    crews with exact paired performance loss and 95% confidence intervals.
//!
//! Simulation remains the sole judge: static-folded beam scoring only nominates top legal
//! replacement pools per seat; every reported delta, replacement, and substitute crew is measured
//! directly by deterministic Monte Carlo combat simulation.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::combat::{CrewSeat, EnemyType};
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::data::support_buffs::SupportBuffScenarioRequest;
use crate::mechanics::crew_fidelity::{CrewFidelityEvaluator, CrewFidelitySummary};
use crate::optimizer::beam::{
    rank_replacement_names_for_seat, state_bit_labels, summarize_officer_role, OfficerRoleSummary,
};
use crate::optimizer::chain::{ChainGrindParams, ChainSimulationSummary};
use crate::optimizer::constraints::normalize_officer_name;
use crate::optimizer::crew_generator::{
    build_officer_pools_from_registry, CrewCandidate, OfficerPools, BRIDGE_SLOTS,
};
use crate::optimizer::enforce_candidate_legality_with_registry;
use crate::optimizer::monte_carlo::scenario::{
    build_shared_scenario_data_from_registry, DefenderOpponent, PlayerDefenderOfficerCrewOverride,
    PvpScenarioParams, SharedScenarioData,
};
use crate::optimizer::monte_carlo::{
    crew_candidate_stable_hash, evaluate_candidates_crn_with_trial_scores, SimulationResult,
};

/// Max legal candidates nominated per seat for CRN simulation before taking top-K.
const PER_SEAT_NOMINATION_CAP: usize = 10;
/// Max multi-seat substitute candidates evaluated in one CRN batch.
const MAX_MULTI_SEAT_CANDIDATES: usize = 32;

/// Summary metrics for one evaluated crew variant.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CounterfactualMetrics {
    /// Ranking-aligned scalar score (`0.8·win + 0.2·avg_hull_remaining`, or chain equivalent).
    pub score: f64,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub loss_rate: f64,
    pub r1_kill_rate: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub avg_defender_hull_remaining: f64,
    pub trials_run: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainSimulationSummary>,
    pub fidelity: CrewFidelitySummary,
}

/// Paired Common Random Numbers (CRN) delta (`variant - baseline`) with 95% normal-approx CI
/// computed directly from per-trial differences `d_t = score_variant(t) - score_baseline(t)`.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PairedDelta {
    pub delta_score: f64,
    pub delta_score_ci_low: f64,
    pub delta_score_ci_high: f64,
    pub delta_win_rate: f64,
    pub delta_avg_hull_remaining: f64,
    pub delta_r1_kill_rate: f64,
    /// True when the 95% paired CI on `delta_score` excludes 0.
    pub statistically_significant: bool,
}

/// One legal single-slot replacement candidate measured on the same CRN seed panel.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SeatReplacementOption {
    pub officer: String,
    pub metrics: CounterfactualMetrics,
    pub delta: PairedDelta,
    /// Trigger states (e.g. `"Morale"`, `"Burning"`) consumed by remaining crewmates that this
    /// replacement continues to supply.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preserves_triggers: Vec<String>,
    /// True when this officer shares captain synergy with the seated captain / bridge.
    pub synergy_match: bool,
}

/// Causal ablation and drop-in replacement analysis for one occupied crew seat.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SeatAblation {
    /// `"captain"`, `"bridge"`, or `"below_decks"`.
    pub slot_group: String,
    /// Zero-based index inside `bridge` or `below_decks`; `None` for captain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_index: Option<usize>,
    pub officer: String,
    /// True when the caller marked this officer unavailable.
    pub unavailable: bool,
    /// Measured evaluation when this seat is left empty.
    pub leave_one_out: CounterfactualMetrics,
    /// Paired CRN delta of dropping this officer (`leave_one_out - baseline`; negative = loss).
    pub drop_delta: PairedDelta,
    /// Positive marginal contribution (`baseline.score - leave_one_out.score`).
    pub marginal_contribution: f64,
    /// Static-folded LCARS trigger/synergy relationships involving this seat.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trigger_dependencies: Vec<String>,
    /// Top legal 1-slot replacements ranked by measured CRN score.
    pub best_replacements: Vec<SeatReplacementOption>,
}

/// One slot change between the baseline crew and a substitute crew plan.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SubstituteSlotChange {
    pub slot_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_index: Option<usize>,
    pub from: String,
    pub to: String,
}

/// A complete legal substitute crew measured on the same CRN seed panel as the baseline.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SubstituteCrewPlan {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub changed_slots: Vec<SubstituteSlotChange>,
    pub metrics: CounterfactualMetrics,
    pub delta: PairedDelta,
    /// `substitute.score / baseline.score * 100.0` (100.0 when baseline score is ~0).
    pub retention_pct: f64,
    /// Concise human-readable summary of the performance trade-off.
    pub summary: String,
}

/// Parameters controlling a counterfactual ablation & substitute-planning pass.
#[derive(Clone)]
pub struct CounterfactualScenario<'a> {
    pub ship: &'a str,
    pub hostile: &'a str,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub below_decks_slots: usize,
    pub below_decks_pool_mode: BelowDecksPoolMode,
    pub profile_id: Option<&'a str>,
    pub support_buffs: SupportBuffScenarioRequest<'a>,
    pub chain_grind: Option<ChainGrindParams>,
    pub defender_opponent: DefenderOpponent,
    pub player_defender_officer_crew: Option<PlayerDefenderOfficerCrewOverride>,
    pub pvp: Option<PvpScenarioParams>,
    pub enemy_type: EnemyType,
    pub num_sims: usize,
    pub seed: u64,
    pub unavailable_officers: Vec<String>,
    pub max_substitutes_per_slot: usize,
    pub max_substitute_crews: usize,
}

/// Full output of a counterfactual ablation & substitute crew planning run.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CounterfactualOutcome {
    pub seed: u64,
    pub trials_per_variant: usize,
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub baseline: CounterfactualMetrics,
    pub seat_ablations: Vec<SeatAblation>,
    pub unavailable_officers: Vec<String>,
    pub substitutes: Vec<SubstituteCrewPlan>,
    pub variants_evaluated: usize,
    pub using_placeholder_combatants: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeatRef {
    Captain,
    Bridge(usize),
    BelowDecks(usize),
}

impl SeatRef {
    fn group_label(self) -> &'static str {
        match self {
            SeatRef::Captain => "captain",
            SeatRef::Bridge(_) => "bridge",
            SeatRef::BelowDecks(_) => "below_decks",
        }
    }

    fn index(self) -> Option<usize> {
        match self {
            SeatRef::Captain => None,
            SeatRef::Bridge(i) | SeatRef::BelowDecks(i) => Some(i),
        }
    }

    fn crew_seat(self) -> CrewSeat {
        match self {
            SeatRef::Captain => CrewSeat::Captain,
            SeatRef::Bridge(_) => CrewSeat::Bridge,
            SeatRef::BelowDecks(_) => CrewSeat::BelowDeck,
        }
    }
}

/// Scalar ranking score matching `rank_results` / `CandidateTrialAccumulator`.
#[inline]
pub(crate) fn scalar_score_from_sim(sim: &SimulationResult) -> f64 {
    if let Some(ref c) = sim.chain {
        let hull = c
            .ending_hull_mean_given_primary
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        0.8 * c.primary_success_rate + 0.2 * hull
    } else {
        0.8 * sim.win_rate + 0.2 * sim.avg_hull_remaining
    }
}

/// Paired 95% confidence interval on `alt - base` from Common Random Numbers per-trial scores.
pub(crate) fn paired_delta(
    base_sim: &SimulationResult,
    base_scores: &[f64],
    alt_sim: &SimulationResult,
    alt_scores: &[f64],
) -> PairedDelta {
    let n = base_scores.len().min(alt_scores.len());
    let (delta_score, ci_low, ci_high) = if n == 0 {
        let d = scalar_score_from_sim(alt_sim) - scalar_score_from_sim(base_sim);
        (d, d, d)
    } else if n == 1 {
        let d = alt_scores[0] - base_scores[0];
        (d, d, d)
    } else {
        let nf = n as f64;
        let mut mean = 0.0;
        let mut m2 = 0.0;
        for idx in 0..n {
            let d = alt_scores[idx] - base_scores[idx];
            let k = (idx + 1) as f64;
            let diff = d - mean;
            mean += diff / k;
            let diff2 = d - mean;
            m2 += diff * diff2;
        }
        let var = (m2 / (nf - 1.0)).max(0.0);
        let se = (var / nf).sqrt();
        const Z: f64 = 1.96;
        (mean, mean - Z * se, mean + Z * se)
    };
    let significant = ci_low > 1e-9 || ci_high < -1e-9;
    PairedDelta {
        delta_score,
        delta_score_ci_low: ci_low,
        delta_score_ci_high: ci_high,
        delta_win_rate: alt_sim.win_rate - base_sim.win_rate,
        delta_avg_hull_remaining: alt_sim.avg_hull_remaining - base_sim.avg_hull_remaining,
        delta_r1_kill_rate: alt_sim.r1_kill_rate - base_sim.r1_kill_rate,
        statistically_significant: significant,
    }
}

fn metrics_from_sim(
    sim: &SimulationResult,
    fidelity_eval: &CrewFidelityEvaluator<'_>,
) -> CounterfactualMetrics {
    let fidelity = fidelity_eval.evaluate(
        &sim.candidate.captain,
        &sim.candidate.bridge,
        &sim.candidate.below_decks,
    );
    CounterfactualMetrics {
        score: scalar_score_from_sim(sim),
        win_rate: sim.win_rate,
        win_rate_ci_low: sim.win_rate_ci_low,
        win_rate_ci_high: sim.win_rate_ci_high,
        stall_rate: sim.stall_rate,
        loss_rate: sim.loss_rate,
        r1_kill_rate: sim.r1_kill_rate,
        avg_hull_remaining: sim.avg_hull_remaining,
        avg_hull_remaining_ci_low: sim.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: sim.avg_hull_remaining_ci_high,
        avg_defender_hull_remaining: sim.avg_defender_hull_remaining,
        trials_run: sim.trials_run,
        chain: sim.chain.clone(),
        fidelity,
    }
}

fn build_ablated_crew(base: &CrewCandidate, seat: SeatRef) -> CrewCandidate {
    let mut c = base.clone();
    match seat {
        SeatRef::Captain => {
            c.captain.clear();
        }
        SeatRef::Bridge(i) => {
            if let Some(slot) = c.bridge.get_mut(i) {
                slot.clear();
            }
        }
        SeatRef::BelowDecks(i) => {
            if let Some(slot) = c.below_decks.get_mut(i) {
                slot.clear();
            }
        }
    }
    c
}

fn build_replaced_crew(base: &CrewCandidate, seat: SeatRef, replacement: &str) -> CrewCandidate {
    let mut c = base.clone();
    match seat {
        SeatRef::Captain => {
            c.captain = replacement.to_string();
        }
        SeatRef::Bridge(i) => {
            if let Some(slot) = c.bridge.get_mut(i) {
                *slot = replacement.to_string();
            }
        }
        SeatRef::BelowDecks(i) => {
            if let Some(slot) = c.below_decks.get_mut(i) {
                *slot = replacement.to_string();
            }
        }
    }
    c
}

fn occupied_seats(crew: &CrewCandidate) -> Vec<(SeatRef, String)> {
    let mut out = Vec::new();
    if !crew.captain.trim().is_empty() {
        out.push((SeatRef::Captain, crew.captain.clone()));
    }
    for (i, b) in crew.bridge.iter().enumerate() {
        if !b.trim().is_empty() {
            out.push((SeatRef::Bridge(i), b.clone()));
        }
    }
    for (i, bd) in crew.below_decks.iter().enumerate() {
        if !bd.trim().is_empty() {
            out.push((SeatRef::BelowDecks(i), bd.clone()));
        }
    }
    out
}

fn describe_trigger_dependencies(
    seat: SeatRef,
    summary: &OfficerRoleSummary,
    all_seats: &[(SeatRef, String, OfficerRoleSummary)],
) -> Vec<String> {
    let mut notes = Vec::new();

    // 1. States supplied by this officer that at least one other crewmate consumes.
    if summary.provides_state != 0 {
        for bit_idx in 0..4u8 {
            let bit = 1u8 << bit_idx;
            if summary.provides_state & bit == 0 {
                continue;
            }
            let consumers: Vec<&str> = all_seats
                .iter()
                .filter(|(s, _, other)| *s != seat && (other.needs_state & bit) != 0)
                .map(|(_, name, _)| name.as_str())
                .collect();
            if !consumers.is_empty() {
                if let Some(label) = state_bit_labels(bit).first() {
                    notes.push(format!("Supplies {label} for {}", consumers.join(", ")));
                }
            }
        }
    }

    // 2. States consumed by this officer that another crewmate supplies.
    if summary.needs_state != 0 {
        for bit_idx in 0..4u8 {
            let bit = 1u8 << bit_idx;
            if summary.needs_state & bit == 0 {
                continue;
            }
            let providers: Vec<&str> = all_seats
                .iter()
                .filter(|(s, _, other)| *s != seat && (other.provides_state & bit) != 0)
                .map(|(_, name, _)| name.as_str())
                .collect();
            if !providers.is_empty() {
                if let Some(label) = state_bit_labels(bit).first() {
                    notes.push(format!("Uses {label} from {}", providers.join(", ")));
                }
            }
        }
    }

    // 3. Captain bridge-effectiveness or captain-maneuver amplification.
    if seat == SeatRef::Captain && summary.bridge_effectiveness_boost > 0.01 {
        notes.push(format!(
            "Amplifies bridge abilities (+{}%)",
            (summary.bridge_effectiveness_boost * 100.0).round() as i64
        ));
    }
    if summary.captain_maneuver_boost > 0.01 {
        notes.push(format!(
            "Amplifies captain maneuver (+{}%)",
            (summary.captain_maneuver_boost * 100.0).round() as i64
        ));
    }

    // 4. Native captain-bridge synergy partnership.
    if let Some(my_syn) = summary.synergy {
        match seat {
            SeatRef::Bridge(_) => {
                if let Some((_, cap_name, cap_sum)) =
                    all_seats.iter().find(|(s, _, _)| *s == SeatRef::Captain)
                {
                    if cap_sum
                        .synergy
                        .is_some_and(|cs| cs.group_id == my_syn.group_id)
                    {
                        notes.push(format!("Synergy partner for Captain {cap_name}"));
                    }
                }
            }
            SeatRef::Captain => {
                let partners: Vec<&str> = all_seats
                    .iter()
                    .filter(|(s, _, other)| {
                        matches!(s, SeatRef::Bridge(_))
                            && other
                                .synergy
                                .is_some_and(|os| os.group_id == my_syn.group_id)
                    })
                    .map(|(_, name, _)| name.as_str())
                    .collect();
                if !partners.is_empty() {
                    notes.push(format!("Receives synergy from {}", partners.join(", ")));
                }
            }
            SeatRef::BelowDecks(_) => {}
        }
    }

    notes
}

fn diff_slot_changes(base: &CrewCandidate, alt: &CrewCandidate) -> Vec<SubstituteSlotChange> {
    let mut changes = Vec::new();
    if normalize_officer_name(&base.captain) != normalize_officer_name(&alt.captain) {
        changes.push(SubstituteSlotChange {
            slot_group: "captain".to_string(),
            slot_index: None,
            from: base.captain.clone(),
            to: alt.captain.clone(),
        });
    }
    let br_len = base.bridge.len().max(alt.bridge.len());
    for i in 0..br_len {
        let f = base.bridge.get(i).cloned().unwrap_or_default();
        let t = alt.bridge.get(i).cloned().unwrap_or_default();
        if normalize_officer_name(&f) != normalize_officer_name(&t) {
            changes.push(SubstituteSlotChange {
                slot_group: "bridge".to_string(),
                slot_index: Some(i),
                from: f,
                to: t,
            });
        }
    }
    let bd_len = base.below_decks.len().max(alt.below_decks.len());
    for i in 0..bd_len {
        let f = base.below_decks.get(i).cloned().unwrap_or_default();
        let t = alt.below_decks.get(i).cloned().unwrap_or_default();
        if normalize_officer_name(&f) != normalize_officer_name(&t) {
            changes.push(SubstituteSlotChange {
                slot_group: "below_decks".to_string(),
                slot_index: Some(i),
                from: f,
                to: t,
            });
        }
    }
    changes
}

fn summarize_substitute(
    base_score: f64,
    alt_score: f64,
    delta: &PairedDelta,
    num_changed: usize,
    replaced_unavailable: bool,
) -> (f64, String) {
    let retention = if base_score > 1e-9 {
        (alt_score / base_score) * 100.0
    } else {
        100.0
    };
    let half_ci = ((delta.delta_score_ci_high - delta.delta_score_ci_low) * 0.5).max(0.0);
    let slot_word = if num_changed == 1 { "seat" } else { "seats" };
    let text = if delta.delta_score > 1e-4 && delta.statistically_significant {
        format!(
            "Outperforms baseline by +{:.2}% (±{:.2}%) swapping {} {}",
            delta.delta_score * 100.0,
            half_ci * 100.0,
            num_changed,
            slot_word
        )
    } else if replaced_unavailable {
        format!(
            "Retains {:.1}% of baseline ({:+.2}% ± {:.2}%) replacing {} unavailable {}",
            retention,
            delta.delta_score * 100.0,
            half_ci * 100.0,
            num_changed,
            slot_word
        )
    } else {
        format!(
            "Retains {:.1}% of baseline ({:+.2}% ± {:.2}%) swapping {} {}",
            retention,
            delta.delta_score * 100.0,
            half_ci * 100.0,
            num_changed,
            slot_word
        )
    };
    (retention, text)
}

fn crew_has_no_duplicates_and_avoids(cand: &CrewCandidate, excluded: &HashSet<String>) -> bool {
    let mut seen = HashSet::new();
    let check = |name: &str, seen: &mut HashSet<String>| -> bool {
        let norm = normalize_officer_name(name);
        if norm.is_empty() {
            return true;
        }
        if excluded.contains(&norm) {
            return false;
        }
        seen.insert(norm)
    };
    if !check(&cand.captain, &mut seen) {
        return false;
    }
    for b in &cand.bridge {
        if !check(b, &mut seen) {
            return false;
        }
    }
    for bd in &cand.below_decks {
        if !check(bd, &mut seen) {
            return false;
        }
    }
    true
}

/// Run CRN counterfactual ablations, per-seat drop-in replacement search, and (when officers are
/// marked unavailable) multi-seat substitute crew synthesis for a baseline crew.
pub fn analyze_crew_counterfactual_with_registry(
    registry: &DataRegistry,
    scenario: &CounterfactualScenario<'_>,
    baseline_crew: &CrewCandidate,
) -> CounterfactualOutcome {
    let shared: SharedScenarioData = build_shared_scenario_data_from_registry(
        registry,
        scenario.ship,
        scenario.hostile,
        scenario.ship_tier,
        scenario.ship_level,
        scenario.profile_id,
        scenario.support_buffs.clone(),
        scenario.defender_opponent,
        scenario.player_defender_officer_crew.clone(),
        scenario.pvp.clone(),
    );
    let fidelity_eval = CrewFidelityEvaluator::from_registry(registry);
    let pools = build_officer_pools_from_registry(
        registry,
        scenario.below_decks_pool_mode,
        Some(scenario.enemy_type),
        scenario.profile_id,
        scenario.below_decks_slots,
        None,
    )
    .unwrap_or(OfficerPools {
        captains: Vec::new(),
        bridge: Vec::new(),
        below_decks: Vec::new(),
    });

    let num_sims = scenario.num_sims.clamp(64, 5_000);
    let max_subs_per_slot = scenario.max_substitutes_per_slot.clamp(1, 5);
    let max_sub_crews = scenario.max_substitute_crews.clamp(1, 10);

    let unavailable_norm: HashSet<String> = scenario
        .unavailable_officers
        .iter()
        .map(|s| normalize_officer_name(s))
        .filter(|s| !s.is_empty())
        .collect();

    // Canonicalize seat roles for baseline.
    let seats = occupied_seats(baseline_crew);
    let seat_summaries: Vec<(SeatRef, String, OfficerRoleSummary)> = seats
        .iter()
        .map(|(seat_ref, name)| {
            let sum = summarize_officer_role(registry, &shared, name, seat_ref.crew_seat());
            (*seat_ref, name.clone(), sum)
        })
        .collect();

    let baseline_norms: HashSet<String> = seats
        .iter()
        .map(|(_, name)| normalize_officer_name(name))
        .collect();

    // Nominate legal 1-slot replacement candidates for every occupied seat.
    let mut seat_nominations: Vec<(SeatRef, Vec<String>, u8)> = Vec::with_capacity(seats.len());
    for (seat_ref, name, sum) in &seat_summaries {
        let my_norm = normalize_officer_name(name);
        let pool_source: &[String] = match seat_ref {
            SeatRef::Captain => &pools.captains,
            SeatRef::Bridge(_) => &pools.bridge,
            SeatRef::BelowDecks(_) => &pools.below_decks,
        };
        let filtered_pool: Vec<String> = pool_source
            .iter()
            .filter(|cand_name| {
                let n = normalize_officer_name(cand_name);
                !n.is_empty()
                    && n != my_norm
                    && !baseline_norms.contains(&n)
                    && !unavailable_norm.contains(&n)
            })
            .cloned()
            .collect();

        // States needed by OTHER seats that only this seat (or mainly this seat) provides.
        let mut needed_by_others = 0u8;
        let mut provided_by_others = 0u8;
        for (other_ref, _, other_sum) in &seat_summaries {
            if other_ref != seat_ref {
                needed_by_others |= other_sum.needs_state;
                provided_by_others |= other_sum.provides_state;
            }
        }
        let desired_states = needed_by_others & !provided_by_others;
        let desired_or_matched = if desired_states != 0 {
            desired_states
        } else {
            sum.provides_state & needed_by_others
        };

        let captain_syn = seat_summaries
            .iter()
            .find(|(s, _, _)| *s == SeatRef::Captain)
            .and_then(|(_, _, s)| s.synergy);
        let other_bridge_syns: Vec<_> = seat_summaries
            .iter()
            .filter(|(s, _, _)| matches!(s, SeatRef::Bridge(_)) && s != seat_ref)
            .filter_map(|(_, _, s)| s.synergy)
            .collect();

        let ranked_names = rank_replacement_names_for_seat(
            registry,
            &shared,
            &filtered_pool,
            seat_ref.crew_seat(),
            desired_or_matched,
            captain_syn,
            &other_bridge_syns,
            PER_SEAT_NOMINATION_CAP,
        );
        seat_nominations.push((*seat_ref, ranked_names, desired_or_matched));
    }

    // Build unified batch for Phase 1 CRN simulation:
    // index 0 = baseline
    // indices 1..=seats.len() = leave-one-out ablations (1 per seat)
    // remaining indices = all legal 1-slot replacement crews (deduplicated by exact slot+name)
    let mut batch: Vec<CrewCandidate> = Vec::new();
    batch.push(baseline_crew.clone());

    let mut ablation_indices: Vec<usize> = Vec::with_capacity(seats.len());
    for (seat_ref, _, _) in &seat_summaries {
        ablation_indices.push(batch.len());
        batch.push(build_ablated_crew(baseline_crew, *seat_ref));
    }

    let mut replacement_batches: Vec<Vec<(String, usize)>> = Vec::with_capacity(seats.len());
    for (seat_ref, cand_names, _) in &seat_nominations {
        let mut raw_cands: Vec<CrewCandidate> = Vec::with_capacity(cand_names.len());
        for cand_name in cand_names {
            raw_cands.push(build_replaced_crew(baseline_crew, *seat_ref, cand_name));
        }
        // Filter through exact roster + seat legality engine!
        let (legal_cands, _) = enforce_candidate_legality_with_registry(
            registry,
            scenario.profile_id,
            scenario.below_decks_slots,
            raw_cands,
        );
        let mut seat_entries = Vec::with_capacity(legal_cands.len());
        for legal in legal_cands {
            let officer_name = match seat_ref {
                SeatRef::Captain => legal.captain.clone(),
                SeatRef::Bridge(i) => legal.bridge.get(*i).cloned().unwrap_or_default(),
                SeatRef::BelowDecks(i) => legal.below_decks.get(*i).cloned().unwrap_or_default(),
            };
            if officer_name.is_empty() {
                continue;
            }
            let idx = batch.len();
            batch.push(legal);
            seat_entries.push((officer_name, idx));
        }
        replacement_batches.push(seat_entries);
    }

    // Execute Phase 1 CRN Monte Carlo batch in parallel!
    let eval_phase1 = evaluate_candidates_crn_with_trial_scores(
        &shared,
        &batch,
        num_sims,
        scenario.seed,
        scenario.chain_grind.as_ref(),
    );
    let mut variants_evaluated = batch.len();

    let (base_sim, base_scores) = &eval_phase1[0];
    let baseline_metrics = metrics_from_sim(base_sim, &fidelity_eval);

    // Assemble per-seat ablations and top-K measured replacements.
    let mut seat_ablations: Vec<SeatAblation> = Vec::with_capacity(seats.len());
    let mut top_replacements_by_seat: Vec<(SeatRef, Vec<String>)> = Vec::with_capacity(seats.len());

    let captain_syn = seat_summaries
        .iter()
        .find(|(s, _, _)| *s == SeatRef::Captain)
        .and_then(|(_, _, s)| s.synergy);

    for (pos, (seat_ref, officer_name, summary)) in seat_summaries.iter().enumerate() {
        let (loo_sim, loo_scores) = &eval_phase1[ablation_indices[pos]];
        let leave_one_out = metrics_from_sim(loo_sim, &fidelity_eval);
        let drop_delta = paired_delta(base_sim, base_scores, loo_sim, loo_scores);
        let marginal_contribution = baseline_metrics.score - leave_one_out.score;
        let trigger_dependencies =
            describe_trigger_dependencies(*seat_ref, summary, &seat_summaries);

        let desired_states = seat_nominations[pos].2;
        let other_bridge_syns: Vec<_> = seat_summaries
            .iter()
            .filter(|(s, _, _)| matches!(s, SeatRef::Bridge(_)) && s != seat_ref)
            .filter_map(|(_, _, s)| s.synergy)
            .collect();

        let mut options: Vec<SeatReplacementOption> = replacement_batches[pos]
            .iter()
            .map(|(rep_name, batch_idx)| {
                let (rep_sim, rep_scores) = &eval_phase1[*batch_idx];
                let rep_metrics = metrics_from_sim(rep_sim, &fidelity_eval);
                let rep_delta = paired_delta(base_sim, base_scores, rep_sim, rep_scores);
                let rep_sum =
                    summarize_officer_role(registry, &shared, rep_name, seat_ref.crew_seat());
                let preserves_triggers = state_bit_labels(rep_sum.provides_state & desired_states)
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                let synergy_match = match (seat_ref, rep_sum.synergy) {
                    (SeatRef::Bridge(_), Some(rs)) => {
                        captain_syn.is_some_and(|cs| cs.group_id == rs.group_id)
                    }
                    (SeatRef::Captain, Some(rs)) => other_bridge_syns
                        .iter()
                        .any(|bs| bs.group_id == rs.group_id),
                    _ => false,
                };
                SeatReplacementOption {
                    officer: rep_name.clone(),
                    metrics: rep_metrics,
                    delta: rep_delta,
                    preserves_triggers,
                    synergy_match,
                }
            })
            .collect();

        options.sort_by(|a, b| {
            b.metrics
                .score
                .total_cmp(&a.metrics.score)
                .then_with(|| b.metrics.win_rate.total_cmp(&a.metrics.win_rate))
                .then_with(|| a.officer.cmp(&b.officer))
        });
        let top_names_for_combo: Vec<String> =
            options.iter().take(4).map(|o| o.officer.clone()).collect();
        top_replacements_by_seat.push((*seat_ref, top_names_for_combo));
        options.truncate(max_subs_per_slot);

        let is_unavailable = unavailable_norm.contains(&normalize_officer_name(officer_name));
        seat_ablations.push(SeatAblation {
            slot_group: seat_ref.group_label().to_string(),
            slot_index: seat_ref.index(),
            officer: officer_name.clone(),
            unavailable: is_unavailable,
            leave_one_out,
            drop_delta,
            marginal_contribution,
            trigger_dependencies,
            best_replacements: options,
        });
    }

    // Phase 2: Substitute Crew Planner.
    let unavailable_seat_positions: Vec<usize> = seat_ablations
        .iter()
        .enumerate()
        .filter(|(_, ab)| ab.unavailable)
        .map(|(i, _)| i)
        .collect();

    let mut substitutes: Vec<SubstituteCrewPlan> = Vec::new();
    let mut seen_sub_hashes: HashSet<u64> = HashSet::new();

    if unavailable_seat_positions.is_empty() {
        // No baseline officers marked unavailable: surface the top legal 1-slot alternatives/upgrades
        // across all seats directly from Phase 1 CRN evaluations (zero extra sims!).
        let mut single_slot_plans: Vec<SubstituteCrewPlan> = Vec::new();
        for (pos, (seat_ref, _, _)) in seat_summaries.iter().enumerate() {
            for (rep_name, batch_idx) in &replacement_batches[pos] {
                let (rep_sim, rep_scores) = &eval_phase1[*batch_idx];
                let h = crew_candidate_stable_hash(&rep_sim.candidate);
                if !seen_sub_hashes.insert(h) {
                    continue;
                }
                let metrics = metrics_from_sim(rep_sim, &fidelity_eval);
                let delta = paired_delta(base_sim, base_scores, rep_sim, rep_scores);
                let changed_slots = vec![SubstituteSlotChange {
                    slot_group: seat_ref.group_label().to_string(),
                    slot_index: seat_ref.index(),
                    from: seat_summaries[pos].1.clone(),
                    to: rep_name.clone(),
                }];
                let (retention_pct, summary) = summarize_substitute(
                    baseline_metrics.score,
                    metrics.score,
                    &delta,
                    changed_slots.len(),
                    false,
                );
                single_slot_plans.push(SubstituteCrewPlan {
                    captain: rep_sim.candidate.captain.clone(),
                    bridge: rep_sim.candidate.bridge.clone(),
                    below_decks: rep_sim.candidate.below_decks.clone(),
                    changed_slots,
                    metrics,
                    delta,
                    retention_pct,
                    summary,
                });
            }
        }
        single_slot_plans.sort_by(|a, b| {
            b.metrics
                .score
                .total_cmp(&a.metrics.score)
                .then_with(|| b.metrics.win_rate.total_cmp(&a.metrics.win_rate))
                .then_with(|| a.captain.cmp(&b.captain))
        });
        single_slot_plans.truncate(max_sub_crews);
        substitutes = single_slot_plans;
    } else if unavailable_seat_positions.len() == 1 {
        // Exactly 1 unavailable seat: Phase 1 already CRN-evaluated every nominated replacement!
        let pos = unavailable_seat_positions[0];
        let seat_ref = seat_summaries[pos].0;
        let from_name = seat_summaries[pos].1.clone();
        let mut plans: Vec<SubstituteCrewPlan> = Vec::new();
        for (rep_name, batch_idx) in &replacement_batches[pos] {
            let (rep_sim, rep_scores) = &eval_phase1[*batch_idx];
            if !crew_has_no_duplicates_and_avoids(&rep_sim.candidate, &unavailable_norm) {
                continue;
            }
            let h = crew_candidate_stable_hash(&rep_sim.candidate);
            if !seen_sub_hashes.insert(h) {
                continue;
            }
            let metrics = metrics_from_sim(rep_sim, &fidelity_eval);
            let delta = paired_delta(base_sim, base_scores, rep_sim, rep_scores);
            let changed_slots = vec![SubstituteSlotChange {
                slot_group: seat_ref.group_label().to_string(),
                slot_index: seat_ref.index(),
                from: from_name.clone(),
                to: rep_name.clone(),
            }];
            let (retention_pct, summary) =
                summarize_substitute(baseline_metrics.score, metrics.score, &delta, 1, true);
            plans.push(SubstituteCrewPlan {
                captain: rep_sim.candidate.captain.clone(),
                bridge: rep_sim.candidate.bridge.clone(),
                below_decks: rep_sim.candidate.below_decks.clone(),
                changed_slots,
                metrics,
                delta,
                retention_pct,
                summary,
            });
        }
        plans.sort_by(|a, b| {
            b.metrics
                .score
                .total_cmp(&a.metrics.score)
                .then_with(|| b.metrics.win_rate.total_cmp(&a.metrics.win_rate))
                .then_with(|| a.captain.cmp(&b.captain))
        });
        plans.truncate(max_sub_crews);
        substitutes = plans;
    } else {
        // 2+ unavailable seats: start from baseline with every unavailable seat vacated so
        // intermediate states never trip the unavailable-officer check while filling seats.
        let mut cleared_base = baseline_crew.clone();
        for &pos in &unavailable_seat_positions {
            cleared_base = build_ablated_crew(&cleared_base, seat_summaries[pos].0);
        }
        let mut partial_crews: Vec<CrewCandidate> = vec![cleared_base];
        let per_seat_fanout = if unavailable_seat_positions.len() == 2 {
            4
        } else if unavailable_seat_positions.len() == 3 {
            3
        } else {
            2
        };

        for &pos in &unavailable_seat_positions {
            let (seat_ref, top_names) = &top_replacements_by_seat[pos];
            let pool_fallback: &[String] = match seat_ref {
                SeatRef::Captain => &pools.captains,
                SeatRef::Bridge(_) => &pools.bridge,
                SeatRef::BelowDecks(_) => &pools.below_decks,
            };
            let mut choices: Vec<String> =
                top_names.iter().take(per_seat_fanout).cloned().collect();
            if choices.len() < per_seat_fanout {
                for cand in pool_fallback {
                    let n = normalize_officer_name(cand);
                    if !unavailable_norm.contains(&n)
                        && !choices.iter().any(|c| normalize_officer_name(c) == n)
                    {
                        choices.push(cand.clone());
                        if choices.len() >= per_seat_fanout {
                            break;
                        }
                    }
                }
            }
            let mut next_partial = Vec::new();
            for partial in &partial_crews {
                for choice in &choices {
                    let next = build_replaced_crew(partial, *seat_ref, choice);
                    if crew_has_no_duplicates_and_avoids(&next, &unavailable_norm) {
                        next_partial.push(next);
                        if next_partial.len() >= MAX_MULTI_SEAT_CANDIDATES * 2 {
                            break;
                        }
                    }
                }
                if next_partial.len() >= MAX_MULTI_SEAT_CANDIDATES * 2 {
                    break;
                }
            }
            if !next_partial.is_empty() {
                partial_crews = next_partial;
            }
        }

        // Deduplicate & verify full legality.
        let mut deduped: Vec<CrewCandidate> = Vec::new();
        for c in partial_crews {
            if !crew_has_no_duplicates_and_avoids(&c, &unavailable_norm) {
                continue;
            }
            let h = crew_candidate_stable_hash(&c);
            if seen_sub_hashes.insert(h) {
                deduped.push(c);
            }
        }
        let (mut legal_subs, _) = enforce_candidate_legality_with_registry(
            registry,
            scenario.profile_id,
            scenario.below_decks_slots,
            deduped,
        );
        legal_subs.truncate(MAX_MULTI_SEAT_CANDIDATES);

        if !legal_subs.is_empty() {
            let eval_phase2 = evaluate_candidates_crn_with_trial_scores(
                &shared,
                &legal_subs,
                num_sims,
                scenario.seed,
                scenario.chain_grind.as_ref(),
            );
            variants_evaluated += legal_subs.len();

            let mut plans: Vec<SubstituteCrewPlan> = eval_phase2
                .iter()
                .map(|(sub_sim, sub_scores)| {
                    let metrics = metrics_from_sim(sub_sim, &fidelity_eval);
                    let delta = paired_delta(base_sim, base_scores, sub_sim, sub_scores);
                    let changed_slots = diff_slot_changes(baseline_crew, &sub_sim.candidate);
                    let (retention_pct, summary) = summarize_substitute(
                        baseline_metrics.score,
                        metrics.score,
                        &delta,
                        changed_slots.len(),
                        true,
                    );
                    SubstituteCrewPlan {
                        captain: sub_sim.candidate.captain.clone(),
                        bridge: sub_sim.candidate.bridge.clone(),
                        below_decks: sub_sim.candidate.below_decks.clone(),
                        changed_slots,
                        metrics,
                        delta,
                        retention_pct,
                        summary,
                    }
                })
                .collect();
            plans.sort_by(|a, b| {
                b.metrics
                    .score
                    .total_cmp(&a.metrics.score)
                    .then_with(|| b.metrics.win_rate.total_cmp(&a.metrics.win_rate))
                    .then_with(|| a.captain.cmp(&b.captain))
            });
            plans.truncate(max_sub_crews);
            substitutes = plans;
        }
    }

    let mut warnings = Vec::new();
    if shared.using_placeholder_combatants {
        warnings.push(
            "Ship or hostile record was missing and fell back to placeholder stats.".to_string(),
        );
    }
    if !unavailable_seat_positions.is_empty() && substitutes.is_empty() {
        warnings.push(
            "No legal substitute crew could be assembled from the remaining roster after excluding unavailable officers."
                .to_string(),
        );
    }

    let _ = BRIDGE_SLOTS;
    CounterfactualOutcome {
        seed: scenario.seed,
        trials_per_variant: num_sims,
        captain: baseline_crew.captain.clone(),
        bridge: baseline_crew.bridge.clone(),
        below_decks: baseline_crew.below_decks.clone(),
        baseline: baseline_metrics,
        seat_ablations,
        unavailable_officers: scenario.unavailable_officers.clone(),
        substitutes,
        variants_evaluated,
        using_placeholder_combatants: shared.using_placeholder_combatants,
        warnings,
    }
}

/// Resolve normalized seat-name helper used by tests & fleet cross-checks.
pub(crate) fn crew_officer_norms(crew: &CrewCandidate) -> Vec<String> {
    let mut out = Vec::new();
    for raw in std::iter::once(crew.captain.as_str())
        .chain(crew.bridge.iter().map(String::as_str))
        .chain(crew.below_decks.iter().map(String::as_str))
    {
        let n = normalize_officer_name(raw);
        if !n.is_empty() {
            out.push(n);
        }
    }
    out
}

/// Lookup table mapping officer id or name -> canonical name for API coercion.
pub(crate) fn canonical_unavailable_names(
    raw: &[String],
    officers: &[(String, String)],
) -> Vec<String> {
    let by_key: HashMap<String, String> = officers
        .iter()
        .flat_map(|(id, name)| {
            [
                (normalize_officer_name(id), name.clone()),
                (normalize_officer_name(name), name.clone()),
            ]
        })
        .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in raw {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resolved = by_key
            .get(&normalize_officer_name(trimmed))
            .cloned()
            .unwrap_or_else(|| trimmed.to_string());
        let norm = normalize_officer_name(&resolved);
        if seen.insert(norm) {
            out.push(resolved);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::profile_index::DEMO_PROFILE_ID;

    #[test]
    fn paired_delta_zero_variance_on_identical_scores() {
        let base = SimulationResult {
            candidate: CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "C".into()],
                below_decks: vec![],
            },
            unsupported_crew_health_effects: None,
            trials_run: 4,
            win_rate: 0.75,
            win_rate_ci_low: 0.3,
            win_rate_ci_high: 0.95,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 0.25,
            loss_rate_ci_low: 0.05,
            loss_rate_ci_high: 0.7,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.2,
            avg_hull_remaining_ci_high: 0.8,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            chain: None,
            expected_hull_damage: None,
        };
        let scores = vec![0.9, 0.9, 0.9, 0.0];
        let d = paired_delta(&base, &scores, &base, &scores);
        assert!(d.delta_score.abs() < 1e-12);
        assert!(d.delta_score_ci_low.abs() < 1e-12);
        assert!(d.delta_score_ci_high.abs() < 1e-12);
        assert!(!d.statistically_significant);
    }

    #[test]
    fn counterfactual_ablations_and_substitutes_are_deterministic_and_legal() {
        let registry = DataRegistry::load().expect("registry");
        let crew = CrewCandidate {
            captain: "Cadet Uhura".into(),
            bridge: vec!["Badgey".into(), "Nesmith".into()],
            below_decks: vec!["Odo".into(), "Harry Kim".into()],
        };
        let scenario = CounterfactualScenario {
            ship: "uss_saladin",
            hostile: "1140710508",
            ship_tier: None,
            ship_level: None,
            below_decks_slots: 2,
            below_decks_pool_mode: BelowDecksPoolMode::default(),
            profile_id: Some(DEMO_PROFILE_ID),
            support_buffs: SupportBuffScenarioRequest::default(),
            chain_grind: None,
            defender_opponent: DefenderOpponent::Hostile,
            player_defender_officer_crew: None,
            pvp: None,
            enemy_type: EnemyType::RedMovingSpace,
            num_sims: 120,
            seed: 42,
            unavailable_officers: vec!["Cadet Uhura".into(), "Badgey".into()],
            max_substitutes_per_slot: 3,
            max_substitute_crews: 4,
        };

        let out1 = analyze_crew_counterfactual_with_registry(&registry, &scenario, &crew);
        let out2 = analyze_crew_counterfactual_with_registry(&registry, &scenario, &crew);

        assert_eq!(out1.seat_ablations.len(), 5);
        assert!((out1.baseline.score - out2.baseline.score).abs() < 1e-12);
        assert!(
            out1.seat_ablations[0].marginal_contribution > 0.005,
            "Cadet Uhura should show positive marginal contribution on Saladin vs 1140710508, got {:?}",
            out1.seat_ablations[0].marginal_contribution
        );
        assert!(out1.seat_ablations[0].unavailable);
        assert!(out1.seat_ablations[1].unavailable);
        assert!(!out1.seat_ablations[2].unavailable);
        assert!(
            !out1.substitutes.is_empty(),
            "should find legal multi-seat substitute crews"
        );
        for sub in &out1.substitutes {
            let norms = crew_officer_norms(&CrewCandidate {
                captain: sub.captain.clone(),
                bridge: sub.bridge.clone(),
                below_decks: sub.below_decks.clone(),
            });
            assert!(
                !norms.contains(&normalize_officer_name("Cadet Uhura")),
                "substitute must exclude unavailable Cadet Uhura: {sub:?}"
            );
            assert!(
                !norms.contains(&normalize_officer_name("Badgey")),
                "substitute must exclude unavailable Badgey: {sub:?}"
            );
            let unique: HashSet<&String> = norms.iter().collect();
            assert_eq!(
                unique.len(),
                norms.len(),
                "substitute must have no duplicates"
            );
        }
    }
}
