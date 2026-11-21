//! Bounded Quality-Diversity (MAP-Elites) archive over combat behavioral cells.
//!
//! Tracks the highest-scoring crew inside each discrete behavioral cell
//! `(captain_group, speed_bucket, survival_bucket)`. During portfolio search, the archive prevents
//! confirmation slots from collapsing onto slight below-decks perturbations of a single captain
//! archetype while preserving global top performers.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::data::data_registry::DataRegistry;
use crate::optimizer::constraints::normalize_officer_name;
use crate::optimizer::monte_carlo::{crew_candidate_stable_hash, SimulationResult};

/// Coarse fight-speed regime derived from Monte Carlo win / round-1 kill / stall rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpeedBucket {
    R1Kill,
    Fast,
    Attrition,
}

impl SpeedBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            SpeedBucket::R1Kill => "r1_kill",
            SpeedBucket::Fast => "fast",
            SpeedBucket::Attrition => "attrition",
        }
    }

    pub fn from_result(r: &SimulationResult) -> Self {
        if r.r1_kill_rate >= 0.50 {
            SpeedBucket::R1Kill
        } else if r.win_rate >= 0.50 && r.stall_rate <= 0.10 {
            SpeedBucket::Fast
        } else {
            SpeedBucket::Attrition
        }
    }
}

/// Coarse surviving-hull regime derived from Monte Carlo mean surviving hull fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurvivalBucket {
    HighHull,
    MidHull,
    LowHull,
}

impl SurvivalBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            SurvivalBucket::HighHull => "high_hull",
            SurvivalBucket::MidHull => "mid_hull",
            SurvivalBucket::LowHull => "low_hull",
        }
    }

    pub fn from_result(r: &SimulationResult) -> Self {
        if r.avg_hull_remaining >= 0.70 {
            SurvivalBucket::HighHull
        } else if r.avg_hull_remaining >= 0.35 {
            SurvivalBucket::MidHull
        } else {
            SurvivalBucket::LowHull
        }
    }
}

/// Behavioral descriptor key for one MAP-Elites cell.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BehaviorCell {
    pub captain_group: String,
    pub speed: SpeedBucket,
    pub survival: SurvivalBucket,
}

impl BehaviorCell {
    pub fn from_result(r: &SimulationResult, captain_group: String) -> Self {
        Self {
            captain_group,
            speed: SpeedBucket::from_result(r),
            survival: SurvivalBucket::from_result(r),
        }
    }

