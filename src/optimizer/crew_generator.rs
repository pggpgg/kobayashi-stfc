#[derive(Debug, Clone, PartialEq)]
pub struct CrewCandidate {
    pub captain: String,
    pub bridge: String,
    pub below_decks: String,
}

#[derive(Debug, Clone)]
pub struct CrewGenerator;

impl CrewGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_candidates(&self, ship: &str, hostile: &str, seed: u64) -> Vec<CrewCandidate> {
        let officer_pool = [
            "Khan", "Nero", "T'Laan", "Pike", "Moreau", "Chen", "Kirk", "Spock", "Uhura",
        ];

        let offset = (ship
            .len()
            .wrapping_add(hostile.len())
            .wrapping_add(seed as usize % officer_pool.len()))
            % officer_pool.len();
        let mut candidates = Vec::with_capacity(4);

        for i in 0..4 {
            let captain = officer_pool[(offset + (i * 2)) % officer_pool.len()];
            let bridge = officer_pool[(offset + (i * 2) + 1) % officer_pool.len()];
            let below_decks = officer_pool[(offset + (i * 2) + 2) % officer_pool.len()];
            candidates.push(CrewCandidate {
                captain: captain.to_string(),
                bridge: bridge.to_string(),
                below_decks: below_decks.to_string(),
            });
        }

        candidates
    }
}
