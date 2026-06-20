//! Aggregate implemented / partial / ignored mechanics across LCARS, ship hull abilities, and hostile catalogs.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::combat::abilities::TimingWindow;
use crate::data::data_registry::DataRegistry;
use crate::data::hostile_ability_resolve::{
    collect_upstream_hostile_ability_ids, hostile_ability_effect_from_catalog,
    load_hostile_ability_catalog, HostileAbilityCatalogEntry, DEFAULT_HOSTILE_ABILITY_CATALOG_PATH,
};
use crate::data::ship::{load_extended_ship_record, ExtendedShipIndex, ShipAbility};
use crate::data::ship_ability_resolve::{
    parse_ship_ability_timing, ship_ability_effect_from_catalog,
};
use crate::lcars::{
    lcars_effect_coverage, LcarsEffectCoverage, MechanicCoverageTier, ResolveOptions,
};
const DEFAULT_SHIPS_EXTENDED_DIR: &str = "data/ships_extended";

#[derive(Debug, Default, Clone, Serialize)]
pub struct TierCounts {
    pub implemented: u32,
    pub partial: u32,
    pub ignored: u32,
}

impl TierCounts {
    fn add_tier(&mut self, tier: MechanicCoverageTier) {
        match tier {
            MechanicCoverageTier::Implemented => self.implemented += 1,
            MechanicCoverageTier::Partial => self.partial += 1,
            MechanicCoverageTier::Ignored => self.ignored += 1,
        }
    }

    fn gap_total(&self) -> u32 {
        self.ignored + self.partial
    }
}

/// One row in the ordered fidelity backlog (from [`build_fidelity_backlog`]).
#[derive(Debug, Clone, Serialize)]
pub struct FidelityBacklogItem {
    /// 1-based priority: lower = higher impact gaps first (more ignored, then more partial).
    pub rank: u32,
    /// `lcars`, `ship_hull_abilities`, or `hostile_ability_catalog`.
    pub area: &'static str,
    /// LCARS `effect_type` (lowercased) or `_aggregate` for rolled-up areas.
    pub key: String,
    pub ignored: u32,
    pub partial: u32,
    pub implemented: u32,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MechanicsCoverageReport {
    pub status: &'static str,
    pub lcars_officers_files: u32,
    pub lcars_effects: TierCounts,
    pub ship_hull_abilities: TierCounts,
    pub ships_with_abilities_scanned: u32,
    pub hostile_catalog_entries: TierCounts,
    pub hostile_catalog_entry_count: u32,
    /// Unique upstream hostile ability ids in cached stfc.space JSON.
    pub hostile_upstream_unique_ability_ids: u32,
    /// Catalog rows with `effect_type` other than combat_noop.
    pub hostile_catalog_modeled_count: u32,
    /// Catalog rows explicitly marked combat_noop.
    pub hostile_catalog_noop_count: u32,
    /// Upstream ability ids missing from the catalog (should stay 0 after regen).
    pub hostile_upstream_ids_missing_from_catalog: u32,
    /// Counts by LCARS `effect_type` string (lowercased).
    pub lcars_by_effect_type: HashMap<String, TierCounts>,
    /// Sample of ignored LCARS pathways (capped) for debugging.
    pub lcars_ignored_samples: Vec<String>,
    /// Ordered combat / LCARS gap list for fidelity work (same data as above; sortable backlog).
    pub fidelity_backlog: Vec<FidelityBacklogItem>,
    pub notes: Vec<String>,
}

fn root_dir() -> &'static Path {
    crate::runtime_paths::asset_root()
}

fn bump(map: &mut HashMap<String, TierCounts>, key: &str, tier: MechanicCoverageTier) {
    map.entry(key.to_string()).or_default().add_tier(tier);
}

