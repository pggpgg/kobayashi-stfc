//! Per-profile optimize result cache (`profiles/{id}/optimize_history.json`).
//!
//! Keys are opaque client fingerprints (`optimize_cache_key`). Entries store resolved run
//! parameters plus Monte Carlo aggregates so a later **tiered** or **exhaustive two-phase**
//! run can skip scout and/or confirm for matching crews when metadata still aligns.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::data::profile_index::{profile_path, OPTIMIZE_HISTORY_JSON};
use crate::optimizer::chain::ChainGrindParams;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::{crew_candidate_stable_hash, SimulationResult};
use crate::optimizer::ranking::{RankedCrewResult, RankingScore};

pub const OPTIMIZE_HISTORY_SCHEMA: u32 = 1;
pub const MAX_OPTIMIZE_CACHE_KEYS: usize = 200;
pub const MAX_OPTIMIZE_HISTORY_CREWS: usize = 24;
pub const MAX_OPTIMIZE_CACHE_KEY_BYTES: usize = 512;

/// Max persisted-history crews merged into analytical **matchup priors** only (not prepended as candidates).
pub const MAX_PRIOR_REFERENCE_CREWS_FROM_HISTORY: usize = 16;

/// Entries without [`OptimizeHistoryEntry::tiered_budget_policy`] deserialize as this value.
pub const TIERED_BUDGET_POLICY_LEGACY: u8 = 0;
/// Current tiered Monte Carlo budget allocator (adaptive coarse fraction, ranking-aligned confirm widths, optional confirm cap).
pub const TIERED_BUDGET_POLICY_V2: u8 = 2;

/// [`OptimizeHistoryEntry::optimize_history_kind`] — tiered cache row (default for legacy JSON).
pub const OPTIMIZE_HISTORY_KIND_TIERED: u8 = 0;
/// Exhaustive two-phase scout→confirm cache row.
pub const OPTIMIZE_HISTORY_KIND_EXHAUSTIVE_TWO_PHASE: u8 = 1;

