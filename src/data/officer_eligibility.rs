//! Officer eligibility matrix: per-ability, per-scenario combat verdicts sourced from the
//! community cheat-sheet (`data/upstream/cheat-sheet/raw-officers-*.csv`), normalized by
//! `cargo run --bin import_officer_eligibility` into [`DEFAULT_ELIGIBILITY_MATRIX_PATH`].
//!
//! The matrix is keyed by **upstream ability id** ([`crate::data::officer::OfficerAbility::ability_id`],
//! == the CSV's `AbilityID`). Each ability carries a verdict per combat scenario:
//!
//! - [`EligibilityVerdict::Works`] (`✅`) — ability functions against that target.
//! - [`EligibilityVerdict::Conditional`] (`✴️`) — functions only if in-combat conditions are met
//!   (morale up, target burning, …). The combat engine already resolves these dynamically, so
//!   eligibility treats them as *eligible*; they only drive interpretability.
//! - [`EligibilityVerdict::DoesNotWork`] (`➖`) — does nothing against that target (non-combat/loot,
//!   PvP-only, anti-armada vs non-armada, …). This is the signal the optimizer hard-filters on.
//!
//! Two consumers: [`is_eligible_for_optimization`] (hard pool filter, all seats) and
//! [`seat_best_verdict`] (simulate interpretability). Officers/abilities absent from the matrix
//! (coverage gaps) fall back to the legacy heuristics in [`crate::data::heuristics`].

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::types::EnemyType;
use crate::data::hostile::HostileRecord;
use crate::data::officer::Officer;

pub const DEFAULT_ELIGIBILITY_MATRIX_PATH: &str = "data/officers/eligibility_matrix.json";

/// Whether an ability functions against a given target category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityVerdict {
    /// `✅` — works.
    Works,
    /// `✴️` — works only if in-combat conditions are met.
    Conditional,
    /// `➖` — does not work against this target.
    DoesNotWork,
}

impl EligibilityVerdict {
    /// Higher = more functional. Used to pick the best verdict across a seat's abilities.
    fn rank(self) -> u8 {
        match self {
            EligibilityVerdict::Works => 2,
            EligibilityVerdict::Conditional => 1,
            EligibilityVerdict::DoesNotWork => 0,
        }
    }
}

/// One scenario cell: the verdict plus the cheat-sheet's reason text (the gating condition for
/// `✴️`, or the non-combat modifier name for `➖`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScenarioVerdict {
    pub verdict: EligibilityVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Eligibility for one officer ability across all scenarios.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbilityEligibility {
    pub ability_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_officer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_officer_id: Option<String>,
    /// `CM` | `OA` | `BDA` (cheat-sheet ability type), kept for debugging/coverage reports.
    pub ability_type: String,
    /// `captain` | `officer` | `below_decks` (our slot vocabulary; mapped from `ability_type`).
    pub slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional_reason: Option<String>,
    /// Keyed by [`EligibilityScenario::as_key`] (12 combat scenarios + `loot` + `utility`).
    pub scenarios: BTreeMap<String, ScenarioVerdict>,
}

/// The full matrix as loaded from `eligibility_matrix.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EligibilityMatrix {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityEligibility>,
}

impl EligibilityMatrix {
    pub fn ability(&self, ability_id: &str) -> Option<&AbilityEligibility> {
        self.abilities.get(ability_id)
    }

    pub fn scenario_verdict(
        &self,
        ability_id: &str,
        scenario: EligibilityScenario,
    ) -> Option<&ScenarioVerdict> {
        self.abilities
            .get(ability_id)
            .and_then(|a| a.scenarios.get(scenario.as_key()))
    }
}

/// The 14 cheat-sheet columns: the 12 combat [`EnemyType`] scenarios plus `Loot` and `Utility`
/// (which are informational only — not real combat targets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityScenario {
    PvpSpace,
    PvpStation,
    RedMovingSpace,
    Waves,
    MissionBosses,
    QTrial,
    SoloArmadas,
    GroupArmadas,
    Assaults,
    InvadingEntities,
    OutpostArmadas,
    OutpostRetaliationAttackers,
    Loot,
    Utility,
}

