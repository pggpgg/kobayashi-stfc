use crate::data::import::{load_imported_roster_ids, DEFAULT_IMPORT_OUTPUT_PATH};
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};

/// Number of bridge officer slots (in addition to captain). Players typically crew 1 captain + 2 bridge.
pub const BRIDGE_SLOTS: usize = 2;
/// Number of below-decks officer slots. Assumed fixed for now; will be configurable by ship level later.
pub const BELOW_DECKS_SLOTS: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct CrewCandidate {
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CandidateStrategy {
    pub exhaustive_pool_threshold: usize,
    pub max_candidates: usize,
    pub large_pool_captain_limit: usize,
    pub large_pool_bridge_limit: usize,
    pub use_seeded_shuffle: bool,
}

impl Default for CandidateStrategy {
    fn default() -> Self {
        Self {
            exhaustive_pool_threshold: 12,
            max_candidates: 128,
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
        let mut officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
            .map(|loaded| {
                loaded
                    .into_iter()
                    .filter(|officer| !officer.name.trim().is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Only filter by imported roster when we have enough officers for a full crew:
        // 1 captain + BRIDGE_SLOTS bridge + BELOW_DECKS_SLOTS below decks (all distinct).
        const MIN_OFFICERS: usize = 1 + BRIDGE_SLOTS + BELOW_DECKS_SLOTS;
        if let Some(roster_ids) = load_imported_roster_ids(DEFAULT_IMPORT_OUTPUT_PATH) {
            if roster_ids.len() >= MIN_OFFICERS {
                officers.retain(|officer| roster_ids.contains(&officer.id));
            }
        }

        if officers.is_empty() {
            return Vec::new();
        }

        let mut captains: Vec<&Officer> = officers
            .iter()
            .filter(|officer| is_captain_eligible(officer))
            .collect();
        let mut bridge: Vec<&Officer> = officers
            .iter()
            .filter(|officer| can_fill_position(officer, Position::Bridge))
            .collect();
        let mut below_decks: Vec<&Officer> = officers
            .iter()
            .filter(|officer| can_fill_position(officer, Position::BelowDecks))
            .collect();

        if captains.is_empty() {
            captains = officers.iter().collect();
        }
        if bridge.is_empty() {
            bridge = officers.iter().collect();
        }
        if below_decks.is_empty() {
            below_decks = officers.iter().collect();
        }

        // Need at least 1 captain, 2 bridge, 3 below decks to form any crew.
        if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < BELOW_DECKS_SLOTS {
            return Vec::new();
        }

        if self.strategy.use_seeded_shuffle {
            let base_seed = mix_seed(seed, ship, hostile);
            deterministic_shuffle(&mut captains, base_seed);
            deterministic_shuffle(&mut bridge, base_seed ^ 0x9E37_79B9_7F4A_7C15);
            deterministic_shuffle(&mut below_decks, base_seed ^ 0x517C_C1B7_2722_0A95);
        }

        let min_pool = captains
            .len()
            .min(bridge.len())
            .min(below_decks.len());
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_candidates(
                &captains,
                &bridge,
                &below_decks,
                self.strategy.max_candidates,
            )
        } else {
            sampled_candidates(
                &captains,
                &bridge,
                &below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
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
    captains: &[&Officer],
    bridge: &[&Officer],
    below_decks: &[&Officer],
    max_candidates: usize,
) -> Vec<CrewCandidate> {
    let mut candidates = Vec::new();

    for captain in captains {
        for (i, b1) in bridge.iter().enumerate() {
            if b1.id == captain.id {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if b2.id == captain.id || b2.id == b1.id {
                    continue;
                }
                let used: std::collections::HashSet<&str> =
                    [captain.id.as_str(), b1.id.as_str(), b2.id.as_str()].into_iter().collect();
                for (di, d1) in below_decks.iter().enumerate() {
                    if used.contains(d1.id.as_str()) {
                        continue;
                    }
                    let used2: std::collections::HashSet<&str> =
                        used.iter().copied().chain(std::iter::once(d1.id.as_str())).collect();
                    for (dj, d2) in below_decks.iter().enumerate().skip(di + 1) {
                        if used2.contains(d2.id.as_str()) {
                            continue;
                        }
                        let used3: std::collections::HashSet<&str> = used2
                            .iter()
                            .copied()
                            .chain(std::iter::once(d2.id.as_str()))
                            .collect();
                        for d3 in below_decks.iter().skip(dj + 1) {
                            if used3.contains(d3.id.as_str()) {
                                continue;
                            }
                            candidates.push(CrewCandidate {
                                captain: captain.name.clone(),
                                bridge: vec![b1.name.clone(), b2.name.clone()],
                                below_decks: vec![
                                    d1.name.clone(),
                                    d2.name.clone(),
                                    d3.name.clone(),
                                ],
                            });
                            if candidates.len() >= max_candidates {
                                return candidates;
                            }
                        }
                    }
                }
            }
        }
    }

    candidates
}

fn sampled_candidates(
    captains: &[&Officer],
    bridge: &[&Officer],
    below_decks: &[&Officer],
    strategy: &CandidateStrategy,
    seed: u64,
) -> Vec<CrewCandidate> {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let mut candidates = Vec::new();
    let stride = ((seed as usize) % 5) + 1;

    for captain in captains.iter().take(captain_limit) {
        for (bi, b1) in bridge.iter().take(bridge_limit).enumerate() {
            if b1.id == captain.id {
                continue;
            }
            for b2 in bridge.iter().take(bridge_limit).skip(bi + 1) {
                if b2.id == captain.id || b2.id == b1.id {
                    continue;
                }
            let used: std::collections::HashSet<&str> =
                [captain.id.as_str(), b1.id.as_str(), b2.id.as_str()].into_iter().collect();
            let below_indices: Vec<usize> = (0..below_decks.len())
                .step_by(stride)
                .filter(|&i| !used.contains(below_decks[i].id.as_str()))
                .collect();
            for (ii, &di) in below_indices.iter().enumerate() {
                let d1 = below_decks[di];
                let used2: std::collections::HashSet<&str> =
                    used.iter().copied().chain(std::iter::once(d1.id.as_str())).collect();
                for &dj in below_indices.iter().skip(ii + 1) {
                    let d2 = below_decks[dj];
                    if used2.contains(d2.id.as_str()) {
                        continue;
                    }
                    let used3: std::collections::HashSet<&str> = used2
                        .iter()
                        .copied()
                        .chain(std::iter::once(d2.id.as_str()))
                        .collect();
                    for &dk in below_indices.iter().skip(ii + 2) {
                        let d3 = below_decks[dk];
                        if used3.contains(d3.id.as_str()) {
                            continue;
                        }
                        candidates.push(CrewCandidate {
                            captain: captain.name.clone(),
                            bridge: vec![b1.name.clone(), b2.name.clone()],
                            below_decks: vec![
                                d1.name.clone(),
                                d2.name.clone(),
                                d3.name.clone(),
                            ],
                        });
                        if candidates.len() >= strategy.max_candidates {
                            return candidates;
                        }
                    }
                }
            }
            }
        }
    }

    candidates
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
            max_candidates: 32,
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
            max_candidates: 24,
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
