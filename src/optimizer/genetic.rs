//! Genetic algorithm optimizer for large crew search spaces.
//! Evolves a population of crew combinations using Monte Carlo fitness, selection, crossover, and mutation.

use crate::optimizer::crew_generator::{
    build_officer_pools, OfficerPools, CrewCandidate, BRIDGE_SLOTS, BELOW_DECKS_SLOTS,
};
use crate::optimizer::monte_carlo::{run_monte_carlo_parallel, SimulationResult};
use crate::optimizer::ranking::{rank_results, RankedCrewResult};
use std::collections::HashSet;

/// Same scalar as ranking: win_rate * 0.8 + avg_hull_remaining * 0.2
fn fitness_from_result(result: &SimulationResult) -> f32 {
    (result.win_rate * 0.8 + result.avg_hull_remaining * 0.2) as f32
}

/// Configuration for the genetic algorithm.
#[derive(Debug, Clone)]
pub struct GeneticConfig {
    pub population_size: usize,
    pub generations: usize,
    pub mutation_rate: f64,
    pub sims_per_eval: usize,
    pub tournament_size: usize,
    pub elitism_count: usize,
    /// Stop early if best fitness has not improved for this many generations.
    pub stagnation_limit: Option<usize>,
}

impl Default for GeneticConfig {
    fn default() -> Self {
        Self {
            population_size: 64,
            generations: 40,
            mutation_rate: 0.15,
            sims_per_eval: 500,
            tournament_size: 3,
            elitism_count: 2,
            stagnation_limit: Some(10),
        }
    }
}

/// Deterministic RNG for reproducible GA runs. Uses same LCG style as crew_generator.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    /// Returns a uniform index in [0, n) or 0 if n == 0.
    fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() as usize) % n
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() as f64) / (u64::MAX as f64 + 1.0)
    }
}

/// Build one random valid crew from pools. Uses names for distinctness.
fn random_crew(rng: &mut Rng, pools: &OfficerPools) -> Option<CrewCandidate> {
    if pools.captains.is_empty()
        || pools.bridge.len() < BRIDGE_SLOTS
        || pools.below_decks.len() < BELOW_DECKS_SLOTS
    {
        return None;
    }
    let captain = pools.captains[rng.index(pools.captains.len())].clone();
    let mut used: HashSet<String> = HashSet::new();
    used.insert(captain.clone());

    let mut bridge = Vec::with_capacity(BRIDGE_SLOTS);
    for _ in 0..BRIDGE_SLOTS {
        let available: Vec<&String> = pools.bridge.iter().filter(|s| !used.contains(*s)).collect();
        if available.is_empty() {
            return None;
        }
        let name = available[rng.index(available.len())].clone();
        bridge.push(name.clone());
        used.insert(name);
    }

    let mut below_decks = Vec::with_capacity(BELOW_DECKS_SLOTS);
    for _ in 0..BELOW_DECKS_SLOTS {
        let available: Vec<&String> = pools
            .below_decks
            .iter()
            .filter(|s| !used.contains(*s))
            .collect();
        if available.is_empty() {
            return None;
        }
        let name = available[rng.index(available.len())].clone();
        below_decks.push(name.clone());
        used.insert(name);
    }

    Some(CrewCandidate {
        captain,
        bridge,
        below_decks,
    })
}

/// Initialize population with random valid crews.
fn init_population(
    pools: &OfficerPools,
    population_size: usize,
    seed: u64,
) -> Vec<CrewCandidate> {
    let mut rng = Rng::new(seed);
    let mut pop = Vec::with_capacity(population_size);
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 50_000;
    while pop.len() < population_size && attempts < MAX_ATTEMPTS {
        if let Some(crew) = random_crew(&mut rng, pools) {
            pop.push(crew);
        }
        attempts += 1;
    }
    pop
}

/// Tournament selection: pick best of `tournament_size` random individuals by fitness.
fn tournament_select(
    population: &[CrewCandidate],
    fitness: &[f32],
    tournament_size: usize,
    rng: &mut Rng,
) -> usize {
    let n = population.len();
    if n == 0 {
        return 0;
    }
    let mut best_idx = rng.index(n);
    let mut best_fit = fitness[best_idx];
    for _ in 1..tournament_size {
        let idx = rng.index(n);
        if fitness[idx] > best_fit {
            best_fit = fitness[idx];
            best_idx = idx;
        }
    }
    best_idx
}

