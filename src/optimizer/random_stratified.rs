//! Stratified random crew sampling — the `random_stratified` lane
//! (docs/OPTIMIZER_AMBITIOUS_ROADMAP.md §1.1).
//!
//! Samples legal crews from the same eligibility-filtered officer pools the
//! generator uses, stratified so coverage spreads across captain
//! (faction, rarity) cells and below-decks group families instead of
//! clustering on pool-order neighbors. Used two ways:
//!
//! - as a standalone [`crate::optimizer::OptimizerStrategy::RandomStratified`]
//!   benchmark-control lane (scout → confirm on a pure random candidate set), and
//! - as the tiered scout-phase exploration slice
//!   (`tiered_random_exploration_pct`): a budget-neutral swap of part of the
//!   scout candidate set for random crews that bypass the analytical prefilter.
//!
//! Deterministic: same seed and same catalog produce the same sample.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::combat::rng::Rng;
use crate::combat::EnemyType;
use crate::data::data_registry::DataRegistry;
use crate::data::heuristics::BelowDecksPoolMode;
use crate::optimizer::constraints::{normalize_officer_name, CrewSearchConstraints};
use crate::optimizer::crew_generator::{
    build_officer_pools_from_registry, CrewCandidate, BRIDGE_SLOTS,
};
use crate::optimizer::monte_carlo::crew_candidate_stable_hash;

/// Method-provenance label for crews produced by this lane.
pub const METHOD_RANDOM_STRATIFIED: &str = "random_stratified";

/// Candidate count for the standalone strategy when the request omits `max_candidates`.
pub const DEFAULT_RANDOM_STRATIFIED_CANDIDATES: usize = 384;

/// Seed salt so the sample stream is decorrelated from the Monte Carlo streams
/// that consume the same scenario seed.
const RANDOM_STRATIFIED_SEED_SALT: u64 = 0x5261_6e64_5374_7261; // "RandStra"

/// Inputs for [`sample_stratified_random_crews`]. Mirrors the pool-building
/// parameters of the main candidate generator so sampled crews face the exact
/// same roster/ban/eligibility filters.
#[derive(Debug)]
pub struct StratifiedSampleParams<'a> {
    /// Target number of distinct crews (best effort; small pools may yield fewer).
    pub count: usize,
    /// Scenario seed; the sampler derives its own decorrelated stream from it.
    pub seed: u64,
    pub below_decks_slots: usize,
    pub below_decks_pool_mode: BelowDecksPoolMode,
    pub enemy_type: EnemyType,
    pub profile_id: Option<&'a str>,
    /// Explicit optimize constraints; sampled crews must satisfy them.
    pub constraints: Option<&'a CrewSearchConstraints>,
    /// Stable hashes of crews already in the candidate set; collisions are skipped.
    pub exclude_hashes: Option<&'a HashSet<u64>>,
}

/// Officer strata keys derived from LCARS metadata, all lowercase; officers
/// missing metadata land in an `"unknown"` cell so they stay reachable.
struct StrataMetadata {
    /// normalized officer name → (faction, rarity, group)
    by_name: HashMap<String, (String, String, String)>,
}

impl StrataMetadata {
    fn from_registry(registry: &DataRegistry) -> Self {
        let mut by_name = HashMap::new();
        if let Some(lcars) = registry.lcars_officers() {
            for officer in lcars {
                let key = normalize_officer_name(&officer.name);
                if key.is_empty() {
                    continue;
                }
                by_name.insert(
                    key,
                    (
                        normalize_stratum(officer.faction.as_deref()),
                        normalize_stratum(officer.rarity.as_deref()),
                        normalize_stratum(officer.group.as_deref()),
                    ),
                );
            }
        }
        // Canonical catalog fallback for group when LCARS is absent.
        for officer in registry.officers() {
            let key = normalize_officer_name(&officer.name);
            if key.is_empty() || by_name.contains_key(&key) {
                continue;
            }
            by_name.insert(
                key,
                (
                    unknown_stratum(),
                    unknown_stratum(),
                    normalize_stratum(officer.group.as_deref()),
                ),
            );
        }
        Self { by_name }
    }

    fn faction_rarity(&self, name: &str) -> (String, String) {
        self.by_name
            .get(&normalize_officer_name(name))
            .map(|(f, r, _)| (f.clone(), r.clone()))
            .unwrap_or_else(|| (unknown_stratum(), unknown_stratum()))
    }

    fn group(&self, name: &str) -> String {
        self.by_name
            .get(&normalize_officer_name(name))
            .map(|(_, _, g)| g.clone())
            .unwrap_or_else(unknown_stratum)
    }
}

fn normalize_stratum(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or("").trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        unknown_stratum()
    } else {
        trimmed
    }
}

fn unknown_stratum() -> String {
    "unknown".to_string()
}

