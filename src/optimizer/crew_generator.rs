use std::collections::HashSet;

use crate::data::data_registry::DataRegistry;
use crate::data::import::load_imported_roster_ids_unlocked_only;
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, ROSTER_IMPORTED};
use crate::perf_log;

/// Number of bridge officer slots (in addition to captain). Players typically crew 1 captain + 2 bridge.
pub const BRIDGE_SLOTS: usize = 2;
/// Default below-decks slots when not overridden (matches mid/high-tier STFC ships).
pub const DEFAULT_BELOW_DECKS_SLOTS: usize = 3;
/// Backwards-compatible alias for [`DEFAULT_BELOW_DECKS_SLOTS`].
pub const BELOW_DECKS_SLOTS: usize = DEFAULT_BELOW_DECKS_SLOTS;
pub const MIN_BELOW_DECKS_SLOTS: usize = 2;
pub const MAX_BELOW_DECKS_SLOTS: usize = 5;

/// Tier-aware default: early ships often have 2 below-decks slots; tier 2+ uses 3 in this heuristic.
pub fn default_below_decks_slots_for_tier(ship_tier: Option<u32>) -> usize {
    match ship_tier {
        Some(1) => MIN_BELOW_DECKS_SLOTS,
        _ => DEFAULT_BELOW_DECKS_SLOTS,
    }
}

/// Resolve slot count from explicit API value or ship tier default.
pub fn resolve_below_decks_slots(ship_tier: Option<u32>, explicit: Option<u32>) -> usize {
    if let Some(n) = explicit {
        let n = n as usize;
        return n.clamp(MIN_BELOW_DECKS_SLOTS, MAX_BELOW_DECKS_SLOTS);
    }
    default_below_decks_slots_for_tier(ship_tier)
}