/// Crossover: produce one child from two parents. Child has distinct officers.
fn crossover(
    a: &CrewCandidate,
    b: &CrewCandidate,
    pools: &OfficerPools,
    rng: &mut Rng,
) -> CrewCandidate {
    let captain = if rng.next_f64() < 0.5 { &a.captain } else { &b.captain };
    let captain = captain.clone();
    let mut used: HashSet<String> = HashSet::new();
    used.insert(captain.clone());

    let bridge_union: Vec<String> = a
        .bridge
        .iter()
        .chain(b.bridge.iter())
        .map(String::clone)
        .filter(|s| !used.contains(s))
        .collect();
    let bridge_set: HashSet<String> = bridge_union.into_iter().collect();
    let mut bridge_vec: Vec<String> = bridge_set.into_iter().collect();
    while bridge_vec.len() < BRIDGE_SLOTS {
        let available: Vec<&String> = pools.bridge.iter().filter(|s| !used.contains(*s)).collect();
        if available.is_empty() {
            break;
        }
        let pick = available[rng.index(available.len())].clone();
        bridge_vec.push(pick.clone());
        used.insert(pick);
    }
    if bridge_vec.len() > BRIDGE_SLOTS {
        bridge_vec.truncate(BRIDGE_SLOTS);
    }
    for s in bridge_vec.iter() {
        used.insert(s.clone());
    }

    let below_union: Vec<String> = a
        .below_decks
        .iter()
        .chain(b.below_decks.iter())
        .map(String::clone)
        .filter(|s| !used.contains(s))
        .collect();
    let below_set: HashSet<String> = below_union.into_iter().collect();
    let mut below_vec: Vec<String> = below_set.into_iter().collect();
    while below_vec.len() < BELOW_DECKS_SLOTS {
        let available: Vec<&String> = pools
            .below_decks
            .iter()
            .filter(|s| !used.contains(*s))
            .collect();
        if available.is_empty() {
            break;
        }
        let pick = available[rng.index(available.len())].clone();
        below_vec.push(pick.clone());
        used.insert(pick);
    }
    if below_vec.len() > BELOW_DECKS_SLOTS {
        below_vec.truncate(BELOW_DECKS_SLOTS);
    }

    CrewCandidate {
        captain,
        bridge: bridge_vec,
        below_decks: below_vec,
    }
}

/// Ensure crew has exactly BRIDGE_SLOTS and BELOW_DECKS_SLOTS with no duplicates. Fill from pools if needed.
fn repair_crew(crew: &mut CrewCandidate, pools: &OfficerPools, rng: &mut Rng) {
    let mut used: HashSet<String> = HashSet::new();
    used.insert(crew.captain.clone());
    for s in crew.bridge.iter() {
        used.insert(s.clone());
    }
    for s in crew.below_decks.iter() {
        used.insert(s.clone());
    }

    while crew.bridge.len() < BRIDGE_SLOTS {
        let available: Vec<&String> = pools.bridge.iter().filter(|s| !used.contains(*s)).collect();
        if available.is_empty() {
            break;
        }
        let pick = available[rng.index(available.len())].clone();
        crew.bridge.push(pick.clone());
        used.insert(pick.clone());
    }
    crew.bridge.truncate(BRIDGE_SLOTS);

    while crew.below_decks.len() < BELOW_DECKS_SLOTS {
        let available: Vec<&String> = pools
            .below_decks
            .iter()
            .filter(|s| !used.contains(*s))
            .collect();
        if available.is_empty() {
            break;
        }
        let pick = available[rng.index(available.len())].clone();
        crew.below_decks.push(pick.clone());
        used.insert(pick.clone());
    }
    crew.below_decks.truncate(BELOW_DECKS_SLOTS);
}

/// Mutate one slot: replace with random officer from the appropriate pool, enforce distinctness.
fn mutate(crew: &mut CrewCandidate, pools: &OfficerPools, rate: f64, rng: &mut Rng) {
    if rng.next_f64() >= rate {
        return;
    }
    let slot = rng.index(6);
    let mut used: HashSet<&str> = HashSet::new();
    used.insert(crew.captain.as_str());
    for s in crew.bridge.iter() {
        used.insert(s.as_str());
    }
    for s in crew.below_decks.iter() {
        used.insert(s.as_str());
    }

    match slot {
        0 => {
            let available: Vec<&String> = pools.captains.iter().filter(|s| !used.contains(s.as_str())).collect();
            if !available.is_empty() {
                crew.captain = available[rng.index(available.len())].clone();
            }
        }
        1 => {
            let available: Vec<&String> = pools.bridge.iter().filter(|s| !used.contains(s.as_str())).collect();
            if !available.is_empty() && !crew.bridge.is_empty() {
                crew.bridge[0] = available[rng.index(available.len())].clone();
            }
        }
        2 => {
            let available: Vec<&String> = pools.bridge.iter().filter(|s| !used.contains(s.as_str())).collect();
            if !available.is_empty() && crew.bridge.len() > 1 {
                crew.bridge[1] = available[rng.index(available.len())].clone();
            }
        }
        3..=5 => {
            let di = slot - 3;
            let available: Vec<&String> = pools
                .below_decks
                .iter()
                .filter(|s| !used.contains(s.as_str()))
                .collect();
            if !available.is_empty() && di < crew.below_decks.len() {
                crew.below_decks[di] = available[rng.index(available.len())].clone();
            }
        }
        _ => {}
    }
    repair_crew(crew, pools, rng);
}

