//! Genetic algorithm optimizer for large crew search spaces.
//! Evolves a population of crew combinations using Monte Carlo fitness, selection, crossover, and mutation.
//!
//! # Seeded Initialization
//! When `GeneticConfig::seed_population` is non-empty, the initial population is seeded
//! with those crew candidates, then filled with random crews to reach `population_size`.
//! This enables warm-start optimization from community-known crews (heuristics seeds).
//!
//! # Adaptive Mutation
//! When `adaptive_mutation` is true and the population is seeded, the mutation rate starts
//! low (`mutation_rate_floor`) and increases on stagnation up to `mutation_rate_ceiling`,
//! balancing gentle exploration around good seeds with escape from local optima.

use crate::combat::rng::Rng;
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
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub only_below_decks_with_ability: bool,

    /// Pre-built crew candidates to seed the initial population.
    /// When non-empty, these replace random initialization; remaining slots filled randomly.
    /// When empty, pure random init (current behavior).
    pub seed_population: Vec<CrewCandidate>,

    /// When true, mutation rate starts low and increases on stagnation.
    pub adaptive_mutation: bool,

    /// Starting mutation rate when adaptive + seeded. Defaults to 0.05.
    pub mutation_rate_floor: f64,

    /// Maximum mutation rate for adaptive schedule. Defaults to 0.40.
    pub mutation_rate_ceiling: f64,
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
            only_below_decks_with_ability: false,
            seed_population: Vec::new(),
            adaptive_mutation: true,
            mutation_rate_floor: 0.05,
            mutation_rate_ceiling: 0.40,
        }
    }
}

impl GeneticConfig {
    /// Config tuned for seeded populations: larger pop, more generations, adaptive mutation.
    /// Population size is 2× the seed count (min 80, max 200).
    pub fn seeded(seed_population: Vec<CrewCandidate>) -> Self {
        let pop_size = (seed_population.len() * 2).clamp(80, 200);
        Self {
            population_size: pop_size,
            generations: 60,
            sims_per_eval: 500,
            stagnation_limit: Some(15),
            seed_population,
            adaptive_mutation: true,
            mutation_rate_floor: 0.05,
            mutation_rate_ceiling: 0.40,
            ..Self::default()
        }
    }
}

/// Extension trait providing additional RNG methods used by the genetic algorithm.
trait RngExt {
    /// Returns a uniform index in [0, n) or 0 if n == 0.
    fn index(&mut self, n: usize) -> usize;

    /// Returns a uniform float in [0.0, 1.0).
    fn next_f64(&mut self) -> f64;
}

impl RngExt for Rng {
    fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() as usize) % n
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64 + 1.0)
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

/// Initialize population with optional seed candidates, filling remaining slots randomly.
/// When `seed_candidates` is empty, this behaves identically to pure random initialization.
fn init_population_seeded(
    pools: &OfficerPools,
    population_size: usize,
    seed_candidates: &[CrewCandidate],
    seed: u64,
) -> Vec<CrewCandidate> {
    let mut pop = Vec::with_capacity(population_size);

    // Inject seed candidates (up to population_size, preserving order = author priority).
    for candidate in seed_candidates.iter().take(population_size) {
        pop.push(candidate.clone());
    }

    // Fill remaining slots with random crews.
    let mut rng = Rng::new(seed);
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
/// Progress callback: (generation, max_generations, best_fitness); returns false to abort.
pub fn run_genetic_optimizer(
    ship: &str,
    hostile: &str,
    config: &GeneticConfig,
    seed: u64,
    mut on_progress: impl FnMut(usize, usize, f32) -> bool,
) -> Vec<CrewCandidate> {
    let pools = match build_officer_pools(config.only_below_decks_with_ability) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut population = init_population_seeded(
        &pools,
        config.population_size,
        &config.seed_population,
        seed,
    );
    if population.is_empty() {
        return Vec::new();
    }

    // Adaptive mutation: start low when seeded, ramp up on stagnation.
    let is_seeded = !config.seed_population.is_empty();
    let mut current_mutation_rate = if is_seeded && config.adaptive_mutation {
        config.mutation_rate_floor
    } else {
        config.mutation_rate
    };

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

        // Adaptive mutation: bump rate by 1.5× every 3 stagnant generations.
        if config.adaptive_mutation && stagnation > 0 && stagnation.is_multiple_of(3) {
            current_mutation_rate =
                (current_mutation_rate * 1.5).min(config.mutation_rate_ceiling);
        }

        if !on_progress(generation + 1, config.generations, best_fitness) {
            break;
        }

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
            mutate(&mut child, &pools, current_mutation_rate, &mut rng);
            next_pop.push(child);
        }
        population = next_pop;
    }

    best_individuals
}

