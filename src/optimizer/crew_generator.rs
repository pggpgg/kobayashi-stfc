use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Mutex;

use lru::LruCache;

use crate::data::data_registry::DataRegistry;
use crate::data::import::load_imported_roster_ids_unlocked_only;
use crate::data::loader::ship_tiers_levels_and_crew_slots;
use crate::data::officer::{load_canonical_officers, Officer, DEFAULT_CANONICAL_OFFICERS_PATH};
use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, ROSTER_IMPORTED};
use crate::data::ship::CrewSlotUnlock;
use crate::optimizer::constraints::{
    normalize_officer_name, CrewSearchConstraints, OfficerGroupConstraint,
};
use crate::optimizer::officer_learning::OfficerPerformanceScores;
use crate::perf_log;

/// Profile id with no `profiles/<id>/roster.imported.json` in the repo: roster import filter is
/// skipped so tests and tools see the full canonical officer catalog (see unit tests using this id).
pub const NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS: &str =
    "__kobayashi_test_profile_without_roster_dir__";

/// Path to `roster.imported.json` for the given API profile id (or default profile when `None`).
fn roster_import_json_path_for_profile(profile_id: Option<&str>) -> String {
    let pid = profile_id
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| resolve_profile_id_for_api(None));
    profile_path(&pid, ROSTER_IMPORTED)
        .to_string_lossy()
        .into_owned()
}

/// Number of bridge officer slots (in addition to captain). Players typically crew 1 captain + 2 bridge.
pub const BRIDGE_SLOTS: usize = 2;
/// Default below-decks slots when not overridden (matches mid/high-tier STFC ships).
pub const DEFAULT_BELOW_DECKS_SLOTS: usize = 3;
/// Backwards-compatible alias for [`DEFAULT_BELOW_DECKS_SLOTS`].
pub const BELOW_DECKS_SLOTS: usize = DEFAULT_BELOW_DECKS_SLOTS;
/// Minimum explicit/resolvable below-decks slots (early-game ships may have zero unlocked).
pub const MIN_BELOW_DECKS_SLOTS: usize = 0;
/// STFC currently exposes up to seven below-decks slots on player ships.
pub const MAX_BELOW_DECKS_SLOTS: usize = 7;

/// Tier-aware default when ship JSON has no `crew_slots` schedule (legacy heuristic).
pub fn default_below_decks_slots_for_tier(ship_tier: Option<u32>) -> usize {
    match ship_tier {
        Some(1) => 2,
        _ => DEFAULT_BELOW_DECKS_SLOTS,
    }
}

/// Resolve slot count: explicit API override, else unlock schedule + ship level, else tier default.
pub fn resolve_below_decks_slots(
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    crew_slots: Option<&[CrewSlotUnlock]>,
    explicit: Option<u32>,
) -> usize {
    if let Some(n) = explicit {
        let n = n as usize;
        return n.clamp(MIN_BELOW_DECKS_SLOTS, MAX_BELOW_DECKS_SLOTS);
    }
    let level = ship_level.unwrap_or(1).max(1);
    if let Some(schedule) = crew_slots {
        if !schedule.is_empty() {
            return schedule
                .iter()
                .filter(|s| s.unlock_level <= level)
                .count()
                .clamp(MIN_BELOW_DECKS_SLOTS, MAX_BELOW_DECKS_SLOTS);
        }
    }
    default_below_decks_slots_for_tier(ship_tier)
}