fn classify_ship_hull_ability(a: &ShipAbility) -> LcarsEffectCoverage {
    let timing = match parse_ship_ability_timing(&a.timing) {
        Some(t) => t,
        None => {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Ignored,
                pathway: format!("unknown_timing:{}", a.timing),
            };
        }
    };

    let et = a.effect_type.trim().to_lowercase().replace('-', "_");
    match et.as_str() {
        "accuracy" | "accuracy_bonus" => {
            if timing == TimingWindow::CombatBegin {
                LcarsEffectCoverage {
                    tier: MechanicCoverageTier::Implemented,
                    pathway: "combat_begin_accuracy".to_string(),
                }
            } else {
                LcarsEffectCoverage {
                    tier: MechanicCoverageTier::Partial,
                    pathway: "accuracy_non_combat_begin".to_string(),
                }
            }
        }
        "combat_noop" | "unmodeled" | "not_applicable" => LcarsEffectCoverage {
            tier: MechanicCoverageTier::Partial,
            pathway: "catalogued_noop".to_string(),
        },
        _ => {
            if ship_ability_effect_from_catalog(&a.effect_type, timing, a.value, a.duration_rounds)
                .is_some()
            {
                LcarsEffectCoverage {
                    tier: MechanicCoverageTier::Implemented,
                    pathway: "hull_ability_seat".to_string(),
                }
            } else {
                LcarsEffectCoverage {
                    tier: MechanicCoverageTier::Partial,
                    pathway: "effect_or_timing_unsupported".to_string(),
                }
            }
        }
    }
}

fn classify_hostile_catalog_entry(entry: &HostileAbilityCatalogEntry) -> LcarsEffectCoverage {
    let timing = match parse_ship_ability_timing(&entry.timing) {
        Some(t) => t,
        None => {
            return LcarsEffectCoverage {
                tier: MechanicCoverageTier::Ignored,
                pathway: format!("unknown_timing:{}", entry.timing),
            };
        }
    };
    let v = entry.value_override.unwrap_or(0.15);
    let chance = 50.0_f64;
    if hostile_ability_effect_from_catalog(
        &entry.effect_type,
        timing,
        chance,
        v,
        entry.duration_rounds,
        entry.round_interval,
        entry.shots,
        entry.weapon_index,
    )
    .is_some()
    {
        LcarsEffectCoverage {
            tier: MechanicCoverageTier::Implemented,
            pathway: "defender_return_fire".to_string(),
        }
    } else {
        let et = entry.effect_type.trim().to_lowercase();
        match et.as_str() {
            "combat_noop" | "unmodeled" | "not_applicable" => LcarsEffectCoverage {
                tier: MechanicCoverageTier::Partial,
                pathway: "catalogued_noop".to_string(),
            },
            _ => LcarsEffectCoverage {
                tier: MechanicCoverageTier::Partial,
                pathway: "hostile_effect_not_mapped".to_string(),
            },
        }
    }
}

fn area_sort_key(area: &str) -> u8 {
    match area {
        "lcars" => 0,
        "ship_hull_abilities" => 1,
        "hostile_ability_catalog" => 2,
        _ => 3,
    }
}