/// Run genetic optimization. Returns top individuals for final ranking.
/// Progress callback: (generation, max_generations, best_fitness).
pub fn run_genetic_optimizer(
    ship: &str,
    hostile: &str,
    config: &GeneticConfig,
    seed: u64,
    mut on_progress: impl FnMut(usize, usize, f32),
) -> Vec<CrewCandidate> {
    let pools = match build_officer_pools() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut population = init_population(&pools, config.population_size, seed);
    if population.is_empty() {
        return Vec::new();
    }

    let mut best_fitness = -1.0f32;
    let mut best_individuals: Vec<CrewCandidate> = Vec::new();
    let mut stagnation = 0_usize;

    for generation in 0..config.generations {
        let sim_results = run_monte_carlo_parallel(
            ship,
            hostile,
            &population,
            config.sims_per_eval,
            seed.wrapping_add(generation as u64),
        );
        let fitness: Vec<f32> = sim_results.iter().map(fitness_from_result).collect();

        let mut indexed: Vec<(usize, f32)> = fitness.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let gen_best = indexed.first().map(|(_, f)| *f).unwrap_or(-1.0);
        if gen_best > best_fitness {
            best_fitness = gen_best;
            stagnation = 0;
            best_individuals = indexed
                .iter()
                .take(config.elitism_count.max(10))
                .map(|(i, _)| population[*i].clone())
                .collect();
        } else {
            stagnation += 1;
        }

        on_progress(generation + 1, config.generations, best_fitness);

        if let Some(limit) = config.stagnation_limit {
            if stagnation >= limit {
                break;
            }
        }

        let mut rng = Rng::new(seed.wrapping_add(0x1234_5678).wrapping_add((generation as u64) << 32));

        let mut next_pop = Vec::with_capacity(config.population_size);
        for i in 0..config.elitism_count {
            if i < population.len() {
                next_pop.push(population[indexed[i].0].clone());
            }
        }
        while next_pop.len() < config.population_size {
            let pa = tournament_select(&population, &fitness, config.tournament_size, &mut rng);
            let pb = tournament_select(&population, &fitness, config.tournament_size, &mut rng);
            let mut child = crossover(&population[pa], &population[pb], &pools, &mut rng);
            repair_crew(&mut child, &pools, &mut rng);
            mutate(&mut child, &pools, config.mutation_rate, &mut rng);
            next_pop.push(child);
        }
        population = next_pop;
    }

    best_individuals
}

/// Run genetic optimization and return ranked results (same shape as optimize_scenario).
/// Runs a final Monte Carlo pass on top candidates with requested sim count, then ranks.
pub fn run_genetic_optimizer_ranked(
    ship: &str,
    hostile: &str,
    config: &GeneticConfig,
    seed: u64,
    final_sims: usize,
    mut on_progress: impl FnMut(usize, usize, f32),
) -> Vec<RankedCrewResult> {
    let top = run_genetic_optimizer(ship, hostile, config, seed, &mut on_progress);
    if top.is_empty() {
        return Vec::new();
    }
    let final_results = run_monte_carlo_parallel(ship, hostile, &top, final_sims.max(1), seed);
    rank_results(final_results)
}

#[cfg(test)]
mod tests {
    use super::{crossover, mutate, random_crew, repair_crew, GeneticConfig, Rng};
    use crate::optimizer::crew_generator::{CrewCandidate, OfficerPools};

    fn small_pools() -> OfficerPools {
        OfficerPools {
            captains: vec!["CapA".into(), "CapB".into()],
            bridge: vec!["B1".into(), "B2".into(), "B3".into(), "B4".into()],
            below_decks: vec!["D1".into(), "D2".into(), "D3".into(), "D4".into(), "D5".into()],
        }
    }