/// Resolve below-decks slots using `data/ships_extended` crew schedule when present.
pub fn resolve_below_decks_slots_for_ship(
    ship: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    explicit: Option<u32>,
) -> usize {
    let schedule = ship_tiers_levels_and_crew_slots(ship.trim())
        .map(|(_, _, cs)| cs)
        .filter(|cs| !cs.is_empty());
    resolve_below_decks_slots(ship_tier, ship_level, schedule.as_deref(), explicit)
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
///
/// When `roster.imported.json` exists and parses, **unlocked** roster ids always restrict the
/// officer list (even if fewer than needed to fill a legal crew), so discovery does not silently
/// expand to the full canonical catalog.
pub fn build_officer_pools_from_registry(
    registry: &DataRegistry,
    only_below_decks_with_ability: bool,
    profile_id: Option<&str>,
    below_decks_slots: usize,
    constraints: Option<&CrewSearchConstraints>,
) -> Option<OfficerPools> {
    let officers: Vec<Officer> = registry
        .officers()
        .iter()
        .filter(|o| !o.name.trim().is_empty())
        .cloned()
        .collect();

    let mut officers = officers;
    let roster_path = roster_import_json_path_for_profile(profile_id);
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
        officers.retain(|officer| roster_ids.contains(&officer.id));
    }

    if officers.is_empty() {
        return None;
    }

    let captains: Vec<String> = officers
        .iter()
        .filter(|officer| is_captain_eligible(officer))
        .map(|o| o.name.clone())
        .collect();
    let bridge: Vec<String> = officers
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
    }

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < below_decks_slots {
        return None;
    }

    let pools = OfficerPools {
        captains,
        bridge,
        below_decks,
    };
    match constraints {
        Some(c) if !c.is_empty() => narrow_officer_pools_for_constraints(
            registry.officer_index(),
            pools,
            c,
            below_decks_slots,
        ),
        _ => Some(pools),
    }
}

/// Registry officer name key (alphanumeric only, lowercase) — matches [`DataRegistry::officer_index`].
#[inline]
fn officer_lookup_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn pool_display_name_norm(name: &str) -> String {
    normalize_officer_name(name)
}

fn officer_index_from_officers(officers: &[Officer]) -> HashMap<String, Officer> {
    officers
        .iter()
        .map(|o| (officer_lookup_key(&o.name), o.clone()))
        .collect()
}

/// Maximum cardinality bipartite matching; `adj[i]` lists right-node indices incident to left `i`.
fn bipartite_max_matching(adj: &[Vec<usize>], n_right: usize) -> usize {
    let n_left = adj.len();
    let mut match_r: Vec<Option<usize>> = vec![None; n_right];

    fn dfs(v: usize, adj: &[Vec<usize>], match_r: &mut [Option<usize>], seen: &mut [bool]) -> bool {
        for &u in &adj[v] {
            if u >= seen.len() || seen[u] {
                continue;
            }
            seen[u] = true;
            if match_r[u].is_none()
                || dfs(match_r[u].expect("set when recursive"), adj, match_r, seen)
            {
                match_r[u] = Some(v);
                return true;
            }
        }
        false
    }

    let mut seen = vec![false; n_right];
    let mut size = 0_usize;
    for v in 0..n_left {
        seen.fill(false);
        if dfs(v, adj, &mut match_r, &mut seen) {
            size += 1;
        }
    }
    size
}

/// True if each officer in `keys` can be assigned to a distinct slot (1 captain, 2 bridge, k below).
fn officer_keys_simultaneously_placeable(
    index: &HashMap<String, Officer>,
    pools: &OfficerPools,
    keys: &[&str],
    below_decks_slots: usize,
) -> bool {
    if keys.is_empty() {
        return true;
    }
    let n_slots = 1 + 2 + below_decks_slots;
    if keys.len() > n_slots {
        return false;
    }
    let mut adj: Vec<Vec<usize>> = Vec::with_capacity(keys.len());
    for lk in keys {
        let Some(off) = index.get(*lk) else {
            return false;
        };
        let want = pool_display_name_norm(&off.name);
        let mut edges = Vec::new();
        if pools
            .captains
            .iter()
            .any(|n| pool_display_name_norm(n) == want)
            && is_captain_eligible(off)
        {
            edges.push(0);
        }
        if pools
            .bridge
            .iter()
            .any(|n| pool_display_name_norm(n) == want)
            && can_fill_position(off, Position::Bridge)
        {
            edges.push(1);
            edges.push(2);
        }
        if pools
            .below_decks
            .iter()
            .any(|n| pool_display_name_norm(n) == want)
            && can_fill_position(off, Position::BelowDecks)
        {
            for s in 3..n_slots {
                edges.push(s);
            }
        }
        if edges.is_empty() {
            return false;
        }
        adj.push(edges);
    }
    bipartite_max_matching(&adj, n_slots) == keys.len()
}