impl EligibilityScenario {
    pub const ALL: [EligibilityScenario; 14] = [
        EligibilityScenario::PvpSpace,
        EligibilityScenario::PvpStation,
        EligibilityScenario::RedMovingSpace,
        EligibilityScenario::Waves,
        EligibilityScenario::MissionBosses,
        EligibilityScenario::QTrial,
        EligibilityScenario::SoloArmadas,
        EligibilityScenario::GroupArmadas,
        EligibilityScenario::Assaults,
        EligibilityScenario::InvadingEntities,
        EligibilityScenario::OutpostArmadas,
        EligibilityScenario::OutpostRetaliationAttackers,
        EligibilityScenario::Loot,
        EligibilityScenario::Utility,
    ];

    /// JSON key used in [`AbilityEligibility::scenarios`]. The 12 combat keys match
    /// [`EnemyType`]'s `snake_case` serialization.
    pub fn as_key(self) -> &'static str {
        match self {
            EligibilityScenario::PvpSpace => "pvp_space",
            EligibilityScenario::PvpStation => "pvp_station",
            EligibilityScenario::RedMovingSpace => "red_moving_space",
            EligibilityScenario::Waves => "waves",
            EligibilityScenario::MissionBosses => "mission_bosses",
            EligibilityScenario::QTrial => "q_trial",
            EligibilityScenario::SoloArmadas => "solo_armadas",
            EligibilityScenario::GroupArmadas => "group_armadas",
            EligibilityScenario::Assaults => "assaults",
            EligibilityScenario::InvadingEntities => "invading_entities",
            EligibilityScenario::OutpostArmadas => "outpost_armadas",
            EligibilityScenario::OutpostRetaliationAttackers => "outpost_retaliation_attackers",
            EligibilityScenario::Loot => "loot",
            EligibilityScenario::Utility => "utility",
        }
    }

    /// Cheat-sheet CSV column header (paired `_Reason` column adds the suffix).
    pub fn csv_tag_column(self) -> &'static str {
        match self {
            EligibilityScenario::PvpSpace => "Tag_PvPinSpace",
            EligibilityScenario::PvpStation => "Tag_PvPStation",
            EligibilityScenario::RedMovingSpace => "Tag_NonArmadaHostiles",
            EligibilityScenario::Waves => "Tag_WaveDefense",
            EligibilityScenario::MissionBosses => "Tag_MissionBosses",
            EligibilityScenario::QTrial => "Tag_QTrial",
            EligibilityScenario::SoloArmadas => "Tag_SoloArmadas",
            EligibilityScenario::GroupArmadas => "Tag_GroupArmadas",
            EligibilityScenario::Assaults => "Tag_Assaults",
            EligibilityScenario::InvadingEntities => "Tag_InvadingEntities",
            EligibilityScenario::OutpostArmadas => "Tag_OutpostArmadas",
            EligibilityScenario::OutpostRetaliationAttackers => "Tag_OutpostRetaliators",
            EligibilityScenario::Loot => "Tag_Loot",
            EligibilityScenario::Utility => "Tag_Utility",
        }
    }

    /// Map a combat [`EnemyType`] to its scenario column. Exhaustive on purpose: adding an
    /// `EnemyType` variant must force an update here.
    pub fn from_enemy_type(et: EnemyType) -> EligibilityScenario {
        match et {
            EnemyType::PvpSpace => EligibilityScenario::PvpSpace,
            EnemyType::PvpStation => EligibilityScenario::PvpStation,
            EnemyType::RedMovingSpace => EligibilityScenario::RedMovingSpace,
            EnemyType::Waves => EligibilityScenario::Waves,
            EnemyType::MissionBosses => EligibilityScenario::MissionBosses,
            EnemyType::QTrial => EligibilityScenario::QTrial,
            EnemyType::GroupArmadas => EligibilityScenario::GroupArmadas,
            EnemyType::SoloArmadas => EligibilityScenario::SoloArmadas,
            EnemyType::InvadingEntities => EligibilityScenario::InvadingEntities,
            EnemyType::Assaults => EligibilityScenario::Assaults,
            EnemyType::OutpostArmadas => EligibilityScenario::OutpostArmadas,
            EnemyType::OutpostRetaliationAttackers => {
                EligibilityScenario::OutpostRetaliationAttackers
            }
        }
    }
}