/// Run genetic optimization and return ranked results (same shape as optimize_scenario).
/// Runs a final Monte Carlo pass on top candidates with requested sim count, then ranks.
/// Progress callback returns false to abort.
pub fn run_genetic_optimizer_ranked(
    ship: &str,
    hostile: &str,
    config: &GeneticConfig,
    seed: u64,
    final_sims: usize,
    mut on_progress: impl FnMut(usize, usize, f32) -> bool,
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
    use super::{crossover, init_population_seeded, mutate, random_crew, repair_crew, GeneticConfig};
    use crate::combat::rng::Rng;
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

    fn make_crew(cap: &str, b: &[&str], bd: &[&str]) -> CrewCandidate {
        CrewCandidate {
            captain: cap.into(),
            bridge: b.iter().map(|s| (*s).into()).collect(),
            below_decks: bd.iter().map(|s| (*s).into()).collect(),
        }
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
        let a = make_crew("CapA", &["B1", "B2"], &["D1", "D2", "D3"]);
        let b = make_crew("CapB", &["B3", "B4"], &["D4", "D5", "D1"]);
        let mut rng = Rng::new(99);
        for _ in 0..10 {
            let child = crossover(&a, &b, &pools, &mut rng);
            assert!(valid_crew(&child), "child should be valid: {:?}", child);
        }
    }

    #[test]
    fn mutate_preserves_valid_crew() {
        let pools = small_pools();
        let mut crew = make_crew("CapA", &["B1", "B2"], &["D1", "D2", "D3"]);
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
        assert!(c.seed_population.is_empty());
        assert!(c.adaptive_mutation);
        assert!(c.mutation_rate_floor < c.mutation_rate);
        assert!(c.mutation_rate_ceiling > c.mutation_rate);
    }

    #[test]
    fn seeded_config_scales_population() {
        // 5 seeds → pop_size = max(10, 80) = 80
        let seeds: Vec<CrewCandidate> = (0..5)
            .map(|i| make_crew(&format!("Cap{i}"), &["B1", "B2"], &["D1", "D2", "D3"]))
            .collect();
        let cfg = GeneticConfig::seeded(seeds);
        assert_eq!(cfg.population_size, 80);
        assert_eq!(cfg.generations, 60);
        assert_eq!(cfg.seed_population.len(), 5);

        // 120 seeds → pop_size = min(240, 200) = 200
        let many_seeds: Vec<CrewCandidate> = (0..120)
            .map(|i| make_crew(&format!("Cap{i}"), &["B1", "B2"], &["D1", "D2", "D3"]))
            .collect();
        let cfg2 = GeneticConfig::seeded(many_seeds);
        assert_eq!(cfg2.population_size, 200);
    }

    #[test]
    fn init_population_seeded_uses_seeds() {
        let pools = small_pools();
        let seed_a = make_crew("CapA", &["B1", "B2"], &["D1", "D2", "D3"]);
        let seed_b = make_crew("CapB", &["B3", "B4"], &["D4", "D5", "D1"]);
        let seeds = vec![seed_a.clone(), seed_b.clone()];

        let pop = init_population_seeded(&pools, 6, &seeds, 42);
        assert_eq!(pop.len(), 6, "population should be full");
        // First two should be our seeds.
        assert_eq!(pop[0].captain, seed_a.captain);
        assert_eq!(pop[1].captain, seed_b.captain);
        // All should be valid.
        for crew in &pop {
            assert!(valid_crew(crew), "crew should be valid: {:?}", crew);
        }
    }

    #[test]
    fn init_population_seeded_truncates_excess() {
        let pools = small_pools();
        let seeds: Vec<CrewCandidate> = (0..10)
            .map(|i| make_crew(if i % 2 == 0 { "CapA" } else { "CapB" }, &["B1", "B2"], &["D1", "D2", "D3"]))
            .collect();
        let pop = init_population_seeded(&pools, 4, &seeds, 99);
        assert_eq!(pop.len(), 4, "population should be capped at population_size");
    }

    #[test]
    fn init_population_seeded_empty_is_random() {
        let pools = small_pools();
        let pop_seeded = init_population_seeded(&pools, 8, &[], 42);
        assert_eq!(pop_seeded.len(), 8);
        for crew in &pop_seeded {
            assert!(valid_crew(crew));
        }
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
            |_, _, _| true,
        );
        let b = super::run_genetic_optimizer(
            "enterprise",
            "swarm",
            &config,
            12345,
            |_, _, _| true,
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
                true
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
