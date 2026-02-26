use crate::data::import::{load_imported_roster_ids_unlocked_only, DEFAULT_IMPORT_OUTPUT_PATH};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};

/// Number of bridge officer slots (in addition to captain). Players typically crew 1 captain + 2 bridge.
pub const BRIDGE_SLOTS: usize = 2;
/// Number of below-decks officer slots. Assumed fixed for now; will be configurable by ship level later.
pub const BELOW_DECKS_SLOTS: usize = 3;

/// Officer pools by slot, as names. Shared by crew generator and genetic optimizer.
#[derive(Debug, Clone)]
pub struct OfficerPools {
    pub captains: Vec<String>,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

/// Builds captain, bridge, and below-decks pools from loaded officers and roster filter.
/// Returns `None` if there are not enough officers to form any valid crew.
pub fn build_officer_pools() -> Option<OfficerPools> {
    let mut officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .map(|loaded| {
            loaded
                .into_iter()
                .filter(|officer| !officer.name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    const MIN_OFFICERS: usize = 1 + BRIDGE_SLOTS + BELOW_DECKS_SLOTS;
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(DEFAULT_IMPORT_OUTPUT_PATH) {
        if roster_ids.len() >= MIN_OFFICERS {
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

    if captains.is_empty() {
        captains = officers.iter().map(|o| o.name.clone()).collect();
    }
    if bridge.is_empty() {
        bridge = officers.iter().map(|o| o.name.clone()).collect();
    }
    if below_decks.is_empty() {
        below_decks = officers.iter().map(|o| o.name.clone()).collect();
    }

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < BELOW_DECKS_SLOTS {
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
}

impl Default for CandidateStrategy {
    fn default() -> Self {
        Self {
            exhaustive_pool_threshold: 12,
            max_candidates: Some(128),
            large_pool_captain_limit: 10,
            large_pool_bridge_limit: 12,
            use_seeded_shuffle: true,
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
        let mut pools = match build_officer_pools() {
            Some(p) => p,
            None => return Vec::new(),
        };

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
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                self.strategy.max_candidates,
            )
        } else {
            sampled_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
            )
        }
    }

    /// Returns the number of crew combinations without allocating candidates.
    /// Used for estimate when no cap is set. Uses same exhaustive/sampled branch as generate_candidates.
    pub fn count_candidates(&self, ship: &str, hostile: &str, seed: u64) -> usize {
        let mut pools = match build_officer_pools() {
            Some(p) => p,
            None => return 0,
        };

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
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                None,
            )
        } else {
            sampled_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                None,
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
) -> Vec<CrewCandidate> {
    let mut candidates = Vec::new();

    for captain in captains {
        for (i, b1) in bridge.iter().enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                let used: std::collections::HashSet<&str> =
                    [captain.as_str(), b1.as_str(), b2.as_str()].into_iter().collect();
                for (di, d1) in below_decks.iter().enumerate() {
                    if used.contains(d1.as_str()) {
                        continue;
                    }
                    let used2: std::collections::HashSet<&str> =
                        used.iter().copied().chain(std::iter::once(d1.as_str())).collect();
                    for (dj, d2) in below_decks.iter().enumerate().skip(di + 1) {
                        if used2.contains(d2.as_str()) {
                            continue;
                        }
                        let used3: std::collections::HashSet<&str> = used2
                            .iter()
                            .copied()
                            .chain(std::iter::once(d2.as_str()))
                            .collect();
                        for d3 in below_decks.iter().skip(dj + 1) {
                            if used3.contains(d3.as_str()) {
                                continue;
                            }
                            candidates.push(CrewCandidate {
                                captain: captain.clone(),
                                bridge: vec![b1.clone(), b2.clone()],
                                below_decks: vec![d1.clone(), d2.clone(), d3.clone()],
                            });
                            if let Some(cap) = max_candidates {
                                if candidates.len() >= cap {
                                    return candidates;
                                }
                            }
                        }
                    }
                }
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
) -> usize {
    const ESTIMATE_CAP: usize = 2_000_000;
    let mut count = 0_usize;

    for captain in captains {
        for (i, b1) in bridge.iter().enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                let used: std::collections::HashSet<&str> =
                    [captain.as_str(), b1.as_str(), b2.as_str()].into_iter().collect();
                for (di, d1) in below_decks.iter().enumerate() {
                    if used.contains(d1.as_str()) {
                        continue;
                    }
                    let used2: std::collections::HashSet<&str> =
                        used.iter().copied().chain(std::iter::once(d1.as_str())).collect();
                    for (dj, d2) in below_decks.iter().enumerate().skip(di + 1) {
                        if used2.contains(d2.as_str()) {
                            continue;
                        }
                        let used3: std::collections::HashSet<&str> = used2
                            .iter()
                            .copied()
                            .chain(std::iter::once(d2.as_str()))
                            .collect();
                        for d3 in below_decks.iter().skip(dj + 1) {
                            if used3.contains(d3.as_str()) {
                                continue;
                            }
                            count += 1;
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
) -> Vec<CrewCandidate> {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let mut candidates = Vec::new();
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
                let used: std::collections::HashSet<&str> =
                    [captain.as_str(), b1.as_str(), b2.as_str()].into_iter().collect();
                let below_indices: Vec<usize> = (0..below_decks.len())
                    .step_by(stride)
                    .filter(|&i| !used.contains(below_decks[i].as_str()))
                    .collect();
                for (ii, &di) in below_indices.iter().enumerate() {
                    let d1 = &below_decks[di];
                    let used2: std::collections::HashSet<&str> =
                        used.iter().copied().chain(std::iter::once(d1.as_str())).collect();
                    for &dj in below_indices.iter().skip(ii + 1) {
                        let d2 = &below_decks[dj];
                        if used2.contains(d2.as_str()) {
                            continue;
                        }
                        let used3: std::collections::HashSet<&str> = used2
                            .iter()
                            .copied()
                            .chain(std::iter::once(d2.as_str()))
                            .collect();
                        for &dk in below_indices.iter().skip(ii + 2) {
                            let d3 = &below_decks[dk];
                            if used3.contains(d3.as_str()) {
                                continue;
                            }
                            candidates.push(CrewCandidate {
                                captain: captain.clone(),
                                bridge: vec![b1.clone(), b2.clone()],
                                below_decks: vec![d1.clone(), d2.clone(), d3.clone()],
                            });
                            if let Some(cap) = strategy.max_candidates {
                                if candidates.len() >= cap {
                                    return candidates;
                                }
                            }
                        }
                    }
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
                let used: std::collections::HashSet<&str> =
                    [captain.as_str(), b1.as_str(), b2.as_str()].into_iter().collect();
                let below_indices: Vec<usize> = (0..below_decks.len())
                    .step_by(stride)
                    .filter(|&i| !used.contains(below_decks[i].as_str()))
                    .collect();
                for (ii, &di) in below_indices.iter().enumerate() {
                    let d1 = &below_decks[di];
                    let used2: std::collections::HashSet<&str> =
                        used.iter().copied().chain(std::iter::once(d1.as_str())).collect();
                    for &dj in below_indices.iter().skip(ii + 1) {
                        let d2 = &below_decks[dj];
                        if used2.contains(d2.as_str()) {
                            continue;
                        }
                        let used3: std::collections::HashSet<&str> = used2
                            .iter()
                            .copied()
                            .chain(std::iter::once(d2.as_str()))
                            .collect();
                        for &dk in below_indices.iter().skip(ii + 2) {
                            let d3 = &below_decks[dk];
                            if used3.contains(d3.as_str()) {
                                continue;
                            }
                            count += 1;
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
    use super::{CandidateStrategy, CrewGenerator};

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
}
