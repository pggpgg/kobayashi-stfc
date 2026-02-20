use crate::optimizer::crew_generator::CrewCandidate;

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub candidate: CrewCandidate,
    pub win_rate: f64,
    pub avg_hull_remaining: f64,
}

pub fn run_monte_carlo(
    ship: &str,
    hostile: &str,
    candidates: &[CrewCandidate],
    iterations: usize,
    seed: u64,
) -> Vec<SimulationResult> {
    candidates
        .iter()
        .cloned()
        .map(|candidate| {
            let seed = stable_seed(
                ship,
                hostile,
                &candidate.captain,
                &candidate.bridge,
                &candidate.below_decks,
                seed,
            );
            let iter_factor = ((iterations.max(1) as f64).ln() / 10.0).min(0.15);

            let base_win = 0.45 + (seed % 500) as f64 / 1000.0;
            let win_rate = (base_win + iter_factor).clamp(0.0, 0.99);

            let base_hull = 0.20 + ((seed / 10) % 500) as f64 / 1000.0;
            let avg_hull_remaining = (base_hull + (win_rate * 0.1)).clamp(0.0, 1.0);

            SimulationResult {
                candidate,
                win_rate,
                avg_hull_remaining,
            }
        })
        .collect()
}

fn stable_seed(
    ship: &str,
    hostile: &str,
    captain: &str,
    bridge: &str,
    below_decks: &str,
    seed: u64,
) -> u64 {
    [ship, hostile, captain, bridge, below_decks]
        .into_iter()
        .flat_map(str::bytes)
        .fold(seed, |acc, b| {
            acc.wrapping_mul(37).wrapping_add(u64::from(b))
        })
}
