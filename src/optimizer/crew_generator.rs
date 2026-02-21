use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};

#[derive(Debug, Clone, PartialEq)]
pub struct CrewCandidate {
    pub captain: String,
    pub bridge: String,
    pub below_decks: String,
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
        let officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
            .map(|loaded| {
                loaded
                    .into_iter()
                    .filter(|officer| !officer.name.trim().is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

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

        if self.strategy.use_seeded_shuffle {
            let base_seed = mix_seed(seed, ship, hostile);
            deterministic_shuffle(&mut captains, base_seed);
            deterministic_shuffle(&mut bridge, base_seed ^ 0x9E37_79B9_7F4A_7C15);
            deterministic_shuffle(&mut below_decks, base_seed ^ 0x517C_C1B7_2722_0A95);
        }

        let min_pool = captains.len().min(bridge.len()).min(below_decks.len());
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
        for bridge_officer in bridge {
            if captain.id == bridge_officer.id {
                continue;
            }
            for below_officer in below_decks {
                if captain.id == below_officer.id || bridge_officer.id == below_officer.id {
                    continue;
                }

                candidates.push(CrewCandidate {
                    captain: captain.name.clone(),
                    bridge: bridge_officer.name.clone(),
                    below_decks: below_officer.name.clone(),
                });

                if candidates.len() >= max_candidates {
                    return candidates;
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
    let bridge_limit = strategy.large_pool_bridge_limit.max(1).min(bridge.len());
    let mut candidates = Vec::new();

    for captain in captains.iter().take(captain_limit) {
        let stride = ((seed as usize) % 5) + 1;

        for (bridge_index, bridge_officer) in bridge.iter().take(bridge_limit).enumerate() {
            if bridge_index % stride != 0 || captain.id == bridge_officer.id {
                continue;
            }

            for below_index in (0..below_decks.len()).step_by(stride) {
                let below = below_decks[below_index];
                if below.id == captain.id || below.id == bridge_officer.id {
                    continue;
                }

                candidates.push(CrewCandidate {
                    captain: captain.name.clone(),
                    bridge: bridge_officer.name.clone(),
                    below_decks: below.name.clone(),
                });

                if candidates.len() >= strategy.max_candidates {
                    return candidates;
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