/// Parse a cheat-sheet cell glyph into a verdict. Handles the multi-codepoint `✴️`
/// (U+2734 U+FE0F) and a bare U+2734. Returns `None` for empty/unrecognized cells.
pub fn verdict_from_glyph(cell: &str) -> Option<EligibilityVerdict> {
    let t = cell.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains('\u{2705}') {
        Some(EligibilityVerdict::Works)
    } else if t.contains('\u{2734}') {
        Some(EligibilityVerdict::Conditional)
    } else if t.contains('\u{2796}') {
        Some(EligibilityVerdict::DoesNotWork)
    } else {
        None
    }
}

/// Load the eligibility matrix from JSON. Returns `None` (with a warning) on a missing or
/// malformed file — the feature degrades gracefully to the legacy heuristics.
pub fn load_eligibility_matrix<P: AsRef<Path>>(path: P) -> Option<EligibilityMatrix> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<EligibilityMatrix>(&raw) {
        Ok(matrix) => Some(matrix),
        Err(e) => {
            eprintln!(
                "warning: failed to parse eligibility matrix at {}: {e}",
                path.display()
            );
            None
        }
    }
}

/// Best (most functional) verdict across all of `officer`'s abilities in `slot`, for `enemy`,
/// with the reason string that produced it. `Works` > `Conditional` > `DoesNotWork`.
///
/// Returns `None` when the matrix has no entry for any of the slot's abilities — a coverage gap
/// the caller should resolve via the legacy heuristic. Abilities with no `ability_id` or no
/// matrix row are skipped (a missing row never makes a seat ineligible on its own).
pub fn seat_best_verdict(
    matrix: &EligibilityMatrix,
    officer: &Officer,
    slot: &str,
    enemy: EnemyType,
) -> Option<(EligibilityVerdict, Option<String>)> {
    let scenario = EligibilityScenario::from_enemy_type(enemy);
    let mut best: Option<(EligibilityVerdict, Option<String>)> = None;
    for ability in officer
        .abilities
        .iter()
        .filter(|a| a.slot.eq_ignore_ascii_case(slot))
    {
        let Some(id) = ability.ability_id.as_deref() else {
            continue;
        };
        let Some(sv) = matrix.scenario_verdict(id, scenario) else {
            continue;
        };
        let better = match &best {
            None => true,
            Some((bv, _)) => sv.verdict.rank() > bv.rank(),
        };
        if better {
            best = Some((sv.verdict, sv.reason.clone()));
        }
    }
    best
}

/// Hard eligibility gate for the optimizer (all seats). An officer is eligible in `slot` against
/// `enemy` unless the matrix says *every* one of its abilities in that seat does not work.
///
/// Coverage gaps fall back to the legacy heuristic: below-decks defers to
/// [`crate::data::heuristics::is_below_decks_eligible_for_optimization`]; captain/bridge default
/// to eligible (preserving today's unfiltered behavior for those seats).
pub fn is_eligible_for_optimization(
    officer: &Officer,
    slot: &str,
    enemy: EnemyType,
    matrix: Option<&EligibilityMatrix>,
    pvp_mode: bool,
) -> bool {
    if let Some(matrix) = matrix {
        if let Some((verdict, _)) = seat_best_verdict(matrix, officer, slot, enemy) {
            return verdict != EligibilityVerdict::DoesNotWork;
        }
    }
    // No matrix loaded or coverage gap → legacy fallback.
    if slot.eq_ignore_ascii_case("below_decks") {
        crate::data::heuristics::is_below_decks_eligible_for_optimization(officer, pvp_mode)
    } else {
        true
    }
}

/// Parse an explicit `enemy_type` request string (snake_case, matching [`EnemyType`]'s
/// serialization) into one of the 12 combat scenarios. Returns `None` for unknown values.
pub fn enemy_type_from_str(s: &str) -> Option<EnemyType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "pvp_space" => Some(EnemyType::PvpSpace),
        "pvp_station" => Some(EnemyType::PvpStation),
        "red_moving_space" => Some(EnemyType::RedMovingSpace),
        "waves" => Some(EnemyType::Waves),
        "mission_bosses" => Some(EnemyType::MissionBosses),
        "q_trial" => Some(EnemyType::QTrial),
        "solo_armadas" => Some(EnemyType::SoloArmadas),
        "group_armadas" => Some(EnemyType::GroupArmadas),
        "assaults" => Some(EnemyType::Assaults),
        "invading_entities" => Some(EnemyType::InvadingEntities),
        "outpost_armadas" => Some(EnemyType::OutpostArmadas),
        "outpost_retaliation_attackers" => Some(EnemyType::OutpostRetaliationAttackers),
        _ => None,
    }
}

