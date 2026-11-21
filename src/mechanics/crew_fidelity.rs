//! Seat-accurate per-crew modeling fidelity evaluation for API recommendations.
//!
//! Unlike catalog-wide officer scorecards (which aggregate all three seats of an officer),
//! a concrete crew recommendation only activates specific seats:
//! - Captain: `captain_ability` + `bridge_ability`
//! - Bridge: `bridge_ability`
//! - Below decks: `below_decks_ability`
//!
//! Evaluating only the active seats gives players an honest per-crew trust grade (`A`–`D`),
//! coverage percentage, and concise caveats for any dropped or partially modeled combat effects.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::data::data_registry::DataRegistry;
use crate::lcars::effect_spec_adapter::combat_tag_to_stat;
use crate::lcars::{
    lcars_effect_coverage, lcars_effect_to_combat_effect_spec_with_report, LcarsAbility,
    LcarsDropReport, LcarsOfficer, MechanicCoverageTier, ResolveOptions,
};
use crate::mechanics::officer_scorecard::effect_is_combat_intent;
use crate::optimizer::constraints::normalize_officer_name;

const MAX_CAVEATS: usize = 6;

/// Seat-accurate modeling fidelity summary attached to a [`crate::server::api::CrewRecommendation`].
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct CrewFidelitySummary {
    /// Active seat combat-intent effects fully implemented by the simulator.
    pub implemented: u32,
    /// Active seat combat-intent effects partially modeled by the simulator.
    pub partial: u32,
    /// Active seat combat-intent effects ignored, dropped, or unresolved.
    pub ignored: u32,
    /// Total active seat combat-intent effects inspected (`implemented + partial + ignored`).
    pub total_combat_effects: u32,
    /// Coverage percentage `(implemented + 0.5 * partial) / total_combat_effects * 100`, rounded to 0.1%.
    pub coverage_pct: f64,
    /// Coarse trust grade: `"A"` (>= 90% and 0 ignored), `"B"` (>= 75%), `"C"` (>= 50%), `"D"` (< 50%).
    pub grade: String,
    /// Human-readable notes explaining partially modeled, dropped, or unresolved seat effects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
}

/// Reusable index over loaded LCARS officers for evaluating many recommendation rows cheaply.
pub struct CrewFidelityEvaluator<'a> {
    by_norm_name: HashMap<String, &'a LcarsOfficer>,
    opts: ResolveOptions,
}

impl<'a> CrewFidelityEvaluator<'a> {
    pub fn from_registry(registry: &'a DataRegistry) -> Self {
        let mut by_norm_name = HashMap::new();
        if let Some(officers) = registry.lcars_officers() {
            for officer in officers {
                let key = normalize_officer_name(&officer.name);
                if !key.is_empty() {
                    by_norm_name.entry(key).or_insert(officer);
                }
            }
        }
        Self {
            by_norm_name,
            opts: ResolveOptions::default(),
        }
    }

    /// Evaluate modeling fidelity across the active seats of one concrete crew.
    pub fn evaluate(
        &self,
        captain: &str,
        bridge: &[String],
        below_decks: &[String],
    ) -> CrewFidelitySummary {
        let mut acc = FidelityAcc::default();

        // Captain activates both captain maneuver and bridge/officer ability.
        if !captain.trim().is_empty() {
            let norm = normalize_officer_name(captain);
            if let Some(officer) = self.by_norm_name.get(&norm) {
                if let Some(ref ab) = officer.captain_ability {
                    inspect_ability(officer, "captain", ab, &self.opts, &mut acc);
                }
                if let Some(ref ab) = officer.bridge_ability {
                    inspect_ability(officer, "captain/bridge", ab, &self.opts, &mut acc);
                }
            } else {
                acc.ignored += 1;
                acc.push_caveat(format!(
                    "{}: not found in LCARS officer catalog",
                    captain.trim()
                ));
            }
        }

        // Bridge officers activate bridge_ability only.
        for name in bridge {
            if name.trim().is_empty() {
                continue;
            }
            let norm = normalize_officer_name(name);
            if let Some(officer) = self.by_norm_name.get(&norm) {
                if let Some(ref ab) = officer.bridge_ability {
                    inspect_ability(officer, "bridge", ab, &self.opts, &mut acc);
                }
            } else {
                acc.ignored += 1;
                acc.push_caveat(format!(
                    "{}: not found in LCARS officer catalog",
                    name.trim()
                ));
            }
        }

        // Below-decks officers activate below_decks_ability only.
        for name in below_decks {
            if name.trim().is_empty() {
                continue;
            }
            let norm = normalize_officer_name(name);
            if let Some(officer) = self.by_norm_name.get(&norm) {
                if let Some(ref ab) = officer.below_decks_ability {
                    inspect_ability(officer, "below decks", ab, &self.opts, &mut acc);
                }
            } else {
                acc.ignored += 1;
                acc.push_caveat(format!(
                    "{}: not found in LCARS officer catalog",
                    name.trim()
                ));
            }
        }

        acc.finish()
    }
}