/// Officer pools by slot, as names. Shared by crew generator and genetic optimizer.
#[derive(Debug, Clone)]
pub struct OfficerPools {
    pub captains: Vec<String>,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

/// True if the officer has at least one ability with slot "below_decks".
fn has_below_decks_ability(officer: &Officer) -> bool {
    officer
        .abilities
        .iter()
        .any(|a| a.slot.eq_ignore_ascii_case("below_decks"))
}

/// Builds officer pools from registry (no officer reload). Still loads roster for filter.
pub fn build_officer_pools_from_registry(
    registry: &DataRegistry,
    only_below_decks_with_ability: bool,
    profile_id: Option<&str>,
    below_decks_slots: usize,
) -> Option<OfficerPools> {
    let officers: Vec<Officer> = registry
        .officers()
        .iter()
        .filter(|o| !o.name.trim().is_empty())
        .cloned()
        .collect();

    let min_officers = 1 + BRIDGE_SLOTS + below_decks_slots;
    let mut officers = officers;
    let roster_path = profile_path(&resolve_profile_id_for_api(profile_id), ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
        if roster_ids.len() >= min_officers {
            officers.retain(|officer| roster_ids.contains(&officer.id));
        }
    }

    if officers.is_empty() {
        return None;
    }

    let mut captains: Vec<String> = officers
        .iter()
        .filter(|officer| is_captain_eligible(officer))
        .map(|o| o.name.clone())
        .collect();
    let mut bridge: Vec<String> = officers
        .iter()
        .filter(|officer| can_fill_position(officer, Position::Bridge))
        .map(|o| o.name.clone())
        .collect();
    let mut below_decks: Vec<String> = officers
        .iter()
        .filter(|officer| can_fill_position(officer, Position::BelowDecks))
        .map(|o| o.name.clone())
        .collect();

    if only_below_decks_with_ability {
        below_decks = officers
            .iter()
            .filter(|officer| {
                can_fill_position(officer, Position::BelowDecks) && has_below_decks_ability(officer)
            })
            .map(|o| o.name.clone())
            .collect();
    } else if below_decks.is_empty() {
        below_decks = officers.iter().map(|o| o.name.clone()).collect();
    }

    if captains.is_empty() {
        captains = officers.iter().map(|o| o.name.clone()).collect();
    }
    if bridge.is_empty() {
        bridge = officers.iter().map(|o| o.name.clone()).collect();
    }

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < below_decks_slots {
        return None;
    }

    Some(OfficerPools {
        captains,
        bridge,
        below_decks,
    })
}

/// Builds captain, bridge, and below-decks pools from loaded officers and roster filter.
/// When `only_below_decks_with_ability` is true, the below-decks pool is restricted to officers
/// that have a below-decks ability; no fallback to all officers is applied in that case.
/// Returns `None` if there are not enough officers to form any valid crew.
pub fn build_officer_pools(
    only_below_decks_with_ability: bool,
    below_decks_slots: usize,
) -> Option<OfficerPools> {
    let mut officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .map(|loaded| {
            loaded
                .into_iter()
                .filter(|officer| !officer.name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let min_officers = 1 + BRIDGE_SLOTS + below_decks_slots;
    let roster_path = profile_path(&resolve_profile_id_for_api(None), ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
        if roster_ids.len() >= min_officers {
            officers.retain(|officer| roster_ids.contains(&officer.id));
        }
    }

    if officers.is_empty() {
        return None;
    }

    let mut captains: Vec<String> = officers
        .iter()
        .filter(|officer| is_captain_eligible(officer))
        .map(|o| o.name.clone())
        .collect();
    let mut bridge: Vec<String> = officers
        .iter()
        .filter(|officer| can_fill_position(officer, Position::Bridge))
        .map(|o| o.name.clone())
        .collect();
    let mut below_decks: Vec<String> = officers
        .iter()
        .filter(|officer| can_fill_position(officer, Position::BelowDecks))
        .map(|o| o.name.clone())
        .collect();

    if only_below_decks_with_ability {
        below_decks = officers
            .iter()
            .filter(|officer| {
                can_fill_position(officer, Position::BelowDecks) && has_below_decks_ability(officer)
            })
            .map(|o| o.name.clone())
            .collect();
        // Do not fallback to all officers when user requested this filter.
    } else if below_decks.is_empty() {
        below_decks = officers.iter().map(|o| o.name.clone()).collect();
    }

    if captains.is_empty() {
        captains = officers.iter().map(|o| o.name.clone()).collect();
    }
    if bridge.is_empty() {
        bridge = officers.iter().map(|o| o.name.clone()).collect();
    }

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < below_decks_slots {
        return None;
    }

    Some(OfficerPools {
        captains,
        bridge,
        below_decks,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrewCandidate {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CandidateStrategy {
    pub exhaustive_pool_threshold: usize,
    /// When Some(n), generation stops after n candidates. When None, all combinations are generated.
    pub max_candidates: Option<usize>,
    pub large_pool_captain_limit: usize,
    pub large_pool_bridge_limit: usize,
    pub use_seeded_shuffle: bool,
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub only_below_decks_with_ability: bool,
    /// Number of below-decks slots per generated crew (2–5).
    pub below_decks_slots: usize,
}

impl Default for CandidateStrategy {
    fn default() -> Self {
        Self {
            exhaustive_pool_threshold: 12,
            max_candidates: Some(128),
            large_pool_captain_limit: 10,
            large_pool_bridge_limit: 12,
            use_seeded_shuffle: true,
            only_below_decks_with_ability: false,
            below_decks_slots: DEFAULT_BELOW_DECKS_SLOTS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrewGenerator {
    strategy: CandidateStrategy,
}

impl CrewGenerator {
    pub fn new() -> Self {
        Self {
            strategy: CandidateStrategy::default(),
        }
    }

    pub fn with_strategy(strategy: CandidateStrategy) -> Self {
        Self { strategy }
    }

    pub fn generate_candidates(&self, ship: &str, hostile: &str, seed: u64) -> Vec<CrewCandidate> {
        let mut pools = match build_officer_pools(
            self.strategy.only_below_decks_with_ability,
            self.strategy.below_decks_slots,
        ) {
            Some(p) => p,
            None => return Vec::new(),
        };
        self.generate_candidates_from_pools(&mut pools, ship, hostile, seed)
    }

    /// Like [generate_candidates] but uses registry for officers (no reload).
    pub fn generate_candidates_from_registry(
        &self,
        registry: &DataRegistry,
        ship: &str,
        hostile: &str,
        seed: u64,
        profile_id: Option<&str>,
    ) -> Vec<CrewCandidate> {
        let mut pools = match build_officer_pools_from_registry(
            registry,
            self.strategy.only_below_decks_with_ability,
            profile_id,
            self.strategy.below_decks_slots,
        ) {
            Some(p) => p,
            None => return Vec::new(),
        };
        self.generate_candidates_from_pools(&mut pools, ship, hostile, seed)
    }

    fn generate_candidates_from_pools(
        &self,
        pools: &mut OfficerPools,
        ship: &str,
        hostile: &str,
        seed: u64,
    ) -> Vec<CrewCandidate> {
        let t0 = perf_log::perf_start();
        if self.strategy.use_seeded_shuffle {
            let base_seed = mix_seed(seed, ship, hostile);
            deterministic_shuffle(&mut pools.captains, base_seed);
            deterministic_shuffle(&mut pools.bridge, base_seed ^ 0x9E37_79B9_7F4A_7C15);
            deterministic_shuffle(&mut pools.below_decks, base_seed ^ 0x517C_C1B7_2722_0A95);
        }

        let min_pool = pools
            .captains
            .len()
            .min(pools.bridge.len())
            .min(pools.below_decks.len());
        let k = self.strategy.below_decks_slots;
        let out = if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                self.strategy.max_candidates,
                k,
            )
        } else {
            sampled_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                k,
            )
        };
        perf_log::log_duration("crew_generator.generate_candidates_from_pools", t0);
        out
    }

    /// Returns the number of crew combinations without allocating candidates.
    /// Used for estimate when no cap is set. Uses same exhaustive/sampled branch as generate_candidates.
    pub fn count_candidates(&self, ship: &str, hostile: &str, seed: u64) -> usize {
        let mut pools = match build_officer_pools(
            self.strategy.only_below_decks_with_ability,
            self.strategy.below_decks_slots,
        ) {
            Some(p) => p,
            None => return 0,
        };
        self.count_candidates_from_pools(&mut pools, ship, hostile, seed)
    }

    /// Like [count_candidates] but uses registry for officers (no reload).
    pub fn count_candidates_from_registry(
        &self,
        registry: &DataRegistry,
        ship: &str,
        hostile: &str,
        seed: u64,
        profile_id: Option<&str>,
    ) -> usize {
        let mut pools = match build_officer_pools_from_registry(
            registry,
            self.strategy.only_below_decks_with_ability,
            profile_id,
            self.strategy.below_decks_slots,
        ) {
            Some(p) => p,
            None => return 0,
        };
        self.count_candidates_from_pools(&mut pools, ship, hostile, seed)
    }

    fn count_candidates_from_pools(
        &self,
        pools: &mut OfficerPools,
        ship: &str,
        hostile: &str,
        seed: u64,
    ) -> usize {
        if self.strategy.use_seeded_shuffle {
            let base_seed = mix_seed(seed, ship, hostile);
            deterministic_shuffle(&mut pools.captains, base_seed);
            deterministic_shuffle(&mut pools.bridge, base_seed ^ 0x9E37_79B9_7F4A_7C15);
            deterministic_shuffle(&mut pools.below_decks, base_seed ^ 0x517C_C1B7_2722_0A95);
        }

        let min_pool = pools
            .captains
            .len()
            .min(pools.bridge.len())
            .min(pools.below_decks.len());
        let k = self.strategy.below_decks_slots;
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_count(&pools.captains, &pools.bridge, &pools.below_decks, None, k)
        } else {
            sampled_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                None,
                k,
            )
        }
    }
}

#[derive(Copy, Clone)]
enum Position {
    Bridge,
    BelowDecks,
}

fn is_captain_eligible(officer: &Officer) -> bool {
    officer
        .abilities
        .iter()
        .any(|ability| ability.slot == "captain")
}

/// True if `name` equals captain or any bridge officer (distinct-officer checks).
#[inline]
fn name_conflicts_bridge_captain(name: &str, captain: &str, b1: &str, b2: &str) -> bool {
    name == captain || name == b1 || name == b2
}

#[inline]
fn below_tuple_ok(names: &[String], captain: &str, b1: &str, b2: &str) -> bool {
    let mut seen = HashSet::with_capacity(names.len());
    for n in names {
        if name_conflicts_bridge_captain(n, captain, b1, b2) || !seen.insert(n.as_str()) {
            return false;
        }
    }
    true
}

/// All k-combinations of indices in `0..n` (strictly increasing index tuples).
fn for_each_combination_indices(n: usize, k: usize, mut visit: impl FnMut(&[usize])) {
    if k == 0 {
        visit(&[]);
        return;
    }
    if k > n {
        return;
    }
    let mut cur = Vec::with_capacity(k);
    fn rec(
        n: usize,
        k: usize,
        start: usize,
        cur: &mut Vec<usize>,
        visit: &mut impl FnMut(&[usize]),
    ) {
        if cur.len() == k {
            visit(cur);
            return;
        }
        for i in start..n {
            cur.push(i);
            rec(n, k, i + 1, cur, visit);
            cur.pop();
        }
    }
    rec(n, k, 0, &mut cur, &mut visit);
}

fn can_fill_position(officer: &Officer, position: Position) -> bool {
    let Some(slot) = officer.slot.as_deref() else {
        return true;
    };

    match slot.to_ascii_lowercase().as_str() {
        "captain" => matches!(position, Position::Bridge),
        "bridge" | "officer" => matches!(position, Position::Bridge),
        "below_decks" => matches!(position, Position::BelowDecks),
        _ => true,
    }
}

fn exhaustive_candidates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    max_candidates: Option<usize>,
    below_decks_slots: usize,
) -> Vec<CrewCandidate> {
    let reserve = max_candidates.unwrap_or(256).min(4096);
    let mut candidates = Vec::with_capacity(reserve);
    let n_bd = below_decks.len();
    if below_decks_slots > n_bd {
        return candidates;
    }

    let mut stop = false;
    for captain in captains {
        if stop {
            break;
        }
        for (i, b1) in bridge.iter().enumerate() {
            if stop {
                break;
            }
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if stop {
                    break;
                }
                if b2 == captain || b2 == b1 {
                    continue;
                }
                for_each_combination_indices(n_bd, below_decks_slots, |idxs| {
                    if stop {
                        return;
                    }
                    let bd: Vec<String> = idxs.iter().map(|&i| below_decks[i].clone()).collect();
                    if !below_tuple_ok(&bd, captain, b1, b2) {
                        return;
                    }
                    candidates.push(CrewCandidate {
                        captain: captain.clone(),
                        bridge: vec![b1.clone(), b2.clone()],
                        below_decks: bd,
                    });
                    if max_candidates.is_some_and(|c| candidates.len() >= c) {
                        stop = true;
                    }
                });
            }
        }
    }

    candidates
}

fn exhaustive_count(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    max_count: Option<usize>,
    below_decks_slots: usize,
) -> usize {
    const ESTIMATE_CAP: usize = 2_000_000;
    let mut count = 0_usize;
    let n_bd = below_decks.len();
    if below_decks_slots > n_bd {
        return 0;
    }

    for captain in captains {
        for (i, b1) in bridge.iter().enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                for_each_combination_indices(n_bd, below_decks_slots, |idxs| {
                    let bd: Vec<String> = idxs.iter().map(|&i| below_decks[i].clone()).collect();
                    if !below_tuple_ok(&bd, captain, b1, b2) {
                        return;
                    }
                    count += 1;
                });
                if let Some(cap) = max_count {
                    if count >= cap {
                        return count;
                    }
                }
                if count >= ESTIMATE_CAP {
                    return ESTIMATE_CAP;
                }
            }
        }
    }

    count
}

fn sampled_candidates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    strategy: &CandidateStrategy,
    seed: u64,
    below_decks_slots: usize,
) -> Vec<CrewCandidate> {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let reserve = strategy.max_candidates.unwrap_or(256).min(4096);
    let mut candidates = Vec::with_capacity(reserve);
    let stride = ((seed as usize) % 5) + 1;

    for captain in captains.iter().take(captain_limit) {
        for (bi, b1) in bridge.iter().take(bridge_limit).enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().take(bridge_limit).skip(bi + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                let below_indices: Vec<usize> = (0..below_decks.len())
                    .step_by(stride)
                    .filter(|&i| {
                        !name_conflicts_bridge_captain(below_decks[i].as_str(), captain, b1, b2)
                    })
                    .collect();
                let m = below_indices.len();
                if below_decks_slots > m {
                    continue;
                }
                let mut stop = false;
                for_each_combination_indices(m, below_decks_slots, |pos| {
                    if stop {
                        return;
                    }
                    let bd: Vec<String> = pos
                        .iter()
                        .map(|&pi| below_decks[below_indices[pi]].clone())
                        .collect();
                    if !below_tuple_ok(&bd, captain, b1, b2) {
                        return;
                    }
                    candidates.push(CrewCandidate {
                        captain: captain.clone(),
                        bridge: vec![b1.clone(), b2.clone()],
                        below_decks: bd,
                    });
                    if strategy
                        .max_candidates
                        .is_some_and(|c| candidates.len() >= c)
                    {
                        stop = true;
                    }
                });
                if stop {
                    return candidates;
                }
            }
        }
    }