    fn valid_crew(c: &CrewCandidate) -> bool {
        let mut seen = std::collections::HashSet::new();
        if !seen.insert(c.captain.as_str()) {
            return false;
        }
        for b in &c.bridge {
            if !seen.insert(b.as_str()) {
                return false;
            }
        }
        for d in &c.below_decks {
            if !seen.insert(d.as_str()) {
                return false;
            }
        }
        c.bridge.len() == 2 && c.below_decks.len() == 3
    }

    #[test]
    fn random_crew_produces_valid_crew() {
        let pools = small_pools();
        let mut rng = Rng::new(42);
        for _ in 0..20 {
            let crew = random_crew(&mut rng, &pools).unwrap();
            assert!(valid_crew(&crew), "crew should be valid: {:?}", crew);
        }
    }

    #[test]
    fn crossover_produces_valid_crew() {
        let pools = small_pools();
        let a = CrewCandidate {
            captain: "CapA".into(),
            bridge: vec!["B1".into(), "B2".into()],
            below_decks: vec!["D1".into(), "D2".into(), "D3".into()],
        };
        let b = CrewCandidate {
            captain: "CapB".into(),
            bridge: vec!["B3".into(), "B4".into()],
            below_decks: vec!["D4".into(), "D5".into(), "D1".into()],
        };
        let mut rng = Rng::new(99);
        for _ in 0..10 {
            let child = crossover(&a, &b, &pools, &mut rng);
            assert!(valid_crew(&child), "child should be valid: {:?}", child);
        }
    }

    #[test]
    fn mutate_preserves_valid_crew() {
        let pools = small_pools();
        let mut crew = CrewCandidate {
            captain: "CapA".into(),
            bridge: vec!["B1".into(), "B2".into()],
            below_decks: vec!["D1".into(), "D2".into(), "D3".into()],
        };
        let mut rng = Rng::new(77);
        for _ in 0..20 {
            mutate(&mut crew, &pools, 1.0, &mut rng);
            repair_crew(&mut crew, &pools, &mut rng);
            assert!(valid_crew(&crew), "crew should remain valid: {:?}", crew);
        }
    }

    #[test]
    fn default_config_is_sane() {
        let c = GeneticConfig::default();
        assert!(c.population_size >= 2);
        assert!(c.generations >= 1);
        assert!(c.mutation_rate >= 0.0 && c.mutation_rate <= 1.0);
        assert!(c.sims_per_eval >= 1);
        assert!(c.tournament_size >= 1);
        assert!(c.elitism_count >= 1);
    }

    #[test]
    fn ga_run_is_deterministic_for_same_seed() {
        let config = GeneticConfig {
            population_size: 4,
            generations: 2,
            sims_per_eval: 10,
            ..GeneticConfig::default()
        };
        let a = super::run_genetic_optimizer(
            "enterprise",
            "swarm",
            &config,
            12345,
            |_, _, _| {},
        );
        let b = super::run_genetic_optimizer(
            "enterprise",
            "swarm",
            &config,
            12345,
            |_, _, _| {},
        );
        if a.is_empty() && b.is_empty() {
            return;
        }
        assert_eq!(a.len(), b.len(), "same seed should yield same number of results");
        assert_eq!(a[0].captain, b[0].captain);
        assert_eq!(a[0].bridge, b[0].bridge);
        assert_eq!(a[0].below_decks, b[0].below_decks);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::{run_genetic_optimizer_ranked, GeneticConfig};

    /// Runs genetic optimizer with minimal config; checks result shape.
    /// Requires officer/ship/hostile data (e.g. from data/). Skips if pools are empty.
    #[test]
    fn genetic_optimizer_returns_ranked_shape() {
        let config = GeneticConfig {
            population_size: 8,
            generations: 2,
            sims_per_eval: 30,
            ..GeneticConfig::default()
        };
        let mut progress_calls = 0;
        let results = run_genetic_optimizer_ranked(
            "enterprise",
            "swarm",
            &config,
            99,
            50,
            |gen, max_gen, _| {
                progress_calls += 1;
                assert!(gen <= max_gen);
            },
        );
        if results.is_empty() {
            return;
        }
        for r in &results {
            assert_eq!(r.bridge.len(), 2);
            assert_eq!(r.below_decks.len(), 3);
            assert!(r.win_rate >= 0.0 && r.win_rate <= 1.0);
            assert!(r.score.value >= 0.0);
        }
        assert!(progress_calls >= 1);
    }
}