/// Resolve the combat scenario for an optimize/simulate run. An explicit, parseable
/// `enemy_type` always wins; otherwise infer from the target: PvP → [`EnemyType::PvpSpace`];
/// a group-armada hostile → [`EnemyType::GroupArmadas`]; an outpost hostile →
/// [`EnemyType::OutpostArmadas`]; anything else → [`EnemyType::RedMovingSpace`].
pub fn resolve_enemy_type(
    explicit: Option<&str>,
    is_pvp: bool,
    hostile: Option<&HostileRecord>,
) -> EnemyType {
    if let Some(et) = explicit.and_then(enemy_type_from_str) {
        return et;
    }
    if is_pvp {
        return EnemyType::PvpSpace;
    }
    if let Some(h) = hostile {
        if h.is_group_armada_target() {
            return EnemyType::GroupArmadas;
        }
        if h.is_outpost {
            return EnemyType::OutpostArmadas;
        }
    }
    EnemyType::RedMovingSpace
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::officer::OfficerAbility;

    fn ability(slot: &str, ability_id: Option<&str>) -> OfficerAbility {
        OfficerAbility {
            slot: slot.to_string(),
            ability_id: ability_id.map(str::to_string),
            conditions: vec![],
            trigger: None,
            modifier: None,
            attributes: None,
            description: None,
            chance_by_rank: vec![],
            value_by_rank: vec![],
            state_mask: 0,
        }
    }

    fn officer(abilities: Vec<OfficerAbility>) -> Officer {
        Officer {
            id: "test-officer".to_string(),
            source_officer_id: Some("123".to_string()),
            name: "Test".to_string(),
            slot: None,
            group: None,
            abilities,
        }
    }

    fn matrix_with(
        ability_id: &str,
        scenario: EligibilityScenario,
        verdict: EligibilityVerdict,
    ) -> EligibilityMatrix {
        let mut scenarios = BTreeMap::new();
        scenarios.insert(
            scenario.as_key().to_string(),
            ScenarioVerdict {
                verdict,
                reason: Some("because".to_string()),
            },
        );
        let mut abilities = BTreeMap::new();
        abilities.insert(
            ability_id.to_string(),
            AbilityEligibility {
                ability_id: ability_id.to_string(),
                source_officer_id: None,
                canonical_officer_id: None,
                ability_type: "CM".to_string(),
                slot: "captain".to_string(),
                conditional_reason: None,
                scenarios,
            },
        );
        EligibilityMatrix {
            abilities,
            ..Default::default()
        }
    }

    #[test]
    fn glyph_parsing_covers_all_three_states() {
        assert_eq!(verdict_from_glyph("✅"), Some(EligibilityVerdict::Works));
        // ✴️ = U+2734 U+FE0F (variation selector)
        assert_eq!(
            verdict_from_glyph("✴️"),
            Some(EligibilityVerdict::Conditional)
        );
        // bare U+2734 with no variation selector
        assert_eq!(
            verdict_from_glyph("\u{2734}"),
            Some(EligibilityVerdict::Conditional)
        );
        assert_eq!(
            verdict_from_glyph("➖"),
            Some(EligibilityVerdict::DoesNotWork)
        );
        assert_eq!(
            verdict_from_glyph("  ➖ "),
            Some(EligibilityVerdict::DoesNotWork)
        );
        assert_eq!(verdict_from_glyph(""), None);
        assert_eq!(verdict_from_glyph("?"), None);
    }

    #[test]
    fn scenario_keys_and_enemy_type_mapping_round_trip() {
        // q_trial is the new key and must be present.
        assert_eq!(EligibilityScenario::QTrial.as_key(), "q_trial");
        assert_eq!(
            EligibilityScenario::from_enemy_type(EnemyType::QTrial),
            EligibilityScenario::QTrial
        );
        // NonArmadaHostiles column maps to red_moving_space.
        assert_eq!(
            EligibilityScenario::RedMovingSpace.csv_tag_column(),
            "Tag_NonArmadaHostiles"
        );
        assert_eq!(
            EligibilityScenario::from_enemy_type(EnemyType::RedMovingSpace).as_key(),
            "red_moving_space"
        );
        // OutpostRetaliators column maps to the long enum key.
        assert_eq!(
            EligibilityScenario::OutpostRetaliationAttackers.csv_tag_column(),
            "Tag_OutpostRetaliators"
        );
    }

    #[test]
    fn does_not_work_excludes_conditional_and_works_do_not() {
        let m = matrix_with(
            "a1",
            EligibilityScenario::MissionBosses,
            EligibilityVerdict::DoesNotWork,
        );
        let o = officer(vec![ability("captain", Some("a1"))]);
        assert!(!is_eligible_for_optimization(
            &o,
            "captain",
            EnemyType::MissionBosses,
            Some(&m),
            false
        ));

        let m = matrix_with(
            "a1",
            EligibilityScenario::MissionBosses,
            EligibilityVerdict::Conditional,
        );
        assert!(is_eligible_for_optimization(
            &o,
            "captain",
            EnemyType::MissionBosses,
            Some(&m),
            false
        ));

        let m = matrix_with(
            "a1",
            EligibilityScenario::MissionBosses,
            EligibilityVerdict::Works,
        );
        assert!(is_eligible_for_optimization(
            &o,
            "captain",
            EnemyType::MissionBosses,
            Some(&m),
            false
        ));
    }

    #[test]
    fn seat_eligible_when_any_officer_ability_works() {
        // officer slot can hold two abilities; eligible if not all DoesNotWork.
        let mut scenarios_bad = BTreeMap::new();
        scenarios_bad.insert(
            EligibilityScenario::RedMovingSpace.as_key().to_string(),
            ScenarioVerdict {
                verdict: EligibilityVerdict::DoesNotWork,
                reason: None,
            },
        );
        let mut scenarios_good = BTreeMap::new();
        scenarios_good.insert(
            EligibilityScenario::RedMovingSpace.as_key().to_string(),
            ScenarioVerdict {
                verdict: EligibilityVerdict::Works,
                reason: None,
            },
        );
        let mut abilities = BTreeMap::new();
        abilities.insert(
            "bad".to_string(),
            AbilityEligibility {
                ability_id: "bad".to_string(),
                source_officer_id: None,
                canonical_officer_id: None,
                ability_type: "OA".to_string(),
                slot: "officer".to_string(),
                conditional_reason: None,
                scenarios: scenarios_bad,
            },
        );
        abilities.insert(
            "good".to_string(),
            AbilityEligibility {
                ability_id: "good".to_string(),
                source_officer_id: None,
                canonical_officer_id: None,
                ability_type: "OA".to_string(),
                slot: "officer".to_string(),
                conditional_reason: None,
                scenarios: scenarios_good,
            },
        );
        let m = EligibilityMatrix {
            abilities,
            ..Default::default()
        };
        let o = officer(vec![
            ability("officer", Some("bad")),
            ability("officer", Some("good")),
        ]);
        assert!(is_eligible_for_optimization(
            &o,
            "officer",
            EnemyType::RedMovingSpace,
            Some(&m),
            false
        ));
    }

    #[test]
    fn coverage_gap_captain_defaults_eligible() {
        let m = EligibilityMatrix::default();
        let o = officer(vec![ability("captain", Some("unknown-id"))]);
        // No matrix entry → captain seat falls back to eligible.
        assert!(is_eligible_for_optimization(
            &o,
            "captain",
            EnemyType::RedMovingSpace,
            Some(&m),
            false
        ));
    }

    #[test]
    fn resolve_enemy_type_prefers_explicit_then_infers() {
        // Explicit, valid value wins — even over the PvP flag.
        assert_eq!(
            resolve_enemy_type(Some("mission_bosses"), false, None),
            EnemyType::MissionBosses
        );
        assert_eq!(
            resolve_enemy_type(Some("q_trial"), true, None),
            EnemyType::QTrial
        );
        // Invalid explicit value → fall through to inference.
        assert_eq!(
            resolve_enemy_type(Some("garbage"), true, None),
            EnemyType::PvpSpace
        );
        assert_eq!(resolve_enemy_type(None, true, None), EnemyType::PvpSpace);
        assert_eq!(
            resolve_enemy_type(None, false, None),
            EnemyType::RedMovingSpace
        );
        assert_eq!(enemy_type_from_str("PVP_Space"), Some(EnemyType::PvpSpace));
        assert_eq!(enemy_type_from_str("nope"), None);
    }
}