    candidates
}

fn sampled_count(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    strategy: &CandidateStrategy,
    seed: u64,
    max_count: Option<usize>,
    below_decks_slots: usize,
) -> usize {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let mut count = 0_usize;
    let stride = ((seed as usize) % 5) + 1;
    const ESTIMATE_CAP: usize = 2_000_000;

    for captain in captains.iter().take(captain_limit) {
        for (bi, b1) in bridge.iter().take(bridge_limit).enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().take(bridge_limit).skip(bi + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                let below_indices: Vec<usize> = (0..below_decks.len())
                    .step_by(stride)
                    .filter(|&i| {
                        !name_conflicts_bridge_captain(below_decks[i].as_str(), captain, b1, b2)
                    })
                    .collect();
                let m = below_indices.len();
                if below_decks_slots > m {
                    continue;
                }
                for_each_combination_indices(m, below_decks_slots, |pos| {
                    let bd: Vec<String> = pos
                        .iter()
                        .map(|&pi| below_decks[below_indices[pi]].clone())
                        .collect();
                    if !below_tuple_ok(&bd, captain, b1, b2) {
                        return;
                    }
                    count += 1;
                });
                if let Some(cap) = max_count {
                    if count >= cap {
                        return count;
                    }
                }
                if count >= ESTIMATE_CAP {
                    return ESTIMATE_CAP;
                }
            }
        }
    }

    count
}