/// Deterministic strata: BTreeMap keeps cell iteration order stable across runs.
fn build_strata<K: Ord>(pool: &[String], key_of: impl Fn(&str) -> K) -> Vec<Vec<String>> {
    let mut cells: BTreeMap<K, Vec<String>> = BTreeMap::new();
    for name in pool {
        cells.entry(key_of(name)).or_default().push(name.clone());
    }
    cells.into_values().collect()
}

fn rng_index(rng: &mut Rng, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (rng.next_u64() % n as u64) as usize
    }
}

/// Random distinct pick: bounded random probes, then a deterministic linear sweep
/// so small pools cannot livelock.
fn choose_distinct(pool: &[String], used: &mut HashSet<String>, rng: &mut Rng) -> Option<String> {
    for _ in 0..32 {
        let value = pool.get(rng_index(rng, pool.len()))?;
        if used.insert(value.to_ascii_lowercase()) {
            return Some(value.clone());
        }
    }
    for value in pool {
        if used.insert(value.to_ascii_lowercase()) {
            return Some(value.clone());
        }
    }
    None
}

/// Sample up to `params.count` distinct legal crews, stratified by captain
/// (faction, rarity) cell and below-decks group family.
///
/// Captain cells are visited round-robin so rare factions/rarities are sampled
/// as often as crowded ones; bridge seats are uniform over the bridge pool;
/// below-decks seats alternate between a family-stratified pick and a uniform
/// pick. Crews violating `constraints` or colliding with `exclude_hashes` are
/// rejected and re-drawn. Returns fewer than `count` crews when the legal space
/// is too small.
pub fn sample_stratified_random_crews(
    registry: &DataRegistry,
    params: &StratifiedSampleParams<'_>,
) -> Vec<CrewCandidate> {
    if params.count == 0 {
        return Vec::new();
    }
    let Some(pools) = build_officer_pools_from_registry(
        registry,
        params.below_decks_pool_mode,
        Some(params.enemy_type),
        params.profile_id,
        params.below_decks_slots,
        params.constraints,
    ) else {
        return Vec::new();
    };
    let metadata = StrataMetadata::from_registry(registry);
    let captain_strata = build_strata(&pools.captains, |name| metadata.faction_rarity(name));
    let bd_family_strata = build_strata(&pools.below_decks, |name| metadata.group(name));
    if captain_strata.is_empty() {
        return Vec::new();
    }

    let mut rng = Rng::new(params.seed ^ RANDOM_STRATIFIED_SEED_SALT);
    let mut out: Vec<CrewCandidate> = Vec::with_capacity(params.count);
    let mut seen: HashSet<u64> = HashSet::new();
    let constraints_active = params.constraints.is_some_and(|c| !c.is_empty());
    let max_attempts = params.count.saturating_mul(64).max(256);
    for attempt in 0..max_attempts {
        if out.len() >= params.count {
            break;
        }
        // Round-robin over captain (faction, rarity) cells; uniform inside a cell.
        let cell = &captain_strata[attempt % captain_strata.len()];
        let captain = cell[rng_index(&mut rng, cell.len())].clone();
        let mut used: HashSet<String> = HashSet::new();
        used.insert(captain.to_ascii_lowercase());

        let mut bridge = Vec::with_capacity(BRIDGE_SLOTS);
        for _ in 0..BRIDGE_SLOTS {
            let Some(value) = choose_distinct(&pools.bridge, &mut used, &mut rng) else {
                break;
            };
            bridge.push(value);
        }
        if bridge.len() != BRIDGE_SLOTS {
            continue;
        }

        let mut below_decks = Vec::with_capacity(params.below_decks_slots);
        for slot in 0..params.below_decks_slots {
            // Even slots draw from a rotating group family (when families exist),
            // odd slots draw uniformly, so crews mix family-coherent and
            // family-crossing picks.
            let family_pick = if slot % 2 == 0 && !bd_family_strata.is_empty() {
                let family = &bd_family_strata[(attempt + slot) % bd_family_strata.len()];
                choose_distinct(family, &mut used, &mut rng)
            } else {
                None
            };
            let Some(value) =
                family_pick.or_else(|| choose_distinct(&pools.below_decks, &mut used, &mut rng))
            else {
                break;
            };
            below_decks.push(value);
        }
        if below_decks.len() != params.below_decks_slots {
            continue;
        }

        let crew = CrewCandidate {
            captain,
            bridge,
            below_decks,
        };
        if constraints_active && !params.constraints.is_some_and(|c| c.satisfies(&crew)) {
            continue;
        }
        let hash = crew_candidate_stable_hash(&crew);
        if params.exclude_hashes.is_some_and(|ex| ex.contains(&hash)) {
            continue;
        }
        if seen.insert(hash) {
            out.push(crew);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::crew_generator::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS;

    fn registry() -> std::sync::Arc<DataRegistry> {
        DataRegistry::load().expect("data registry required for sampler tests")
    }

    fn params<'a>(count: usize, seed: u64) -> StratifiedSampleParams<'a> {
        StratifiedSampleParams {
            count,
            seed,
            below_decks_slots: 3,
            below_decks_pool_mode: BelowDecksPoolMode::Strict,
            enemy_type: EnemyType::RedMovingSpace,
            // Unit tests must not depend on whichever mutable local profile is
            // currently selected in profiles/index.json.
            profile_id: Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            constraints: None,
            exclude_hashes: None,
        }
    }

    #[test]
    fn sampling_is_deterministic_per_seed() {
        let registry = registry();
        let a = sample_stratified_random_crews(&registry, &params(24, 7));
        let b = sample_stratified_random_crews(&registry, &params(24, 7));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.captain, y.captain);
            assert_eq!(x.bridge, y.bridge);
            assert_eq!(x.below_decks, y.below_decks);
        }
        let c = sample_stratified_random_crews(&registry, &params(24, 8));
        let hashes_a: HashSet<u64> = a.iter().map(crew_candidate_stable_hash).collect();
        let hashes_c: HashSet<u64> = c.iter().map(crew_candidate_stable_hash).collect();
        assert_ne!(
            hashes_a, hashes_c,
            "different seeds should change the sample"
        );
    }

    #[test]
    fn sampled_crews_are_legal_and_distinct() {
        let registry = registry();
        let p = params(32, 42);
        let pools = build_officer_pools_from_registry(
            &registry,
            p.below_decks_pool_mode,
            Some(p.enemy_type),
            p.profile_id,
            p.below_decks_slots,
            p.constraints,
        )
        .expect("pools");
        let captains: HashSet<String> = pools.captains.iter().cloned().collect();
        let bridge: HashSet<String> = pools.bridge.iter().cloned().collect();
        let below: HashSet<String> = pools.below_decks.iter().cloned().collect();
        let crews = sample_stratified_random_crews(&registry, &p);
        assert!(!crews.is_empty(), "sampler should produce crews");
        let mut seen = HashSet::new();
        for crew in &crews {
            assert!(
                captains.contains(&crew.captain),
                "captain from captain pool"
            );
            assert_eq!(crew.bridge.len(), BRIDGE_SLOTS);
            assert_eq!(crew.below_decks.len(), p.below_decks_slots);
            for officer in &crew.bridge {
                assert!(bridge.contains(officer), "bridge officer from bridge pool");
            }
            for officer in &crew.below_decks {
                assert!(below.contains(officer), "BD officer from below-decks pool");
            }
            let mut names: Vec<String> = std::iter::once(crew.captain.clone())
                .chain(crew.bridge.iter().cloned())
                .chain(crew.below_decks.iter().cloned())
                .map(|n| n.to_ascii_lowercase())
                .collect();
            names.sort();
            names.dedup();
            assert_eq!(
                names.len(),
                1 + BRIDGE_SLOTS + p.below_decks_slots,
                "no duplicate officers within a crew"
            );
            assert!(
                seen.insert(crew_candidate_stable_hash(crew)),
                "no duplicate crews in the sample"
            );
        }
    }

    #[test]
    fn sampling_spreads_across_captain_strata() {
        let registry = registry();
        let crews = sample_stratified_random_crews(&registry, &params(48, 3));
        let metadata = StrataMetadata::from_registry(&registry);
        let cells: HashSet<(String, String)> = crews
            .iter()
            .map(|c| metadata.faction_rarity(&c.captain))
            .collect();
        assert!(
            cells.len() > 1,
            "captains should span multiple (faction, rarity) cells, got {cells:?}"
        );
    }

    #[test]
    fn exclude_hashes_and_constraints_are_respected() {
        let registry = registry();
        let first = sample_stratified_random_crews(&registry, &params(8, 11));
        assert!(!first.is_empty());
        let exclude: HashSet<u64> = first.iter().map(crew_candidate_stable_hash).collect();
        let mut p = params(8, 11);
        p.exclude_hashes = Some(&exclude);
        let second = sample_stratified_random_crews(&registry, &p);
        for crew in &second {
            assert!(
                !exclude.contains(&crew_candidate_stable_hash(crew)),
                "excluded crew hashes must not be re-sampled"
            );
        }

        let banned = first[0].captain.clone();
        let constraints = CrewSearchConstraints {
            exclude: vec![banned.clone()],
            ..Default::default()
        };
        let mut p = params(16, 11);
        p.constraints = Some(&constraints);
        let constrained = sample_stratified_random_crews(&registry, &p);
        let banned_norm = normalize_officer_name(&banned);
        for crew in &constrained {
            let has_banned = std::iter::once(&crew.captain)
                .chain(&crew.bridge)
                .chain(&crew.below_decks)
                .any(|n| normalize_officer_name(n) == banned_norm);
            assert!(!has_banned, "excluded officer must never be sampled");
        }
    }
}
