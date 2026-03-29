//! Aggregate implemented / partial / ignored mechanics across LCARS, ship hull abilities, and hostile catalogs.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::combat::abilities::TimingWindow;
use crate::data::data_registry::DataRegistry;
use crate::data::hostile_ability_resolve::{
    hostile_ability_effect_from_catalog, load_hostile_ability_catalog, HostileAbilityCatalogEntry,
    DEFAULT_HOSTILE_ABILITY_CATALOG_PATH,
};
use crate::data::ship::{load_extended_ship_record, ExtendedShipIndex, ShipAbility};
use crate::data::ship_ability_resolve::{parse_ship_ability_timing, ship_ability_effect_from_catalog};
use crate::lcars::{
    lcars_effect_coverage, load_lcars_dir, LcarsEffectCoverage, MechanicCoverageTier, ResolveOptions,
};

const DEFAULT_LCARS_DIR: &str = "data/officers";
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
    /// Counts by LCARS `effect_type` string (lowercased).
    pub lcars_by_effect_type: HashMap<String, TierCounts>,
    /// Sample of ignored LCARS pathways (capped) for debugging.
    pub lcars_ignored_samples: Vec<String>,
    pub notes: Vec<String>,
}

fn root_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn bump(map: &mut HashMap<String, TierCounts>, key: &str, tier: MechanicCoverageTier) {
    map.entry(key.to_string())
        .or_default()
        .add_tier(tier);
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

fn scan_ship_abilities(index: &ExtendedShipIndex, out: &mut TierCounts) -> u32 {
    let dir = root_dir().join(DEFAULT_SHIPS_EXTENDED_DIR);
    let mut ships_with = 0u32;
    for e in &index.ships {
        let Some(ext) = load_extended_ship_record(&dir, &e.id) else {
            continue;
        };
        let Some(ref abs) = ext.abilities else { continue };
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

    let lcars_path = root_dir().join(DEFAULT_LCARS_DIR);
    let officers = load_lcars_dir(&lcars_path).unwrap_or_default();
    let lcars_officers_files = officers.len() as u32;
    if officers.is_empty() {
        notes.push(format!(
            "No LCARS YAML loaded from {}; directory missing or empty.",
            lcars_path.display()
        ));
    }

    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
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
        notes.push("Ship extended index not loaded; ship_hull_abilities counts are empty.".to_string());
    }

    let mut hostile_counts = TierCounts::default();
    let hostile_path = root_dir().join(DEFAULT_HOSTILE_ABILITY_CATALOG_PATH);
    let catalog = load_hostile_ability_catalog(hostile_path.to_str().unwrap_or(""));
    let hostile_entry_count = catalog
        .as_ref()
        .map(|c| c.entries.len() as u32)
        .unwrap_or(0);
    if let Some(ref c) = catalog {
        for entry in c.entries.values() {
            let row = classify_hostile_catalog_entry(entry);
            hostile_counts.add_tier(row.tier);
        }
    } else {
        notes.push(format!(
            "Hostile ability catalog not loaded from {}.",
            hostile_path.display()
        ));
    }

    MechanicsCoverageReport {
        status: "ok",
        lcars_officers_files,
        lcars_effects,
        ship_hull_abilities: ship_hull,
        ships_with_abilities_scanned: ships_scanned,
        hostile_catalog_entries: hostile_counts,
        hostile_catalog_entry_count: hostile_entry_count,
        lcars_by_effect_type,
        lcars_ignored_samples,
        notes,
    }
}

pub fn mechanics_coverage_json(registry: &DataRegistry) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&build_mechanics_coverage_report(registry))
}
