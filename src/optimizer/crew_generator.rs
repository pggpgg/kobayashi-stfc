use crate::data::data_registry::DataRegistry;
use crate::perf_log;
use crate::data::import::load_imported_roster_ids_unlocked_only;
use crate::data::profile_index::{profile_path, resolve_profile_id_for_api, ROSTER_IMPORTED};
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
) -> Option<OfficerPools> {
    let officers: Vec<Officer> = registry
        .officers()
        .iter()
        .filter(|o| !o.name.trim().is_empty())
        .cloned()
        .collect();

    const MIN_OFFICERS: usize = 1 + BRIDGE_SLOTS + BELOW_DECKS_SLOTS;
    let mut officers = officers;
    let roster_path = profile_path(&resolve_profile_id_for_api(profile_id), ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
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

    if only_below_decks_with_ability {
        below_decks = officers
            .iter()
            .filter(|officer| {
                can_fill_position(officer, Position::BelowDecks)
                    && has_below_decks_ability(officer)
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

    if captains.is_empty() || bridge.len() < BRIDGE_SLOTS || below_decks.len() < BELOW_DECKS_SLOTS {
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
pub fn build_officer_pools(only_below_decks_with_ability: bool) -> Option<OfficerPools> {
    let mut officers = load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
        .map(|loaded| {
            loaded
                .into_iter()
                .filter(|officer| !officer.name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    const MIN_OFFICERS: usize = 1 + BRIDGE_SLOTS + BELOW_DECKS_SLOTS;
    let roster_path = profile_path(&resolve_profile_id_for_api(None), ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();
    if let Some(roster_ids) = load_imported_roster_ids_unlocked_only(&roster_path) {
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

    if only_below_decks_with_ability {
        below_decks = officers
            .iter()
            .filter(|officer| {
                can_fill_position(officer, Position::BelowDecks)
                    && has_below_decks_ability(officer)
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
    /// When true, below-decks pool only includes officers that have a below-decks ability.
    pub only_below_decks_with_ability: bool,
    /// When true, the same officer may appear in multiple seats (non-canonical; compatibility / bug repro).
    pub allow_duplicate_officers: bool,
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
            allow_duplicate_officers: false,
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
        let mut pools = match build_officer_pools(self.strategy.only_below_decks_with_ability) {
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
        let out = if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                self.strategy.max_candidates,
                self.strategy.allow_duplicate_officers,
            )
        } else {
            sampled_candidates(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                self.strategy.allow_duplicate_officers,
            )
        };
        perf_log::log_duration("crew_generator.generate_candidates_from_pools", t0);
        out
    }

    /// Returns the number of crew combinations without allocating candidates.
    /// Used for estimate when no cap is set. Uses same exhaustive/sampled branch as generate_candidates.
    pub fn count_candidates(&self, ship: &str, hostile: &str, seed: u64) -> usize {
        let mut pools = match build_officer_pools(self.strategy.only_below_decks_with_ability) {
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
        if min_pool <= self.strategy.exhaustive_pool_threshold {
            exhaustive_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                None,
                self.strategy.allow_duplicate_officers,
            )
        } else {
            sampled_count(
                &pools.captains,
                &pools.bridge,
                &pools.below_decks,
                &self.strategy,
                mix_seed(seed ^ 0xA5A5_A5A5_A5A5_A5A5, ship, hostile),
                None,
                self.strategy.allow_duplicate_officers,
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

/// True if `d3` is already used in captain, bridge, or first two below-decks picks.
#[inline]
fn below_third_conflicts(
    d3: &str,
    captain: &str,
    b1: &str,
    b2: &str,
    d1: &str,
    d2: &str,
) -> bool {
    d3 == captain || d3 == b1 || d3 == b2 || d3 == d1 || d3 == d2
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
    allow_duplicate_officers: bool,
) -> Vec<CrewCandidate> {
    if allow_duplicate_officers {
        return exhaustive_candidates_allow_duplicates(captains, bridge, below_decks, max_candidates);
    }
    let reserve = max_candidates.unwrap_or(256).min(4096);
    let mut candidates = Vec::with_capacity(reserve);

    for captain in captains {
        for (i, b1) in bridge.iter().enumerate() {
            if b1 == captain {
                continue;
            }
            for b2 in bridge.iter().skip(i + 1) {
                if b2 == captain || b2 == b1 {
                    continue;
                }
                for (di, d1) in below_decks.iter().enumerate() {
                    if name_conflicts_bridge_captain(d1, captain, b1, b2) {
                        continue;
                    }
                    for (dj, d2) in below_decks.iter().enumerate().skip(di + 1) {
                        if name_conflicts_bridge_captain(d2, captain, b1, b2) || d2 == d1 {
                            continue;
                        }
                        for d3 in below_decks.iter().skip(dj + 1) {
                            if below_third_conflicts(d3, captain, b1, b2, d1, d2) {
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

fn exhaustive_candidates_allow_duplicates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    max_candidates: Option<usize>,
) -> Vec<CrewCandidate> {
    let reserve = max_candidates.unwrap_or(256).min(4096);
    let mut candidates = Vec::with_capacity(reserve);
    for captain in captains {
        for b1 in bridge {
            for b2 in bridge {
                for d1 in below_decks {
                    for d2 in below_decks {
                        for d3 in below_decks {
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
    allow_duplicate_officers: bool,
) -> usize {
    if allow_duplicate_officers {
        return exhaustive_count_allow_duplicates(captains, bridge, below_decks, max_count);
    }
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
                for (di, d1) in below_decks.iter().enumerate() {
                    if name_conflicts_bridge_captain(d1, captain, b1, b2) {
                        continue;
                    }
                    for (dj, d2) in below_decks.iter().enumerate().skip(di + 1) {
                        if name_conflicts_bridge_captain(d2, captain, b1, b2) || d2 == d1 {
                            continue;
                        }
                        for d3 in below_decks.iter().skip(dj + 1) {
                            if below_third_conflicts(d3, captain, b1, b2, d1, d2) {
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

fn exhaustive_count_allow_duplicates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    max_count: Option<usize>,
) -> usize {
    const ESTIMATE_CAP: usize = 2_000_000;
    let mut count = 0_usize;
    for _captain in captains {
        for _b1 in bridge {
            for _b2 in bridge {
                for _d1 in below_decks {
                    for _d2 in below_decks {
                        for _d3 in below_decks {
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
    allow_duplicate_officers: bool,
) -> Vec<CrewCandidate> {
    if allow_duplicate_officers {
        return sampled_candidates_allow_duplicates(captains, bridge, below_decks, strategy, seed);
    }
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
                for (ii, &di) in below_indices.iter().enumerate() {
                    let d1 = &below_decks[di];
                    for &dj in below_indices.iter().skip(ii + 1) {
                        let d2 = &below_decks[dj];
                        if d2 == d1 || name_conflicts_bridge_captain(d2, captain, b1, b2) {
                            continue;
                        }
                        for &dk in below_indices.iter().skip(ii + 2) {
                            let d3 = &below_decks[dk];
                            if below_third_conflicts(d3, captain, b1, b2, d1, d2) {
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

fn sampled_candidates_allow_duplicates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    strategy: &CandidateStrategy,
    _seed: u64,
) -> Vec<CrewCandidate> {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let below_cap = below_decks
        .len()
        .min(bridge_limit.max(BELOW_DECKS_SLOTS))
        .max(BELOW_DECKS_SLOTS);
    let mut candidates = Vec::new();
    for captain in captains.iter().take(captain_limit) {
        let br: Vec<&String> = bridge.iter().take(bridge_limit).collect();
        let bd: Vec<&String> = below_decks.iter().take(below_cap).collect();
        if br.len() < 2 || bd.len() < BELOW_DECKS_SLOTS {
            continue;
        }
        for b1 in &br {
            for b2 in &br {
                for d1 in &bd {
                    for d2 in &bd {
                        for d3 in &bd {
                            candidates.push(CrewCandidate {
                                captain: (*captain).clone(),
                                bridge: vec![(*b1).clone(), (*b2).clone()],
                                below_decks: vec![(*d1).clone(), (*d2).clone(), (*d3).clone()],
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
    allow_duplicate_officers: bool,
) -> usize {
    if allow_duplicate_officers {
        return sampled_count_allow_duplicates(captains, bridge, below_decks, strategy, seed, max_count);
    }
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
                for (ii, &di) in below_indices.iter().enumerate() {
                    let d1 = &below_decks[di];
                    for &dj in below_indices.iter().skip(ii + 1) {
                        let d2 = &below_decks[dj];
                        if d2 == d1 || name_conflicts_bridge_captain(d2, captain, b1, b2) {
                            continue;
                        }
                        for &dk in below_indices.iter().skip(ii + 2) {
                            let d3 = &below_decks[dk];
                            if below_third_conflicts(d3, captain, b1, b2, d1, d2) {
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

fn sampled_count_allow_duplicates(
    captains: &[String],
    bridge: &[String],
    below_decks: &[String],
    strategy: &CandidateStrategy,
    _seed: u64,
    max_count: Option<usize>,
) -> usize {
    let captain_limit = strategy.large_pool_captain_limit.max(1).min(captains.len());
    let bridge_limit = strategy.large_pool_bridge_limit.max(2).min(bridge.len());
    let below_cap = below_decks
        .len()
        .min(bridge_limit.max(BELOW_DECKS_SLOTS))
        .max(BELOW_DECKS_SLOTS);
    const ESTIMATE_CAP: usize = 2_000_000;
    let mut count = 0_usize;
    for _captain in captains.iter().take(captain_limit) {
        let br_len = bridge.iter().take(bridge_limit).count();
        let bd_len = below_decks.iter().take(below_cap).count();
        if br_len < 2 || bd_len < BELOW_DECKS_SLOTS {
            continue;
        }
        count += br_len * br_len * bd_len * bd_len * bd_len;
        if let Some(cap) = max_count {
            if count >= cap {
                return count;
            }
        }
        if count >= ESTIMATE_CAP {
            return ESTIMATE_CAP;
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

    #[test]
    fn allow_duplicate_officers_increases_exhaustive_count() {
        let captains = ["A".to_string()];
        let bridge = ["B".to_string(), "C".to_string()];
        let below = ["D".to_string(), "E".to_string(), "F".to_string()];
        let unique = super::exhaustive_count(&captains, &bridge, &below, None, false);
        let dup = super::exhaustive_count(&captains, &bridge, &below, None, true);
        assert!(dup > unique, "unique={unique} dup={dup}");
    }
}