/// Maximum score achievable for `group` with [`CrewSearchConstraints::satisfies`] counting rules
/// (duplicate names in the group list each contribute if that officer appears on the crew).
fn max_achievable_group_coverage(
    index: &HashMap<String, Officer>,
    pools: &OfficerPools,
    group: &OfficerGroupConstraint,
    below_decks_slots: usize,
) -> u32 {
    let mut weight_by_lookup_key: HashMap<String, u32> = HashMap::new();
    for entry in &group.officers {
        let lk = officer_lookup_key(entry);
        if lk.is_empty() {
            continue;
        }
        *weight_by_lookup_key.entry(lk).or_insert(0) += 1;
    }
    if weight_by_lookup_key.is_empty() {
        return 0;
    }
    let lookup_keys: Vec<String> = weight_by_lookup_key.keys().cloned().collect();
    let u = lookup_keys.len();
    // Subset enumeration stays cheap; very large groups skip early infeasibility detection here
    // (post-generation [`filter_candidates`] remains authoritative — avoids false negatives).
    const MAX_EXACT_UNIQUE: usize = 16;
    if u > MAX_EXACT_UNIQUE {
        return u32::MAX;
    }

    let mut best = 0_u32;
    for mask in 0..(1_u32 << u) {
        let mut subset: Vec<&str> = Vec::new();
        let mut score = 0_u32;
        for i in 0..u {
            if (mask & (1 << i)) != 0 {
                subset.push(&lookup_keys[i]);
                score = score.saturating_add(weight_by_lookup_key[&lookup_keys[i]]);
            }
        }
        if officer_keys_simultaneously_placeable(index, pools, &subset, below_decks_slots) {
            best = best.max(score);
        }
    }
    best
}

/// Tightens captain/bridge/below pools using search constraints so the generator
/// does not enumerate crews that can never satisfy them. Group rules use a
/// simultaneous-placement check aligned with [`CrewSearchConstraints::satisfies`]; post-generation
/// [`crate::optimizer::constraints::filter_candidates`] remains the source of truth.
pub fn narrow_officer_pools_for_constraints(
    officer_index: &HashMap<String, Officer>,
    mut pools: OfficerPools,
    constraints: &CrewSearchConstraints,
    below_decks_slots: usize,
) -> Option<OfficerPools> {
    if constraints.is_empty() {
        return Some(pools);
    }
    let index = officer_index;

    let exclude_set: HashSet<String> = constraints
        .exclude
        .iter()
        .map(|s| pool_display_name_norm(s))
        .filter(|s| !s.is_empty())
        .collect();

    let strip_excluded = |pool: Vec<String>| -> Vec<String> {
        pool.into_iter()
            .filter(|n| !exclude_set.contains(&pool_display_name_norm(n)))
            .collect()
    };
    pools.captains = strip_excluded(pools.captains);
    pools.bridge = strip_excluded(pools.bridge);
    pools.below_decks = strip_excluded(pools.below_decks);

    if !constraints.captain_must_be.is_empty() {
        let wants: HashSet<String> = constraints
            .captain_must_be
            .iter()
            .map(|s| pool_display_name_norm(s))
            .filter(|s| !s.is_empty())
            .collect();
        pools
            .captains
            .retain(|n| wants.contains(&pool_display_name_norm(n)));
    }

    for b in &constraints.bridge_must_include {
        let want = pool_display_name_norm(b);
        if want.is_empty() {
            continue;
        }
        let off = index.get(&officer_lookup_key(b))?;
        if !can_fill_position(off, Position::Bridge) {
            return None;
        }
        if !pools
            .bridge
            .iter()
            .any(|n| pool_display_name_norm(n) == want)
        {
            return None;
        }
    }

    for b in &constraints.below_decks_must_include {
        let want = pool_display_name_norm(b);
        if want.is_empty() {
            continue;
        }
        let off = index.get(&officer_lookup_key(b))?;
        if !can_fill_position(off, Position::BelowDecks) {
            return None;
        }
        if !pools
            .below_decks
            .iter()
            .any(|n| pool_display_name_norm(n) == want)
        {
            return None;
        }
    }

    pools.captains.retain(|n| {
        index
            .get(&officer_lookup_key(n))
            .map(is_captain_eligible)
            .unwrap_or(false)
    });
    pools.bridge.retain(|n| {
        index
            .get(&officer_lookup_key(n))
            .map(|o| can_fill_position(o, Position::Bridge))
            .unwrap_or(false)
    });
    pools.below_decks.retain(|n| {
        index
            .get(&officer_lookup_key(n))
            .map(|o| can_fill_position(o, Position::BelowDecks))
            .unwrap_or(false)
    });

    for req in &constraints.must_include {
        let want = pool_display_name_norm(req);
        if want.is_empty() {
            continue;
        }
        let off = index.get(&officer_lookup_key(req))?;
        let on_cap = pools
            .captains
            .iter()
            .any(|n| pool_display_name_norm(n) == want);
        let on_bridge = pools
            .bridge
            .iter()
            .any(|n| pool_display_name_norm(n) == want);
        let on_bd = pools
            .below_decks
            .iter()
            .any(|n| pool_display_name_norm(n) == want);
        let placeable = (is_captain_eligible(off) && on_cap)
            || (can_fill_position(off, Position::Bridge) && on_bridge)
            || (can_fill_position(off, Position::BelowDecks) && on_bd);
        if !placeable {
            return None;
        }
    }

    for g in &constraints.groups {
        if max_achievable_group_coverage(index, &pools, g, below_decks_slots) < g.min_count {
            return None;
        }
    }

    if pools.captains.is_empty()
        || pools.bridge.len() < BRIDGE_SLOTS
        || pools.below_decks.len() < below_decks_slots
    {
        return None;
    }

    Some(pools)
}