/// Build a single ordered backlog: LCARS effect types with gaps first (by ignored, then partial),
/// then aggregate rows for ship hull and hostile catalogs when they have partial/ignored entries.
pub fn build_fidelity_backlog(
    lcars_by_effect_type: &HashMap<String, TierCounts>,
    ship_hull: &TierCounts,
    ships_with_abilities_scanned: u32,
    hostile: &TierCounts,
) -> Vec<FidelityBacklogItem> {
    let mut rows: Vec<FidelityBacklogItem> = Vec::new();

    for (key, c) in lcars_by_effect_type {
        if c.gap_total() == 0 {
            continue;
        }
        rows.push(FidelityBacklogItem {
            rank: 0,
            area: "lcars",
            key: key.clone(),
            ignored: c.ignored,
            partial: c.partial,
            implemented: c.implemented,
            summary: format!(
                "LCARS officer effects (effect_type={key}): {} ignored, {} partial, {} implemented",
                c.ignored, c.partial, c.implemented
            ),
        });
    }

    if ship_hull.gap_total() > 0 {
        rows.push(FidelityBacklogItem {
            rank: 0,
            area: "ship_hull_abilities",
            key: "_aggregate".to_string(),
            ignored: ship_hull.ignored,
            partial: ship_hull.partial,
            implemented: ship_hull.implemented,
            summary: format!(
                "Extended ship hull abilities (aggregate, {ships_with_abilities_scanned} ships with abilities): {} ignored, {} partial, {} implemented",
                ship_hull.ignored, ship_hull.partial, ship_hull.implemented
            ),
        });
    }

    if hostile.gap_total() > 0 {
        rows.push(FidelityBacklogItem {
            rank: 0,
            area: "hostile_ability_catalog",
            key: "_aggregate".to_string(),
            ignored: hostile.ignored,
            partial: hostile.partial,
            implemented: hostile.implemented,
            summary: format!(
                "Hostile ability catalog (aggregate): {} ignored, {} partial, {} implemented",
                hostile.ignored, hostile.partial, hostile.implemented
            ),
        });
    }

    rows.sort_by(|a, b| {
        b.ignored
            .cmp(&a.ignored)
            .then_with(|| b.partial.cmp(&a.partial))
            .then_with(|| area_sort_key(a.area).cmp(&area_sort_key(b.area)))
            .then_with(|| a.key.cmp(&b.key))
    });

    for (i, row) in rows.iter_mut().enumerate() {
        row.rank = (i + 1) as u32;
    }

    rows
}

fn scan_ship_abilities(index: &ExtendedShipIndex, out: &mut TierCounts) -> u32 {
    let dir = root_dir().join(DEFAULT_SHIPS_EXTENDED_DIR);
    let mut ships_with = 0u32;
    for e in &index.ships {
        let Some(ext) = load_extended_ship_record(&dir, &e.id) else {
            continue;
        };
        let Some(ref abs) = ext.abilities else {
            continue;
        };
        if abs.is_empty() {
            continue;
        }
        ships_with += 1;
        for a in abs {
            let c = classify_ship_hull_ability(a);
            out.add_tier(c.tier);
        }
    }
    ships_with
}