/// Evaluate modeling fidelity for one crew directly from a [`DataRegistry`].
pub fn evaluate_crew_fidelity(
    registry: &DataRegistry,
    captain: &str,
    bridge: &[String],
    below_decks: &[String],
) -> CrewFidelitySummary {
    CrewFidelityEvaluator::from_registry(registry).evaluate(captain, bridge, below_decks)
}

#[derive(Default)]
struct FidelityAcc {
    implemented: u32,
    partial: u32,
    ignored: u32,
    caveats: Vec<String>,
    seen_caveats: HashSet<String>,
}

impl FidelityAcc {
    fn push_caveat(&mut self, msg: String) {
        if self.caveats.len() < MAX_CAVEATS && self.seen_caveats.insert(msg.clone()) {
            self.caveats.push(msg);
        }
    }

    fn finish(self) -> CrewFidelitySummary {
        let total = self.implemented + self.partial + self.ignored;
        let raw_pct = if total == 0 {
            100.0
        } else {
            ((self.implemented as f64 + 0.5 * self.partial as f64) / total as f64) * 100.0
        };
        let coverage_pct = (raw_pct * 10.0).round() / 10.0;
        let grade = if coverage_pct >= 90.0 && self.ignored == 0 {
            "A"
        } else if coverage_pct >= 75.0 {
            "B"
        } else if coverage_pct >= 50.0 {
            "C"
        } else {
            "D"
        }
        .to_string();

        CrewFidelitySummary {
            implemented: self.implemented,
            partial: self.partial,
            ignored: self.ignored,
            total_combat_effects: total,
            coverage_pct,
            grade,
            caveats: self.caveats,
        }
    }
}

fn inspect_ability(
    officer: &LcarsOfficer,
    seat_label: &str,
    ability: &LcarsAbility,
    opts: &ResolveOptions,
    acc: &mut FidelityAcc,
) {
    for (idx, eff) in ability.effects.iter().enumerate() {
        if !effect_is_combat_intent(eff) {
            continue;
        }
        let cov = lcars_effect_coverage(eff, &officer.id, opts);
        let unmapped_tag = eff.effect_type.trim().eq_ignore_ascii_case("tag")
            && combat_tag_to_stat(eff.tag.as_deref().unwrap_or("")).is_none();

        let mut report = LcarsDropReport::default();
        let stable_id = format!("{}::{}::{}", officer.id, ability.name, idx);
        let _ = lcars_effect_to_combat_effect_spec_with_report(
            eff,
            &stable_id,
            &officer.id,
            &ability.name,
            opts.tier_for(&officer.id),
            None,
            idx,
            Some(&mut report),
        );

        let dropped_reason = report.drops.first().map(|d| {
            let cat = LcarsDropReport::reason_category(&d.reason);
            cat.replace('_', " ")
        });

        if unmapped_tag
            || matches!(cov.tier, MechanicCoverageTier::Ignored)
            || dropped_reason.is_some()
        {
            acc.ignored += 1;
            let detail = dropped_reason
                .or_else(|| {
                    if unmapped_tag {
                        Some(format!(
                            "unmapped combat tag '{}'",
                            eff.tag.as_deref().unwrap_or("unknown")
                        ))
                    } else {
                        Some(cov.pathway.to_string())
                    }
                })
                .unwrap_or_else(|| "unsupported effect".to_string());
            acc.push_caveat(format!("{} ({seat_label}): {detail}", officer.name));
        } else if matches!(cov.tier, MechanicCoverageTier::Partial) {
            acc.partial += 1;
            acc.push_caveat(format!(
                "{} ({seat_label}): partial ({})",
                officer.name, cov.pathway
            ));
        } else {
            acc.implemented += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_active_seats_for_known_crew_consistently() {
        let registry = DataRegistry::load().expect("registry loads");
        let summary = evaluate_crew_fidelity(
            &registry,
            "Kirk",
            &["Spock".to_string(), "Bones".to_string()],
            &["Uhura".to_string(), "".to_string()],
        );
        assert_eq!(
            summary.implemented + summary.partial + summary.ignored,
            summary.total_combat_effects
        );
        assert!(summary.total_combat_effects > 0);
        assert!((0.0..=100.0).contains(&summary.coverage_pct));
        assert!(matches!(summary.grade.as_str(), "A" | "B" | "C" | "D"));
    }

    #[test]
    fn missing_officer_produces_caveat_and_prevents_grade_a() {
        let registry = DataRegistry::load().expect("registry loads");
        let summary = evaluate_crew_fidelity(
            &registry,
            "Nonexistent Officer XYZ",
            &["Spock".to_string()],
            &[],
        );
        assert!(summary.ignored >= 1);
        assert_ne!(summary.grade, "A");
        assert!(
            summary
                .caveats
                .iter()
                .any(|c| c.contains("Nonexistent Officer XYZ")),
            "expected missing officer caveat, got {:?}",
            summary.caveats
        );
    }

    #[test]
    fn empty_slots_are_skipped_cleanly() {
        let registry = DataRegistry::load().expect("registry loads");
        let a = evaluate_crew_fidelity(&registry, "Kirk", &["Spock".to_string()], &[]);
        let b = evaluate_crew_fidelity(
            &registry,
            "Kirk",
            &["Spock".to_string(), "".to_string()],
            &["".to_string(), "   ".to_string()],
        );
        assert_eq!(a, b);
    }
}