/// Builds captain, bridge, and below-decks pools from loaded officers and roster filter.
/// When `only_below_decks_with_ability` is true, the below-decks pool is restricted to officers
/// that have a below-decks ability.
///
/// Roster: when `roster.imported.json` exists and parses, unlocked ids always apply (even if the
/// set is smaller than a full crew). There is no fallback that assigns officers to seats they
/// cannot legally fill.
/// Returns `None` if there are not enough officers to form any valid crew.
pub fn build_officer_pools(
    only_below_decks_with_ability: bool,
    below_decks_slots: usize,
    roster_profile_id: Option<&str>,
) -> Option<OfficerPools> {
    build_officer_pools_with_constraints(
        only_below_decks_with_ability,
        below_decks_slots,
        roster_profile_id,
        None,
    )
}

/// Like [`build_officer_pools`], but applies [`narrow_officer_pools_for_constraints`] when
/// `constraints` is non-empty (same registry-free officer load path).
pub fn build_officer_pools_with_constraints(
    only_below_decks_with_ability: bool,
    below_decks_slots: usize,
    roster_profile_id: Option<&str>,
    constraints: Option<&CrewSearchConstraints>,
) -> Option<OfficerPools> {
    let mut officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .map(|loaded| {
            loaded
                .into_iter()
                .filter(|officer| !officer.name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let roster_path = roster_import_json_path_for_profile(roster_profile_id);
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
        officers.retain(|officer| roster_ids.contains(&officer.id));
    }

    if officers.is_empty() {
        return None;
    }

    let officer_index = officer_index_from_officers(&officers);

    let captains: Vec<String> = officers
        .iter()
        .filter(|officer| is_captain_eligible(officer))
        .map(|o| o.name.clone())
        .collect();
    let bridge: Vec<String> = officers
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
    }

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < below_decks_slots {
        return None;
    }

    let pools = OfficerPools {
        captains,
        bridge,
        below_decks,
    };
    match constraints {
        Some(c) if !c.is_empty() => {
            narrow_officer_pools_for_constraints(&officer_index, pools, c, below_decks_slots)
        }
        _ => Some(pools),
    }
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
    /// When set, officer pools are narrowed before enumeration (exclude, seat eligibility).
    pub constraints: Option<CrewSearchConstraints>,
    /// Profile id whose `roster.imported.json` restricts officer pools (`None` = API default profile).
    /// Unit tests may set a synthetic id with no roster file to use the full canonical officer list.
    pub roster_profile_id: Option<String>,
    /// Optional per-officer performance scores for weighted below-decks officer sampling
    /// (learning-based warm start). When set, `sampled_candidates` uses epsilon-greedy
    /// weighted sampling instead of stride-based sampling for below-decks officers.
    pub learned_officer_scores: Option<OfficerPerformanceScores>,
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
            constraints: None,
            roster_profile_id: None,
            learned_officer_scores: None,
        }
    }
}