/// Build a coverage report from bundled data (LCARS dir, extended ships, hostile catalog). Does not require `KOBAYASHI_OFFICER_SOURCE=lcars`.
pub fn build_mechanics_coverage_report(registry: &DataRegistry) -> MechanicsCoverageReport {
    let mut notes = Vec::new();
    let mut lcars_effects = TierCounts::default();
    let mut lcars_by_effect_type: HashMap<String, TierCounts> = HashMap::new();
    let mut lcars_ignored_samples: Vec<String> = Vec::new();
    const IGNORE_SAMPLE_CAP: usize = 40;

    // Built in-process from canonical (+ upstream stats/names) — no committed monolith. Paths are
    // manifest-relative so the report works regardless of CWD.
    let root = root_dir();
    let officers = crate::lcars::build_officer_model(
        &root.join(crate::lcars::DEFAULT_INPUT),
        &root.join(crate::lcars::DEFAULT_SUMMARY),
        &root.join(crate::lcars::DEFAULT_TRANSLATIONS),
        &root.join(crate::lcars::DEFAULT_OFFICER_DATA_DIR),
        false,
    )
    .unwrap_or_default();
    let lcars_officers_files = officers.len() as u32;
    if officers.is_empty() {
        notes.push(
            "No LCARS officers built from canonical source; source data missing or unreadable."
                .to_string(),
        );
    }

    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };

    for o in &officers {
        for ability in [
            o.captain_ability.as_ref(),
            o.bridge_ability.as_ref(),
            o.below_decks_ability.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            for eff in &ability.effects {
                let cov = lcars_effect_coverage(eff, &o.id, &opts);
                lcars_effects.add_tier(cov.tier);
                let key = eff.effect_type.trim().to_lowercase();
                bump(&mut lcars_by_effect_type, &key, cov.tier);
                if cov.tier == MechanicCoverageTier::Ignored
                    && lcars_ignored_samples.len() < IGNORE_SAMPLE_CAP
                {
                    lcars_ignored_samples.push(format!(
                        "{}::{} / {} / {:?} → {}",
                        o.id, ability.name, eff.effect_type, eff.trigger, cov.pathway
                    ));
                }
            }
        }
    }

    let mut ship_hull = TierCounts::default();
    let ships_scanned = registry
        .ship_index
        .as_ref()
        .map(|idx| scan_ship_abilities(idx, &mut ship_hull))
        .unwrap_or(0);
    if registry.ship_index.is_none() {
        notes.push(
            "Ship extended index not loaded; ship_hull_abilities counts are empty.".to_string(),
        );
    }

    let mut hostile_counts = TierCounts::default();
    let hostile_path = root_dir().join(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
    let catalog = load_hostile_ability_catalog(hostile_path.to_str().unwrap_or(""));
    let hostile_entry_count = catalog
        .as_ref()
        .map(|c| c.entries.len() as u32)
        .unwrap_or(0);
    let mut hostile_catalog_modeled_count = 0u32;
    let mut hostile_catalog_noop_count = 0u32;
    if let Some(ref c) = catalog {
        for entry in c.entries.values() {
            let et = entry.effect_type.trim().to_lowercase();
            if et == "combat_noop" || et == "unmodeled" || et == "not_applicable" {
                hostile_catalog_noop_count += 1;
            } else {
                hostile_catalog_modeled_count += 1;
            }
            let row = classify_hostile_catalog_entry(entry);
            hostile_counts.add_tier(row.tier);
        }
    } else {
        notes.push(format!(
            "Hostile ability catalog not loaded from {}.",
            hostile_path.display()
        ));
    }

    let upstream_hostiles_dir = root_dir().join("data/upstream/data-stfc-space/hostiles");
    let upstream_ability_ids = collect_upstream_hostile_ability_ids(&upstream_hostiles_dir);
    let hostile_upstream_unique_ability_ids = upstream_ability_ids.len() as u32;
    let hostile_upstream_ids_missing_from_catalog = if let Some(ref c) = catalog {
        upstream_ability_ids
            .keys()
            .filter(|id| !c.entries.contains_key(id.as_str()))
            .count() as u32
    } else {
        hostile_upstream_unique_ability_ids
    };
    if hostile_upstream_ids_missing_from_catalog > 0 {
        notes.push(format!(
            "Hostile ability catalog missing {hostile_upstream_ids_missing_from_catalog} upstream ability ids; run scripts/generate_full_hostile_ability_catalog.py."
        ));
    }

    let fidelity_backlog = build_fidelity_backlog(
        &lcars_by_effect_type,
        &ship_hull,
        ships_scanned,
        &hostile_counts,
    );

    MechanicsCoverageReport {
        status: "ok",
        lcars_officers_files,
        lcars_effects,
        ship_hull_abilities: ship_hull,
        ships_with_abilities_scanned: ships_scanned,
        hostile_catalog_entries: hostile_counts,
        hostile_catalog_entry_count: hostile_entry_count,
        hostile_upstream_unique_ability_ids,
        hostile_catalog_modeled_count,
        hostile_catalog_noop_count,
        hostile_upstream_ids_missing_from_catalog,
        lcars_by_effect_type,
        lcars_ignored_samples,
        fidelity_backlog,
        notes,
    }
}

pub fn mechanics_coverage_json(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&build_mechanics_coverage_report(registry))
}

#[cfg(test)]
mod fidelity_backlog_tests {
    use super::*;

    #[test]
    fn backlog_sorts_by_ignored_then_partial_then_area_and_key() {
        let mut map = HashMap::new();
        map.insert(
            "zzz".to_string(),
            TierCounts {
                implemented: 1,
                partial: 9,
                ignored: 0,
            },
        );
        map.insert(
            "aaa".to_string(),
            TierCounts {
                implemented: 0,
                partial: 1,
                ignored: 2,
            },
        );
        let ship = TierCounts::default();
        let hostile = TierCounts::default();
        let b = build_fidelity_backlog(&map, &ship, 0, &hostile);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].key, "aaa");
        assert_eq!(b[0].rank, 1);
        assert_eq!(b[1].key, "zzz");
        assert_eq!(b[1].rank, 2);
    }
}