fn deterministic_shuffle<T>(items: &mut [T], seed: u64) {
    if items.len() < 2 {
        return;
    }

    let mut state = seed;
    for index in (1..items.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let swap_index = (state as usize) % (index + 1);
        items.swap(index, swap_index);
    }
}

fn mix_seed(seed: u64, ship: &str, hostile: &str) -> u64 {
    let mut value = seed ^ 0x9E37_79B9_7F4A_7C15;
    for byte in ship.bytes().chain(hostile.bytes()) {
        value ^= u64::from(byte)
            .wrapping_add(0x9E37_79B9)
            .wrapping_add(value << 6)
            .wrapping_add(value >> 2);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_below_decks_slots, CandidateStrategy, CrewGenerator, MAX_BELOW_DECKS_SLOTS,
        MIN_BELOW_DECKS_SLOTS,
    };

    #[test]
    fn resolve_below_decks_uses_explicit_or_tier_default() {
        assert_eq!(resolve_below_decks_slots(None, Some(4)), 4);
        assert_eq!(
            resolve_below_decks_slots(Some(1), None),
            MIN_BELOW_DECKS_SLOTS
        );
        assert_eq!(resolve_below_decks_slots(Some(2), None), 3);
        assert_eq!(resolve_below_decks_slots(None, None), 3);
        assert_eq!(
            resolve_below_decks_slots(None, Some(99)),
            MAX_BELOW_DECKS_SLOTS
        );
        assert_eq!(
            resolve_below_decks_slots(None, Some(1)),
            MIN_BELOW_DECKS_SLOTS
        );
    }

    #[test]
    fn generation_is_deterministic_for_same_seed() {
        let generator = CrewGenerator::with_strategy(CandidateStrategy {
            max_candidates: Some(32),
            ..CandidateStrategy::default()
        });

        let first = generator.generate_candidates("enterprise", "swarm", 7);
        let second = generator.generate_candidates("enterprise", "swarm", 7);

        assert_eq!(first, second);
    }

    #[test]
    fn generation_produces_minimum_candidate_breadth() {
        let generator = CrewGenerator::with_strategy(CandidateStrategy {
            exhaustive_pool_threshold: 8,
            max_candidates: Some(24),
            large_pool_captain_limit: 5,
            large_pool_bridge_limit: 6,
            ..CandidateStrategy::default()
        });

        let candidates = generator.generate_candidates("defiant", "romulan", 11);
        assert!(
            candidates.len() >= 10,
            "expected at least 10 candidates, got {}",
            candidates.len()
        );
    }

    #[test]
    fn generation_respects_below_decks_slot_count() {
        let generator = CrewGenerator::with_strategy(CandidateStrategy {
            below_decks_slots: 2,
            max_candidates: Some(24),
            ..CandidateStrategy::default()
        });
        let candidates = generator.generate_candidates("defiant", "romulan", 11);
        assert!(!candidates.is_empty());
        for c in &candidates {
            assert_eq!(c.below_decks.len(), 2, "{c:?}");
        }
    }
}