/// Cache key for officer pools. Captures every input that affects pool construction + narrowing
/// so repeated calls with the same strategy, roster, and constraints skip rebuild.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct OfficerPoolCacheKey {
    only_below_decks_with_ability: bool,
    below_decks_slots: usize,
    roster_profile_id: String,
    constraints_fingerprint: u64,
    from_registry: bool,
}

/// Deterministic 64-bit fingerprint of constraint state so the cache key stays small and cheap to compare.
fn constraints_fingerprint(constraints: Option<&CrewSearchConstraints>) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(c) = constraints {
        c.must_include.hash(&mut hasher);
        c.exclude.hash(&mut hasher);
        for g in &c.groups {
            g.officers.hash(&mut hasher);
            g.min_count.hash(&mut hasher);
        }
        c.captain_must_be.hash(&mut hasher);
        c.bridge_must_include.hash(&mut hasher);
        c.below_decks_must_include.hash(&mut hasher);
    } else {
        false.hash(&mut hasher);
    }
    hasher.finish()
}

/// Default cache capacity — comfortably holds entries for dozens of (ship, hostile, strategy)
/// combinations without noticeable memory pressure.
const POOL_CACHE_CAPACITY: usize = 64;

pub struct CrewGenerator {
    strategy: CandidateStrategy,
    /// Caches narrowed [`OfficerPools`] so repeated calls with the same strategy,
    /// roster, and constraints avoid re-loading officers from disk and re-filtering.
    pool_cache: Mutex<LruCache<OfficerPoolCacheKey, Option<OfficerPools>>>,
}

impl Clone for CrewGenerator {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            pool_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(POOL_CACHE_CAPACITY).expect("64 > 0"),
            )),
        }
    }
}

impl std::fmt::Debug for CrewGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrewGenerator")
            .field("strategy", &self.strategy)
            .field("pool_cache", &"<LRU cache>")
            .finish()
    }
}