    pub fn label(&self) -> String {
        format!(
            "{}|{}|{}",
            self.captain_group,
            self.speed.as_str(),
            self.survival.as_str()
        )
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveElite {
    pub hash: u64,
    pub fitness: f64,
    pub result: SimulationResult,
}

/// Fitness proxy aligned with [`crate::optimizer::ranking::rank_results`].
#[inline]
fn archive_fitness(r: &SimulationResult) -> f64 {
    if r.chain.is_some() {
        r.win_rate * 1e4 + r.avg_hull_remaining.min(1.0)
    } else {
        r.win_rate * 0.8 + r.avg_hull_remaining * 0.2
    }
}

/// Build a lookup from normalized officer name to normalized captain-group slug.
pub fn captain_group_lookup_from_registry(registry: &DataRegistry) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(lcars) = registry.lcars_officers() {
        for officer in lcars {
            let key = normalize_officer_name(&officer.name);
            if key.is_empty() {
                continue;
            }
            if let Some(group) = officer.group.as_deref() {
                let g = normalize_group_slug(group);
                if !g.is_empty() {
                    map.insert(key, g);
                }
            }
        }
    }
    for officer in registry.officers() {
        let key = normalize_officer_name(&officer.name);
        if key.is_empty() || map.contains_key(&key) {
            continue;
        }
        if let Some(group) = officer.group.as_deref() {
            let g = normalize_group_slug(group);
            if !g.is_empty() {
                map.insert(key, g);
            }
        }
    }
    map
}

fn normalize_group_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

pub fn resolve_captain_group(captain_name: &str, group_lookup: &HashMap<String, String>) -> String {
    let norm = normalize_officer_name(captain_name);
    group_lookup.get(&norm).cloned().unwrap_or_else(|| {
        let fallback = normalize_group_slug(captain_name);
        if fallback.is_empty() {
            "unknown".to_string()
        } else {
            fallback
        }
    })
}

/// Deterministic MAP-Elites archive keyed by [`BehaviorCell`].
#[derive(Debug, Clone, Default)]
pub struct QualityDiversityArchive {
    cells: BTreeMap<BehaviorCell, ArchiveElite>,
    max_cells: usize,
}

impl QualityDiversityArchive {
    pub fn new(max_cells: usize) -> Self {
        Self {
            cells: BTreeMap::new(),
            max_cells: max_cells.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Insert `result` into its behavioral cell if empty or if it strictly beats the incumbent.
    /// Returns the cell label assigned to this crew.
    pub fn insert(
        &mut self,
        result: &SimulationResult,
        group_lookup: &HashMap<String, String>,
    ) -> String {
        let group = resolve_captain_group(&result.candidate.captain, group_lookup);
        let cell = BehaviorCell::from_result(result, group);
        let label = cell.label();
        let fitness = archive_fitness(result);
        let hash = crew_candidate_stable_hash(&result.candidate);

        let replace = match self.cells.get(&cell) {
            None => true,
            Some(incumbent) => {
                fitness > incumbent.fitness
                    || ((fitness - incumbent.fitness).abs() <= f64::EPSILON
                        && hash < incumbent.hash)
            }
        };

        if replace {
            let inserting_new = !self.cells.contains_key(&cell);
            if inserting_new && self.cells.len() >= self.max_cells {
                // Evict the lowest-fitness cell only if the new crew beats it.
                if let Some((worst_key, worst_fit)) = self
                    .cells
                    .iter()
                    .min_by(|a, b| {
                        a.1.fitness
                            .total_cmp(&b.1.fitness)
                            .then_with(|| b.1.hash.cmp(&a.1.hash))
                    })
                    .map(|(k, v)| (k.clone(), v.fitness))
                {
                    if fitness > worst_fit {
                        self.cells.remove(&worst_key);
                        self.cells.insert(
                            cell,
                            ArchiveElite {
                                hash,
                                fitness,
                                result: result.clone(),
                            },
                        );
                    }
                }
            } else {
                self.cells.insert(
                    cell,
                    ArchiveElite {
                        hash,
                        fitness,
                        result: result.clone(),
                    },
                );
            }
        }
        label
    }

    /// Promote up to `total_top_k` crews for independent confirmation:
    /// - First take the global best `(total_top_k - diverse_quota).max(1)` crews from `sorted_raced`.
    /// - Then add up to `diverse_quota` archive elites (preferring uncovered captain groups first,
    ///   then highest fitness among remaining cells) whose fitness is competitive (within 50% of
    ///   the leader or any non-zero win rate).
    /// - Finally top up any remaining slots from `sorted_raced`.
    pub fn promote_for_confirmation(
        &self,
        sorted_raced: &[SimulationResult],
        total_top_k: usize,
        diverse_quota: usize,
        group_lookup: &HashMap<String, String>,
    ) -> (Vec<SimulationResult>, HashSet<u64>) {
        let cap = total_top_k.min(sorted_raced.len());
        if cap == 0 {
            return (Vec::new(), HashSet::new());
        }
        let div_slots = diverse_quota.min(cap.saturating_sub(1));
        let global_slots = cap - div_slots;

        let mut selected: Vec<SimulationResult> = Vec::with_capacity(cap);
        let mut seen_hashes: HashSet<u64> = HashSet::with_capacity(cap);
        let mut covered_groups: HashSet<String> = HashSet::new();
        let mut archive_promoted_hashes: HashSet<u64> = HashSet::new();

        for r in sorted_raced.iter().take(global_slots) {
            let h = crew_candidate_stable_hash(&r.candidate);
            if seen_hashes.insert(h) {
                covered_groups.insert(resolve_captain_group(&r.candidate.captain, group_lookup));
                selected.push(r.clone());
            }
        }

        if div_slots > 0 && !self.cells.is_empty() {
            let leader_fit = sorted_raced.first().map(archive_fitness).unwrap_or(0.0);
            let min_competitive_fit = leader_fit * 0.55;

            let mut elites: Vec<(&BehaviorCell, &ArchiveElite)> = self.cells.iter().collect();
            elites.sort_by(|a, b| {
                b.1.fitness
                    .total_cmp(&a.1.fitness)
                    .then_with(|| a.1.hash.cmp(&b.1.hash))
            });

            // Pass 1: distinct captain groups not yet covered by global slots.
            for (cell, elite) in &elites {
                if selected.len() >= cap {
                    break;
                }
                if elite.fitness < min_competitive_fit {
                    continue;
                }
                if !covered_groups.contains(&cell.captain_group) && seen_hashes.insert(elite.hash) {
                    covered_groups.insert(cell.captain_group.clone());
                    archive_promoted_hashes.insert(elite.hash);
                    selected.push(elite.result.clone());
                }
            }

            // Pass 2: remaining distinct behavioral cells (e.g., faster speed or higher hull).
            for (_, elite) in &elites {
                if selected.len() >= cap {
                    break;
                }
                if elite.fitness < min_competitive_fit {
                    continue;
                }
                if seen_hashes.insert(elite.hash) {
                    archive_promoted_hashes.insert(elite.hash);
                    selected.push(elite.result.clone());
                }
            }
        }

        // Fill any remaining slots from global ranked list.
        for r in sorted_raced {
            if selected.len() >= cap {
                break;
            }
            let h = crew_candidate_stable_hash(&r.candidate);
            if seen_hashes.insert(h) {
                selected.push(r.clone());
            }
        }

        (selected, archive_promoted_hashes)
    }

    /// Map every stored elite hash to its `"group|speed|survival"` cell label.
    pub fn elite_cell_labels(&self) -> HashMap<u64, String> {
        self.cells
            .iter()
            .map(|(cell, elite)| (elite.hash, cell.label()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::crew_generator::CrewCandidate;

    fn mock_result(captain: &str, win_rate: f64, hull: f64, r1: f64) -> SimulationResult {
        SimulationResult {
            candidate: CrewCandidate {
                captain: captain.to_string(),
                bridge: vec!["B1".to_string(), "B2".to_string()],
                below_decks: vec!["D1".to_string()],
            },
            unsupported_crew_health_effects: None,
            trials_run: 64,
            win_rate,
            win_rate_ci_low: (win_rate - 0.1).clamp(0.0, 1.0),
            win_rate_ci_high: (win_rate + 0.1).clamp(0.0, 1.0),
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.05,
            loss_rate: 1.0 - win_rate,
            loss_rate_ci_low: 0.0,
            loss_rate_ci_high: 1.0,
            r1_kill_rate: r1,
            r1_kill_rate_ci_low: 0.0,
            r1_kill_rate_ci_high: 1.0,
            avg_hull_remaining: hull,
            avg_hull_remaining_ci_low: (hull - 0.05).clamp(0.0, 1.0),
            avg_hull_remaining_ci_high: (hull + 0.05).clamp(0.0, 1.0),
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            chain: None,
            expected_hull_damage: None,
        }
    }

    #[test]
    fn archive_retains_best_per_cell_and_promotes_diverse_captain_groups() {
        let mut groups = HashMap::new();
        groups.insert("kirk".to_string(), "tos".to_string());
        groups.insert("pike".to_string(), "snw".to_string());
        groups.insert("janeway".to_string(), "voyager".to_string());

        let r_kirk1 = mock_result("Kirk", 0.92, 0.80, 0.10);
        let mut r_kirk2 = mock_result("Kirk", 0.90, 0.78, 0.10);
        r_kirk2.candidate.below_decks = vec!["D2".to_string()];
        let r_pike = mock_result("Pike", 0.85, 0.75, 0.60);
        let r_janeway = mock_result("Janeway", 0.82, 0.82, 0.05);

        let mut archive = QualityDiversityArchive::new(64);
        assert_eq!(archive.insert(&r_kirk1, &groups), "tos|fast|high_hull");
        assert_eq!(archive.insert(&r_kirk2, &groups), "tos|fast|high_hull");
        assert_eq!(archive.insert(&r_pike, &groups), "snw|r1_kill|high_hull");
        assert_eq!(
            archive.insert(&r_janeway, &groups),
            "voyager|fast|high_hull"
        );
        assert_eq!(archive.len(), 3);

        let sorted = vec![
            r_kirk1.clone(),
            r_kirk2.clone(),
            r_pike.clone(),
            r_janeway.clone(),
        ];
        let (promoted, archive_hashes) = archive.promote_for_confirmation(&sorted, 3, 2, &groups);
        assert_eq!(promoted.len(), 3);
        let captains: HashSet<&str> = promoted
            .iter()
            .map(|r| r.candidate.captain.as_str())
            .collect();
        // Global top 1 is Kirk; diverse quota pulls in Pike & Janeway instead of Kirk #2!
        assert!(captains.contains("Kirk"));
        assert!(captains.contains("Pike"));
        assert!(captains.contains("Janeway"));
        assert_eq!(archive_hashes.len(), 2);
    }
}