/// Exhaustive two-phase: confirmation iterations allocated from scout ranking widths (must match run).
pub const EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OptimizeHistoryFile {
    pub schema: u32,
    #[serde(default)]
    pub entries: HashMap<String, OptimizeHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeHistoryEntry {
    pub updated_at_ms: u128,
    pub sims: u32,
    pub seed: u64,
    pub tiered_scout_sims: usize,
    pub tiered_top_k: usize,
    pub n_candidates: usize,
    /// [`OPTIMIZE_HISTORY_KIND_TIERED`] vs [`OPTIMIZE_HISTORY_KIND_EXHAUSTIVE_TWO_PHASE`].
    #[serde(default)]
    pub optimize_history_kind: u8,
    /// `0` = uniform single-pass scout; `1` = adaptive coarse→refine scout (must match current run).
    #[serde(default)]
    pub tiered_scout_allocator: u8,
    /// `0` = legacy (missing field on disk); [`TIERED_BUDGET_POLICY_V2`] = current confirm/scout budget semantics.
    #[serde(default)]
    pub tiered_budget_policy: u8,
    /// When set, must match the optimize request `tiered_confirm_budget_cap_mult` (global confirm shrink).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiered_confirm_cap_mult: Option<f32>,
    /// Exhaustive two-phase: scout sims per crew (meaningful when [`Self::optimize_history_kind`] is exhaustive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaustive_scout_sims: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaustive_scout_top_keep: Option<usize>,
    /// [`EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1`] for current exhaustive confirm allocator.
    #[serde(default)]
    pub exhaustive_confirm_policy: u8,
    /// Same encoding as the SPA warm-start key chain segment (`0` vs `1:kt:secondary`).
    pub chain_fingerprint: String,
    pub crews: Vec<OptimizeHistoryCrewRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeHistoryCrewRecord {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
    pub win_rate: f64,
    pub win_rate_ci_low: f64,
    pub win_rate_ci_high: f64,
    pub stall_rate: f64,
    pub stall_rate_ci_low: f64,
    pub stall_rate_ci_high: f64,
    pub loss_rate: f64,
    pub loss_rate_ci_low: f64,
    pub loss_rate_ci_high: f64,
    pub r1_kill_rate: f64,
    pub r1_kill_rate_ci_low: f64,
    pub r1_kill_rate_ci_high: f64,
    pub avg_hull_remaining: f64,
    pub avg_hull_remaining_ci_low: f64,
    pub avg_hull_remaining_ci_high: f64,
    pub avg_defender_hull_remaining: f64,
    pub avg_defender_hull_remaining_ci_low: f64,
    pub avg_defender_hull_remaining_ci_high: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<crate::optimizer::chain::ChainSimulationSummary>,
    #[serde(default)]
    pub trials_run: usize,
}

/// Matches [`frontend/src/lib/optimizeWarmStart.ts`] `chain` segment when chain grind is enabled.
pub fn chain_fingerprint(chain: &Option<ChainGrindParams>) -> String {
    match chain {
        None => "0".to_string(),
        Some(p) => format!(
            "1:{}:{}",
            p.kills_target,
            match p.secondary {
                crate::optimizer::chain::ChainSecondaryObjective::MinHullDamage => {
                    "min_hull_damage"
                }
                crate::optimizer::chain::ChainSecondaryObjective::MaxLootPerHullProxy => {
                    "max_loot_per_hull_proxy"
                }
            }
        ),
    }
}

impl OptimizeHistoryCrewRecord {
    pub fn from_simulation(r: &SimulationResult) -> Self {
        Self {
            captain: r.candidate.captain.clone(),
            bridge: r.candidate.bridge.clone(),
            below_decks: r.candidate.below_decks.clone(),
            win_rate: r.win_rate,
            win_rate_ci_low: r.win_rate_ci_low,
            win_rate_ci_high: r.win_rate_ci_high,
            stall_rate: r.stall_rate,
            stall_rate_ci_low: r.stall_rate_ci_low,
            stall_rate_ci_high: r.stall_rate_ci_high,
            loss_rate: r.loss_rate,
            loss_rate_ci_low: r.loss_rate_ci_low,
            loss_rate_ci_high: r.loss_rate_ci_high,
            r1_kill_rate: r.r1_kill_rate,
            r1_kill_rate_ci_low: r.r1_kill_rate_ci_low,
            r1_kill_rate_ci_high: r.r1_kill_rate_ci_high,
            avg_hull_remaining: r.avg_hull_remaining,
            avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
            avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
            avg_defender_hull_remaining: r.avg_defender_hull_remaining,
            avg_defender_hull_remaining_ci_low: r.avg_defender_hull_remaining_ci_low,
            avg_defender_hull_remaining_ci_high: r.avg_defender_hull_remaining_ci_high,
            chain: r.chain.clone(),
            trials_run: r.trials_run,
        }
    }

    pub fn to_simulation_result(&self) -> SimulationResult {
        SimulationResult {
            candidate: CrewCandidate {
                captain: self.captain.clone(),
                bridge: self.bridge.clone(),
                below_decks: self.below_decks.clone(),
            },
            trials_run: self.trials_run,
            win_rate: self.win_rate,
            win_rate_ci_low: self.win_rate_ci_low,
            win_rate_ci_high: self.win_rate_ci_high,
            stall_rate: self.stall_rate,
            stall_rate_ci_low: self.stall_rate_ci_low,
            stall_rate_ci_high: self.stall_rate_ci_high,
            loss_rate: self.loss_rate,
            loss_rate_ci_low: self.loss_rate_ci_low,
            loss_rate_ci_high: self.loss_rate_ci_high,
            r1_kill_rate: self.r1_kill_rate,
            r1_kill_rate_ci_low: self.r1_kill_rate_ci_low,
            r1_kill_rate_ci_high: self.r1_kill_rate_ci_high,
            avg_hull_remaining: self.avg_hull_remaining,
            avg_hull_remaining_ci_low: self.avg_hull_remaining_ci_low,
            avg_hull_remaining_ci_high: self.avg_hull_remaining_ci_high,
            avg_defender_hull_remaining: self.avg_defender_hull_remaining,
            avg_defender_hull_remaining_ci_low: self.avg_defender_hull_remaining_ci_low,
            avg_defender_hull_remaining_ci_high: self.avg_defender_hull_remaining_ci_high,
            chain: self.chain.clone(),
        }
    }
}

pub fn validate_optimize_cache_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_OPTIMIZE_CACHE_KEY_BYTES
        && !key.chars().any(|c| c.is_control())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn load_history_file(profile_id: &str) -> OptimizeHistoryFile {
    let path = profile_path(profile_id, OPTIMIZE_HISTORY_JSON);
    if !path.exists() {
        return OptimizeHistoryFile {
            schema: OPTIMIZE_HISTORY_SCHEMA,
            entries: HashMap::new(),
        };
    }
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut file: OptimizeHistoryFile =
        serde_json::from_str(&raw).unwrap_or_else(|_| OptimizeHistoryFile {
            schema: OPTIMIZE_HISTORY_SCHEMA,
            entries: HashMap::new(),
        });
    if file.schema != OPTIMIZE_HISTORY_SCHEMA {
        file.entries.clear();
        file.schema = OPTIMIZE_HISTORY_SCHEMA;
    }
    file
}

fn evict_oldest_if_needed(entries: &mut HashMap<String, OptimizeHistoryEntry>) {
    while entries.len() > MAX_OPTIMIZE_CACHE_KEYS {
        let remove_key = entries
            .iter()
            .min_by_key(|(_, e)| e.updated_at_ms)
            .map(|(k, _)| k.clone());
        if let Some(k) = remove_key {
            entries.remove(&k);
        } else {
            break;
        }
    }
}

/// Persist merged history (best-effort; errors ignored by callers).
pub fn save_history_file(profile_id: &str, file: &OptimizeHistoryFile) -> io::Result<()> {
    let path = profile_path(profile_id, OPTIMIZE_HISTORY_JSON);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(file)?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Build [`CrewCandidate`] rows from a history entry for [`crate::optimizer::matchup_priors`]
/// when the entry’s [`OptimizeHistoryEntry::chain_fingerprint`] matches `chain` (same grind mode as the current run).
///
/// Does **not** require `entry_matches_run` metadata equality: priors intentionally reuse past winners
/// for the same client fingerprint and chain, even when sim counts or seeds differ.
pub fn prior_reference_crews_from_entry(
    entry: &OptimizeHistoryEntry,
    chain: &Option<ChainGrindParams>,
) -> Vec<CrewCandidate> {
    let chain_fp = chain_fingerprint(chain);
    if entry.chain_fingerprint != chain_fp {
        return Vec::new();
    }
    let mut rows: Vec<&OptimizeHistoryCrewRecord> = entry.crews.iter().collect();
    rows.sort_by(|a, b| {
        b.win_rate
            .partial_cmp(&a.win_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.into_iter()
        .take(MAX_PRIOR_REFERENCE_CREWS_FROM_HISTORY)
        .map(|r| CrewCandidate {
            captain: r.captain.trim().to_string(),
            bridge: r.bridge.clone(),
            below_decks: r.below_decks.clone(),
        })
        .collect()
}

/// Load [`prior_reference_crews_from_entry`] for `profile_id` + `cache_key` when the on-disk entry exists.
pub fn prior_reference_crews_for_matchup_priors(
    profile_id: &str,
    cache_key: &str,
    chain: &Option<ChainGrindParams>,
) -> Vec<CrewCandidate> {
    if !validate_optimize_cache_key(cache_key) {
        return Vec::new();
    }
    let file = load_history_file(profile_id);
    let Some(entry) = file.entries.get(cache_key) else {
        return Vec::new();
    };
    prior_reference_crews_from_entry(entry, chain)
}

fn crew_record_to_ranked_anchor(r: &OptimizeHistoryCrewRecord) -> RankedCrewResult {
    let score_val = if r.chain.is_some() {
        r.win_rate as f32 * 1e4 + r.avg_hull_remaining.min(1.0) as f32
    } else {
        (r.win_rate * 0.8 + r.avg_hull_remaining * 0.2) as f32
    };
    RankedCrewResult {
        captain: r.captain.trim().to_string(),
        bridge: r.bridge.clone(),
        below_decks: r.below_decks.clone(),
        trials_run: r.trials_run,
        win_rate: r.win_rate,
        win_rate_ci_low: r.win_rate_ci_low,
        win_rate_ci_high: r.win_rate_ci_high,
        stall_rate: r.stall_rate,
        stall_rate_ci_low: r.stall_rate_ci_low,
        stall_rate_ci_high: r.stall_rate_ci_high,
        loss_rate: r.loss_rate,
        loss_rate_ci_low: r.loss_rate_ci_low,
        loss_rate_ci_high: r.loss_rate_ci_high,
        r1_kill_rate: r.r1_kill_rate,
        r1_kill_rate_ci_low: r.r1_kill_rate_ci_low,
        r1_kill_rate_ci_high: r.r1_kill_rate_ci_high,
        avg_hull_remaining: r.avg_hull_remaining,
        avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
        avg_defender_hull_remaining: r.avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low: r.avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high: r.avg_defender_hull_remaining_ci_high,
        score: RankingScore { value: score_val },
        chain: r.chain.clone(),
    }
}

/// Persisted top crews from `optimize_history.json` for novelty MMR redundancy only (never emitted as recommendations).
pub fn novelty_anchor_rows_for_profile_cache_key(
    profile_id: &str,
    cache_key: &str,
    chain: &Option<ChainGrindParams>,
) -> Vec<RankedCrewResult> {
    if !validate_optimize_cache_key(cache_key) {
        return Vec::new();
    }
    let file = load_history_file(profile_id);
    let Some(entry) = file.entries.get(cache_key) else {
        return Vec::new();
    };
    novelty_anchor_rows_from_history_entry(entry, chain)
}

fn novelty_anchor_rows_from_history_entry(
    entry: &OptimizeHistoryEntry,
    chain: &Option<ChainGrindParams>,
) -> Vec<RankedCrewResult> {
    let chain_fp = chain_fingerprint(chain);
    if entry.chain_fingerprint != chain_fp {
        return Vec::new();
    }
    let mut rows: Vec<&OptimizeHistoryCrewRecord> = entry.crews.iter().collect();
    rows.sort_by(|a, b| {
        b.win_rate
            .partial_cmp(&a.win_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.into_iter()
        .take(MAX_PRIOR_REFERENCE_CREWS_FROM_HISTORY)
        .map(crew_record_to_ranked_anchor)
        .collect()
}

pub fn upsert_entry(
    profile_id: &str,
    cache_key: &str,
    entry: OptimizeHistoryEntry,
) -> io::Result<()> {
    let mut file = load_history_file(profile_id);
    file.entries.insert(cache_key.to_string(), entry);
    evict_oldest_if_needed(&mut file.entries);
    save_history_file(profile_id, &file)
}

#[allow(clippy::too_many_arguments)]
pub fn entry_matches_run(
    entry: &OptimizeHistoryEntry,
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    tiered_scout_allocator: u8,
    chain_fp: &str,
    tiered_budget_policy: u8,
    tiered_confirm_cap_mult: Option<f32>,
) -> bool {
    if entry.optimize_history_kind != OPTIMIZE_HISTORY_KIND_TIERED {
        return false;
    }
    entry.sims == sims
        && entry.seed == seed
        && entry.tiered_scout_sims == tiered_scout_sims
        && entry.tiered_top_k == tiered_top_k
        && entry.n_candidates == n_candidates
        && entry.tiered_scout_allocator == tiered_scout_allocator
        && entry.chain_fingerprint == chain_fp
        && entry.tiered_budget_policy == tiered_budget_policy
        && entry.tiered_confirm_cap_mult == tiered_confirm_cap_mult
}

#[allow(clippy::too_many_arguments)]
pub fn entry_matches_exhaustive_two_phase(
    entry: &OptimizeHistoryEntry,
    sims: u32,
    seed: u64,
    n_candidates: usize,
    exhaustive_scout_sims: usize,
    exhaustive_top_keep: usize,
    chain_fp: &str,
    exhaustive_confirm_policy: u8,
    tiered_confirm_cap_mult: Option<f32>,
) -> bool {
    if entry.optimize_history_kind != OPTIMIZE_HISTORY_KIND_EXHAUSTIVE_TWO_PHASE {
        return false;
    }
    entry.sims == sims
        && entry.seed == seed
        && entry.n_candidates == n_candidates
        && entry.exhaustive_scout_sims == Some(exhaustive_scout_sims)
        && entry.exhaustive_scout_top_keep == Some(exhaustive_top_keep)
        && entry.exhaustive_confirm_policy == exhaustive_confirm_policy
        && entry.chain_fingerprint == chain_fp
        && entry.tiered_confirm_cap_mult == tiered_confirm_cap_mult
}

/// Like [`preconfirmed_for_candidates`] for exhaustive two-phase runs (`optimize_history_kind` exhaustive).
#[allow(clippy::too_many_arguments)]
pub fn preconfirmed_for_exhaustive_two_phase(
    profile_id: &str,
    cache_key: &str,
    sims: u32,
    seed: u64,
    n_candidates: usize,
    exhaustive_scout_sims: usize,
    exhaustive_top_keep: usize,
    chain: &Option<ChainGrindParams>,
    exhaustive_confirm_policy: u8,
    tiered_confirm_cap_mult: Option<f32>,
    candidates: &[CrewCandidate],
) -> (HashMap<u64, SimulationResult>, u32) {
    let chain_fp = chain_fingerprint(chain);
    let file = load_history_file(profile_id);
    let Some(entry) = file.entries.get(cache_key) else {
        return (HashMap::new(), 0);
    };
    if !entry_matches_exhaustive_two_phase(
        entry,
        sims,
        seed,
        n_candidates,
        exhaustive_scout_sims,
        exhaustive_top_keep,
        &chain_fp,
        exhaustive_confirm_policy,
        tiered_confirm_cap_mult,
    ) {
        return (HashMap::new(), 0);
    }
    let mut map: HashMap<u64, SimulationResult> = HashMap::new();
    for row in &entry.crews {
        let sim = row.to_simulation_result();
        map.insert(crew_candidate_stable_hash(&sim.candidate), sim);
    }
    let mut hits: u32 = 0;
    for c in candidates {
        if map.contains_key(&crew_candidate_stable_hash(c)) {
            hits += 1;
        }
    }
    (map, hits)
}

/// Build preconfirmed map when file entry matches this run's tiered metadata.
#[allow(clippy::too_many_arguments)]
pub fn preconfirmed_for_candidates(
    profile_id: &str,
    cache_key: &str,
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    tiered_scout_allocator: u8,
    chain: &Option<ChainGrindParams>,
    tiered_budget_policy: u8,
    tiered_confirm_cap_mult: Option<f32>,
    candidates: &[CrewCandidate],
) -> (HashMap<u64, SimulationResult>, u32) {
    let chain_fp = chain_fingerprint(chain);
    let file = load_history_file(profile_id);
    let Some(entry) = file.entries.get(cache_key) else {
        return (HashMap::new(), 0);
    };
    if !entry_matches_run(
        entry,
        sims,
        seed,
        tiered_scout_sims,
        tiered_top_k,
        n_candidates,
        tiered_scout_allocator,
        &chain_fp,
        tiered_budget_policy,
        tiered_confirm_cap_mult,
    ) {
        return (HashMap::new(), 0);
    }
    let mut map: HashMap<u64, SimulationResult> = HashMap::new();
    for row in &entry.crews {
        let sim = row.to_simulation_result();
        map.insert(crew_candidate_stable_hash(&sim.candidate), sim);
    }
    let mut hits: u32 = 0;
    for c in candidates {
        if map.contains_key(&crew_candidate_stable_hash(c)) {
            hits += 1;
        }
    }
    (map, hits)
}

fn ranked_to_simulation(r: &RankedCrewResult) -> SimulationResult {
    SimulationResult {
        candidate: CrewCandidate {
            captain: r.captain.clone(),
            bridge: r.bridge.clone(),
            below_decks: r.below_decks.clone(),
        },
        win_rate: r.win_rate,
        win_rate_ci_low: r.win_rate_ci_low,
        win_rate_ci_high: r.win_rate_ci_high,
        stall_rate: r.stall_rate,
        stall_rate_ci_low: r.stall_rate_ci_low,
        stall_rate_ci_high: r.stall_rate_ci_high,
        loss_rate: r.loss_rate,
        loss_rate_ci_low: r.loss_rate_ci_low,
        loss_rate_ci_high: r.loss_rate_ci_high,
        r1_kill_rate: r.r1_kill_rate,
        r1_kill_rate_ci_low: r.r1_kill_rate_ci_low,
        r1_kill_rate_ci_high: r.r1_kill_rate_ci_high,
        avg_hull_remaining: r.avg_hull_remaining,
        avg_hull_remaining_ci_low: r.avg_hull_remaining_ci_low,
        avg_hull_remaining_ci_high: r.avg_hull_remaining_ci_high,
        avg_defender_hull_remaining: r.avg_defender_hull_remaining,
        avg_defender_hull_remaining_ci_low: r.avg_defender_hull_remaining_ci_low,
        avg_defender_hull_remaining_ci_high: r.avg_defender_hull_remaining_ci_high,
        chain: r.chain.clone(),
        trials_run: r.trials_run,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_entry_from_ranked(
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    tiered_scout_allocator: u8,
    chain: &Option<ChainGrindParams>,
    tiered_budget_policy: u8,
    tiered_confirm_cap_mult: Option<f32>,
    ranked: &[RankedCrewResult],
) -> OptimizeHistoryEntry {
    let crews: Vec<OptimizeHistoryCrewRecord> = ranked
        .iter()
        .take(MAX_OPTIMIZE_HISTORY_CREWS)
        .map(|r| OptimizeHistoryCrewRecord::from_simulation(&ranked_to_simulation(r)))
        .collect();
    OptimizeHistoryEntry {
        updated_at_ms: now_ms(),
        sims,
        seed,
        tiered_scout_sims,
        tiered_top_k,
        n_candidates,
        optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
        tiered_scout_allocator,
        tiered_budget_policy,
        tiered_confirm_cap_mult,
        exhaustive_scout_sims: None,
        exhaustive_scout_top_keep: None,
        exhaustive_confirm_policy: 0,
        chain_fingerprint: chain_fingerprint(chain),
        crews,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_entry_from_ranked_exhaustive_two_phase(
    sims: u32,
    seed: u64,
    n_candidates: usize,
    exhaustive_scout_sims: usize,
    exhaustive_top_keep: usize,
    exhaustive_confirm_policy: u8,
    chain: &Option<ChainGrindParams>,
    tiered_confirm_cap_mult: Option<f32>,
    ranked: &[RankedCrewResult],
) -> OptimizeHistoryEntry {
    let crews: Vec<OptimizeHistoryCrewRecord> = ranked
        .iter()
        .take(MAX_OPTIMIZE_HISTORY_CREWS)
        .map(|r| OptimizeHistoryCrewRecord::from_simulation(&ranked_to_simulation(r)))
        .collect();
    OptimizeHistoryEntry {
        updated_at_ms: now_ms(),
        sims,
        seed,
        tiered_scout_sims: 0,
        tiered_top_k: 0,
        n_candidates,
        optimize_history_kind: OPTIMIZE_HISTORY_KIND_EXHAUSTIVE_TWO_PHASE,
        tiered_scout_allocator: 0,
        tiered_budget_policy: TIERED_BUDGET_POLICY_LEGACY,
        tiered_confirm_cap_mult,
        exhaustive_scout_sims: Some(exhaustive_scout_sims),
        exhaustive_scout_top_keep: Some(exhaustive_top_keep),
        exhaustive_confirm_policy,
        chain_fingerprint: chain_fingerprint(chain),
        crews,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_entry_matches_metadata() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 40,
            seed: 9,
            tiered_scout_sims: 0,
            tiered_top_k: 0,
            n_candidates: 60,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_EXHAUSTIVE_TWO_PHASE,
            tiered_scout_allocator: 0,
            tiered_budget_policy: TIERED_BUDGET_POLICY_LEGACY,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: Some(12),
            exhaustive_scout_top_keep: Some(4),
            exhaustive_confirm_policy: EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1,
            chain_fingerprint: "0".into(),
            crews: vec![],
        };
        assert!(entry_matches_exhaustive_two_phase(
            &entry,
            40,
            9,
            60,
            12,
            4,
            "0",
            EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1,
            None
        ));
        assert!(!entry_matches_exhaustive_two_phase(
            &entry,
            40,
            9,
            61,
            12,
            4,
            "0",
            EXHAUSTIVE_CONFIRM_POLICY_WIDTH_V1,
            None
        ));
        assert!(!entry_matches_run(
            &entry,
            40,
            9,
            0,
            0,
            60,
            0,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
    }

    #[test]
    fn validate_cache_key_rejects_control_chars() {
        assert!(!validate_optimize_cache_key("a\nb"));
        assert!(validate_optimize_cache_key("3|demo|ship"));
    }

    #[test]
    fn entry_metadata_mismatch_invalidates() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 100,
            seed: 2,
            tiered_scout_sims: 500,
            tiered_top_k: 20,
            n_candidates: 50,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
            tiered_scout_allocator: 1,
            tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".into(),
            crews: vec![],
        };
        assert!(!entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            49,
            1,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
        assert!(entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            50,
            1,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
    }

    #[test]
    fn entry_allocator_mismatch_invalidates() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 100,
            seed: 2,
            tiered_scout_sims: 500,
            tiered_top_k: 20,
            n_candidates: 50,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
            tiered_scout_allocator: 0,
            tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".into(),
            crews: vec![],
        };
        assert!(!entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            50,
            1,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
        assert!(entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            50,
            0,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
    }

    #[test]
    fn entry_confirm_cap_mismatch_invalidates() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 100,
            seed: 2,
            tiered_scout_sims: 500,
            tiered_top_k: 20,
            n_candidates: 50,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
            tiered_scout_allocator: 1,
            tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
            tiered_confirm_cap_mult: Some(2.5),
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".into(),
            crews: vec![],
        };
        assert!(!entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            50,
            1,
            "0",
            TIERED_BUDGET_POLICY_V2,
            None
        ));
        assert!(entry_matches_run(
            &entry,
            100,
            2,
            500,
            20,
            50,
            1,
            "0",
            TIERED_BUDGET_POLICY_V2,
            Some(2.5)
        ));
    }

    #[test]
    fn evict_drops_oldest() {
        let mut entries = HashMap::new();
        for i in 0..MAX_OPTIMIZE_CACHE_KEYS + 5 {
            entries.insert(
                format!("k{i}"),
                OptimizeHistoryEntry {
                    updated_at_ms: i as u128,
                    sims: 100,
                    seed: 0,
                    tiered_scout_sims: 500,
                    tiered_top_k: 20,
                    n_candidates: 100,
                    optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
                    tiered_scout_allocator: 0,
                    tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
                    tiered_confirm_cap_mult: None,
                    exhaustive_scout_sims: None,
                    exhaustive_scout_top_keep: None,
                    exhaustive_confirm_policy: 0,
                    chain_fingerprint: "0".to_string(),
                    crews: vec![],
                },
            );
        }
        evict_oldest_if_needed(&mut entries);
        assert!(entries.len() <= MAX_OPTIMIZE_CACHE_KEYS);
    }

    fn dummy_sim(candidate: CrewCandidate, win_rate: f64) -> SimulationResult {
        SimulationResult {
            candidate,
            trials_run: 100,
            win_rate,
            win_rate_ci_low: 0.0,
            win_rate_ci_high: 1.0,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: 0.0,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 0.0,
            r1_kill_rate: 0.0,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 0.0,
            avg_hull_remaining: 0.5,
            avg_hull_remaining_ci_low: 0.0,
            avg_hull_remaining_ci_high: 1.0,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            chain: None,
        }
    }

    #[test]
    fn prior_reference_crews_from_entry_orders_by_win_rate() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 100,
            seed: 2,
            tiered_scout_sims: 500,
            tiered_top_k: 20,
            n_candidates: 50,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
            tiered_scout_allocator: 1,
            tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".into(),
            crews: vec![
                OptimizeHistoryCrewRecord::from_simulation(&dummy_sim(
                    CrewCandidate {
                        captain: "LowWR".into(),
                        bridge: vec![],
                        below_decks: vec![],
                    },
                    0.2,
                )),
                OptimizeHistoryCrewRecord::from_simulation(&dummy_sim(
                    CrewCandidate {
                        captain: "HighWR".into(),
                        bridge: vec!["B1".into()],
                        below_decks: vec![],
                    },
                    0.95,
                )),
            ],
        };
        let priors = super::prior_reference_crews_from_entry(&entry, &None);
        assert_eq!(priors.len(), 2);
        assert_eq!(priors[0].captain, "HighWR");
        assert_eq!(priors[0].bridge, vec!["B1".to_string()]);
        assert_eq!(priors[1].captain, "LowWR");
    }

    #[test]
    fn prior_reference_crews_from_entry_chain_mismatch_empty() {
        let entry = OptimizeHistoryEntry {
            updated_at_ms: 1,
            sims: 100,
            seed: 2,
            tiered_scout_sims: 500,
            tiered_top_k: 20,
            n_candidates: 50,
            optimize_history_kind: OPTIMIZE_HISTORY_KIND_TIERED,
            tiered_scout_allocator: 1,
            tiered_budget_policy: TIERED_BUDGET_POLICY_V2,
            tiered_confirm_cap_mult: None,
            exhaustive_scout_sims: None,
            exhaustive_scout_top_keep: None,
            exhaustive_confirm_policy: 0,
            chain_fingerprint: "0".into(),
            crews: vec![OptimizeHistoryCrewRecord::from_simulation(&dummy_sim(
                CrewCandidate {
                    captain: "Solo".into(),
                    bridge: vec![],
                    below_decks: vec![],
                },
                0.9,
            ))],
        };
        let chain = Some(crate::optimizer::chain::ChainGrindParams {
            kills_target: 2,
            secondary: crate::optimizer::chain::ChainSecondaryObjective::MinHullDamage,
        });
        assert!(super::prior_reference_crews_from_entry(&entry, &chain).is_empty());
    }
}