impl Default for CrewGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CrewGenerator {
    pub fn new() -> Self {
        Self {
            strategy: CandidateStrategy::default(),
            pool_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(POOL_CACHE_CAPACITY).expect("64 > 0"),
            )),
        }
    }

    pub fn with_strategy(strategy: CandidateStrategy) -> Self {
        Self {
            strategy,
            pool_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(POOL_CACHE_CAPACITY).expect("64 > 0"),
            )),
        }
    }

    /// Retrieve narrowed pools from the cache, or build + cache them when this is the first call
    /// with the current strategy/roster/constraint combination.
    fn get_or_build_pools(
        &self,
        from_registry: bool,
        registry: Option<&DataRegistry>,
        profile_id: Option<&str>,
    ) -> Option<OfficerPools> {
        let cache_key = OfficerPoolCacheKey {
            only_below_decks_with_ability: self.strategy.only_below_decks_with_ability,
            below_decks_slots: self.strategy.below_decks_slots,
            roster_profile_id: self.strategy.roster_profile_id.clone().unwrap_or_default(),
            constraints_fingerprint: constraints_fingerprint(self.strategy.constraints.as_ref()),
            from_registry,
        };

        // Fast path: cache hit — clone unshuffled pools.
        {
            let mut cache = self.pool_cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }

        // Slow path: build pools outside the lock (avoids holding lock across disk I/O).
        let result = if from_registry {
            build_officer_pools_from_registry(
                registry?,
                self.strategy.only_below_decks_with_ability,
                profile_id,
                self.strategy.below_decks_slots,
                self.strategy.constraints.as_ref(),
            )
        } else {
            build_officer_pools_with_constraints(
                self.strategy.only_below_decks_with_ability,
                self.strategy.below_decks_slots,
                self.strategy.roster_profile_id.as_deref(),
                self.strategy.constraints.as_ref(),
            )
        };

        self.pool_cache
            .lock()
            .unwrap()
            .put(cache_key, result.clone());
        result
    }

    pub fn generate_candidates(&self, ship: &str, hostile: &str, seed: u64) -> Vec<CrewCandidate> {
        let mut pools = match self.get_or_build_pools(false, None, None) {
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
        let mut pools = match self.get_or_build_pools(true, Some(registry), profile_id) {
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
                hostile,
                ship,
            )
        };
        perf_log::log_duration("crew_generator.generate_candidates_from_pools", t0);
        out
    }

    /// Returns the number of crew combinations without allocating candidates.
    /// Used for estimate when no cap is set. Uses same exhaustive/sampled branch as generate_candidates.
    pub fn count_candidates(&self, ship: &str, hostile: &str, seed: u64) -> usize {
        let mut pools = match self.get_or_build_pools(false, None, None) {
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
        let mut pools = match self.get_or_build_pools(true, Some(registry), profile_id) {
            Some(p) => p,
            None => return 0,
        };
        self.count_candidates_from_pools(&mut pools, ship, hostile, seed)
    }

    /// How many candidates [`Self::generate_candidates_from_registry`] would produce for this
    /// strategy (including `max_candidates` early-stop). Used for optimize auto-routing.
    pub fn effective_run_candidate_count_from_registry(
        &self,
        registry: &DataRegistry,
        ship: &str,
        hostile: &str,
        seed: u64,
        profile_id: Option<&str>,
    ) -> usize {
        let mut pools = match self.get_or_build_pools(true, Some(registry), profile_id) {
            Some(p) => p,
            None => return 0,
        };
        self.effective_run_candidate_count_from_pools(&mut pools, ship, hostile, seed)
    }

    fn effective_run_candidate_count_from_pools(
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
        let max_c = self.strategy.max_candidates;
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_count(&pools.captains, &pools.bridge, &pools.below_decks, max_c, k)
        } else {
            sampled_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                max_c,
                k,
            )
        }
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
    hostile: &str,
    ship: &str,
) -> Vec<CrewCandidate> {
    use crate::optimizer::officer_learning::RngExt as LearningRngExt;

    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let reserve = strategy.max_candidates.unwrap_or(256).min(4096);
    let mut candidates = Vec::with_capacity(reserve);

    // RNG adapter for officer_learning::RngExt
    struct CgRng {
        state: u64,
    }
    impl LearningRngExt for CgRng {
        fn index(&mut self, n: usize) -> usize {
            if n == 0 {
                return 0;
            }
            self.state = self
                .state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.state as usize) % n
        }
        fn next_f64(&mut self) -> f64 {
            self.state = self
                .state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.state >> 11) as f64 / (u64::MAX as f64 + 1.0)
        }
    }
    let mut rng = CgRng { state: seed };

    let use_learning = strategy.learned_officer_scores.is_some();

    for captain in captains.iter().take(captain_limit) {
        for (bi, b1) in bridge.iter().take(bridge_limit).enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().take(bridge_limit).skip(bi + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }

                let below_indices: Vec<usize> = if use_learning {
                    // Weighted sampling: select non-conflicting below-decks officers
                    let available: Vec<String> = below_decks
                        .iter()
                        .enumerate()
                        .filter_map(|(_i, name)| {
                            if !name_conflicts_bridge_captain(name.as_str(), captain, b1, b2) {
                                Some(name.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if available.len() < below_decks_slots {
                        continue;
                    }
                    // Map back to original below_decks indices
                    let selected = strategy
                        .learned_officer_scores
                        .as_ref()
                        .expect("scores set when use_learning is true")
                        .epsilon_greedy_sample(
                            &available,
                            below_decks_slots,
                            hostile,
                            ship,
                            &mut rng,
                        );
                    // Convert selected names back to original below_decks indices
                    selected
                        .iter()
                        .map(|&si| {
                            let name = &available[si];
                            below_decks
                                .iter()
                                .position(|n| n == name)
                                .expect("name from available is in below_decks")
                        })
                        .collect()
                } else {
                    // Legacy stride-based sampling
                    let stride = ((seed as usize) % 5) + 1;
                    (0..below_decks.len())
                        .step_by(stride)
                        .filter(|&i| {
                            !name_conflicts_bridge_captain(below_decks[i].as_str(), captain, b1, b2)
                        })
                        .collect()
                };

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
                    count += 1;
                    if max_count.is_some_and(|cap| count >= cap) {
                        stop = true;
                    }
                });
                if let Some(cap) = max_count {
                    if count >= cap {
                        return cap;
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
        build_officer_pools_from_registry, narrow_officer_pools_for_constraints,
        resolve_below_decks_slots, CandidateStrategy, CrewGenerator, CrewSlotUnlock,
        MAX_BELOW_DECKS_SLOTS,
    };
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::constraints::{CrewSearchConstraints, OfficerGroupConstraint};

    #[test]
    fn narrow_pools_returns_none_for_unknown_captain_must_be() {
        let registry = DataRegistry::load().expect("registry");
        let pools = build_officer_pools_from_registry(
            &registry,
            false,
            Some(super::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            3,
            None,
        )
        .expect("pools");
        let c = CrewSearchConstraints {
            captain_must_be: vec!["NotARealCaptainNameForKobayashiTest".into()],
            ..Default::default()
        };
        assert!(
            narrow_officer_pools_for_constraints(registry.officer_index(), pools, &c, 3).is_none()
        );
    }

    #[test]
    fn narrow_pools_returns_none_when_group_min_count_unreachable() {
        let registry = DataRegistry::load().expect("registry");
        let pools = build_officer_pools_from_registry(
            &registry,
            false,
            Some(super::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            3,
            None,
        )
        .expect("pools");
        let one_officer = registry.officers()[0].name.clone();
        let c = CrewSearchConstraints {
            groups: vec![OfficerGroupConstraint {
                officers: vec![one_officer],
                min_count: 99,
            }],
            ..Default::default()
        };
        assert!(
            narrow_officer_pools_for_constraints(registry.officer_index(), pools, &c, 3).is_none()
        );
    }

    #[test]
    fn resolve_below_decks_uses_explicit_or_tier_default() {
        assert_eq!(resolve_below_decks_slots(None, None, None, Some(4)), 4);
        assert_eq!(resolve_below_decks_slots(Some(1), None, None, None), 2);
        assert_eq!(resolve_below_decks_slots(Some(2), None, None, None), 3);
        assert_eq!(resolve_below_decks_slots(None, None, None, None), 3);
        assert_eq!(
            resolve_below_decks_slots(None, None, None, Some(99)),
            MAX_BELOW_DECKS_SLOTS
        );
        assert_eq!(resolve_below_decks_slots(None, None, None, Some(0)), 0);
        assert_eq!(resolve_below_decks_slots(None, None, None, Some(1)), 1);
    }

    #[test]
    fn resolve_below_decks_uses_crew_slot_schedule_and_level() {
        let sched = [
            CrewSlotUnlock {
                slots: Some("1".into()),
                unlock_level: 5,
            },
            CrewSlotUnlock {
                slots: Some("2".into()),
                unlock_level: 10,
            },
            CrewSlotUnlock {
                slots: Some("3".into()),
                unlock_level: 20,
            },
            CrewSlotUnlock {
                slots: Some("4".into()),
                unlock_level: 30,
            },
        ];
        assert_eq!(
            resolve_below_decks_slots(None, Some(4), Some(&sched), None),
            0
        );
        assert_eq!(
            resolve_below_decks_slots(None, Some(5), Some(&sched), None),
            1
        );
        assert_eq!(
            resolve_below_decks_slots(None, Some(30), Some(&sched), None),
            4
        );
    }

    #[test]
    fn generation_is_deterministic_for_same_seed() {
        let generator = CrewGenerator::with_strategy(CandidateStrategy {
            max_candidates: Some(32),
            roster_profile_id: Some(super::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS.into()),
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
            roster_profile_id: Some(super::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS.into()),
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
            roster_profile_id: Some(super::NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS.into()),
            ..CandidateStrategy::default()
        });
        let candidates = generator.generate_candidates("defiant", "romulan", 11);
        assert!(!candidates.is_empty());
        for c in &candidates {
            assert_eq!(c.below_decks.len(), 2, "{c:?}");
        }
    }
}
