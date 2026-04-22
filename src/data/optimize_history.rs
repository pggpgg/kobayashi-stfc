//! Per-profile optimize result cache (`profiles/{id}/optimize_history.json`).
//!
//! Keys are opaque client fingerprints (`optimize_cache_key`). Entries store resolved tiered
//! parameters plus full Monte Carlo aggregates so a later tiered run can skip scout/confirm
//! for matching crews when metadata still aligns.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::data::profile_index::{profile_path, OPTIMIZE_HISTORY_JSON};
use crate::optimizer::chain::ChainGrindParams;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::{crew_candidate_stable_hash, SimulationResult};
use crate::optimizer::ranking::RankedCrewResult;

pub const OPTIMIZE_HISTORY_SCHEMA: u32 = 1;
pub const MAX_OPTIMIZE_CACHE_KEYS: usize = 200;
pub const MAX_OPTIMIZE_HISTORY_CREWS: usize = 24;
pub const MAX_OPTIMIZE_CACHE_KEY_BYTES: usize = 512;

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
        }
    }

    pub fn to_simulation_result(&self) -> SimulationResult {
        SimulationResult {
            candidate: CrewCandidate {
                captain: self.captain.clone(),
                bridge: self.bridge.clone(),
                below_decks: self.below_decks.clone(),
            },
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
    let mut file: OptimizeHistoryFile = serde_json::from_str(&raw).unwrap_or_else(|_| {
        OptimizeHistoryFile {
            schema: OPTIMIZE_HISTORY_SCHEMA,
            entries: HashMap::new(),
        }
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

pub fn entry_matches_run(
    entry: &OptimizeHistoryEntry,
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    chain_fp: &str,
) -> bool {
    entry.sims == sims
        && entry.seed == seed
        && entry.tiered_scout_sims == tiered_scout_sims
        && entry.tiered_top_k == tiered_top_k
        && entry.n_candidates == n_candidates
        && entry.chain_fingerprint == chain_fp
}

/// Build preconfirmed map when file entry matches this run's tiered metadata.
pub fn preconfirmed_for_candidates(
    profile_id: &str,
    cache_key: &str,
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    chain: &Option<ChainGrindParams>,
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
        &chain_fp,
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
    }
}

pub fn build_entry_from_ranked(
    sims: u32,
    seed: u64,
    tiered_scout_sims: usize,
    tiered_top_k: usize,
    n_candidates: usize,
    chain: &Option<ChainGrindParams>,
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
        chain_fingerprint: chain_fingerprint(chain),
        crews,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            chain_fingerprint: "0".into(),
            crews: vec![],
        };
        assert!(!entry_matches_run(
            &entry, 100, 2, 500, 20, 49, "0"
        ));
        assert!(entry_matches_run(
            &entry, 100, 2, 500, 20, 50, "0"
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
                    chain_fingerprint: "0".to_string(),
                    crews: vec![],
                },
            );
        }
        evict_oldest_if_needed(&mut entries);
        assert!(entries.len() <= MAX_OPTIMIZE_CACHE_KEYS);
    }
}
