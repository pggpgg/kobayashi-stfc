//! Snapshot-bound recorded fight suite: manifest loader and profile-bound replay runner.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::calibration::drift::{simulation_band_report, DriftRunReport, MetricBands};
use crate::combat::export_csv::parse_fight_export;
use crate::combat::SimulationResult;
use crate::data::data_registry::DataRegistry;
use crate::data::loader::resolve_hostile_by_display_name;
use crate::optimizer::crew_generator::CrewCandidate;
use crate::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, DefenderOpponent, MonteCarloSeedReplay,
};

/// Suite manifest committed next to calibration fixtures.
#[derive(Debug, Clone, Deserialize)]
pub struct RecordedFightSuite {
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub snapshot_note: Option<String>,
    #[serde(default)]
    pub fights: Vec<RecordedFightEntry>,
}

/// One snapshot-bound fight export wired for profile replay and band scoring.
#[derive(Debug, Clone, Deserialize)]
pub struct RecordedFightEntry {
    pub id: String,
    /// Path relative to repo root (e.g. `fight samples/realta_vs_takret.csv`).
    pub fixture_csv: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    pub ship_id: String,
    #[serde(default)]
    pub ship_tier: Option<u32>,
    #[serde(default)]
    pub ship_level: Option<u32>,
    #[serde(default)]
    pub hostile_id: Option<String>,
    #[serde(default)]
    pub hostile_display_name: Option<String>,
    #[serde(default)]
    pub hostile_level: Option<u32>,
    pub captain: String,
    #[serde(default)]
    pub bridge: Vec<String>,
    #[serde(default)]
    pub below_decks: Vec<String>,
    #[serde(default)]
    pub defender_faction_override: Option<String>,
    #[serde(default)]
    pub primary_axes: Vec<String>,
    #[serde(default)]
    pub holdout: bool,
    #[serde(default)]
    pub bands: MetricBands,
    #[serde(default)]
    pub expect_attacker_won: Option<bool>,
    /// When set, ties this fight to an officer-stat formula anchor (kirk | marla | kras).
    #[serde(default)]
    pub officer_anchor: Option<String>,
    /// RNG seed for deterministic replay (default 42).
    #[serde(default = "default_seed")]
    pub scenario_seed: u64,
    /// Monte Carlo sim index (default 0).
    #[serde(default)]
    pub sim_index: u64,
}

fn default_seed() -> u64 {
    42
}

#[derive(Debug)]
pub struct RecordedSuiteRun {
    pub all_reports: Vec<DriftRunReport>,
    /// Non-holdout fights — used for iteration composite.
    pub iteration_reports: Vec<DriftRunReport>,
    pub axis_coverage: BTreeMap<String, usize>,
    pub officer_anchors: BTreeMap<String, String>,
}

pub fn load_recorded_fight_suite(path: &Path) -> Result<RecordedFightSuite, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let suite: RecordedFightSuite =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    validate_suite(&suite)?;
    Ok(suite)
}

fn validate_suite(suite: &RecordedFightSuite) -> Result<(), String> {
    for fight in &suite.fights {
        if fight.hostile_id.is_none()
            && (fight.hostile_display_name.is_none() || fight.hostile_level.is_none())
        {
            return Err(format!(
                "fight {}: set hostile_id or hostile_display_name + hostile_level",
                fight.id
            ));
        }
        if let Some(anchor) = &fight.officer_anchor {
            match anchor.as_str() {
                "kirk" | "marla" | "kras" => {}
                other => {
                    return Err(format!(
                        "fight {}: unknown officer_anchor {other:?} (expected kirk, marla, or kras)",
                        fight.id
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn run_recorded_suite(
    repo_root: &Path,
    manifest_path: &Path,
) -> Result<RecordedSuiteRun, String> {
    let suite = load_recorded_fight_suite(manifest_path)?;
    if suite.fights.is_empty() {
        return Ok(RecordedSuiteRun {
            all_reports: Vec::new(),
            iteration_reports: Vec::new(),
            axis_coverage: BTreeMap::new(),
            officer_anchors: BTreeMap::new(),
        });
    }

    let registry = DataRegistry::load().map_err(|e| format!("DataRegistry::load: {e}"))?;
    std::env::set_var("KOBAYASHI_OFFICER_SOURCE", "lcars");

    let mut all_reports = Vec::with_capacity(suite.fights.len());
    let mut iteration_reports = Vec::new();
    let mut axis_coverage: BTreeMap<String, usize> = BTreeMap::new();
    let mut officer_anchors: BTreeMap<String, String> = BTreeMap::new();

    for fight in &suite.fights {
        let report = run_recorded_fight(repo_root, registry.as_ref(), &suite, fight)?;
        for axis in &fight.primary_axes {
            *axis_coverage.entry(axis.clone()).or_insert(0) += 1;
        }
        if let Some(anchor) = &fight.officer_anchor {
            officer_anchors.insert(anchor.clone(), fight.id.clone());
        }
        if !fight.holdout {
            iteration_reports.push(report.clone());
        }
        all_reports.push(report);
    }

    Ok(RecordedSuiteRun {
        all_reports,
        iteration_reports,
        axis_coverage,
        officer_anchors,
    })
}

fn run_recorded_fight(
    repo_root: &Path,
    registry: &DataRegistry,
    suite: &RecordedFightSuite,
    fight: &RecordedFightEntry,
) -> Result<DriftRunReport, String> {
    let csv_path = repo_root.join(&fight.fixture_csv);
    let contents =
        fs::read_to_string(&csv_path).map_err(|e| format!("read {}: {e}", csv_path.display()))?;
    let export = parse_fight_export(&contents)
        .map_err(|e| format!("fight {} parse {}: {e}", fight.id, csv_path.display()))?;

    let hostile_id = resolve_fight_hostile_id(fight)?;
    let profile_id = fight
        .profile_id
        .as_deref()
        .or(suite.profile_id.as_deref());

    let candidate = CrewCandidate {
        captain: fight.captain.clone(),
        bridge: fight.bridge.clone(),
        below_decks: fight.below_decks.clone(),
    };

    let replay = replay_optimize_iteration_with_registry(
        registry,
        &fight.ship_id,
        &hostile_id,
        fight.ship_tier,
        fight.ship_level,
        &candidate,
        fight.scenario_seed,
        fight.sim_index,
        profile_id,
        0,
        None,
        DefenderOpponent::Hostile,
    );

    if replay.using_placeholder_combatants {
        return Err(format!(
            "fight {}: ship/hostile did not resolve (ship={} hostile={})",
            fight.id, fight.ship_id, hostile_id
        ));
    }

    let sim_result = replay_to_simulation_result(&replay);
    let expect_won = fight
        .expect_attacker_won
        .or(Some(export.attacker_won));

    Ok(simulation_band_report(
        &fight.id,
        Some(&format!(
            "recorded: {} vs {} (profile={})",
            fight.ship_id,
            hostile_id,
            profile_id.unwrap_or("(suite default)")
        )),
        Some("snapshot-bound recorded fight"),
        &fight.bands,
        expect_won,
        &sim_result,
    ))
}

fn resolve_fight_hostile_id(fight: &RecordedFightEntry) -> Result<String, String> {
    if let Some(id) = &fight.hostile_id {
        return Ok(id.clone());
    }
    let name = fight
        .hostile_display_name
        .as_ref()
        .ok_or_else(|| format!("fight {}: missing hostile_display_name", fight.id))?;
    let level = fight
        .hostile_level
        .ok_or_else(|| format!("fight {}: missing hostile_level", fight.id))?;
    let rec = resolve_hostile_by_display_name(name, level)
        .map_err(|e| format!("fight {}: {e}", fight.id))?
        .ok_or_else(|| {
            format!(
                "fight {}: no hostile for display name {name:?} level {level}",
                fight.id
            )
        })?;
    Ok(rec.id.clone())
}

fn replay_to_simulation_result(replay: &MonteCarloSeedReplay) -> SimulationResult {
    SimulationResult {
        attacker_won: replay.attacker_won,
        winner_by_round_limit: replay.winner_by_round_limit,
        rounds_simulated: replay.rounds_simulated,
        total_damage: replay.total_damage,
        attacker_hull_remaining: replay.attacker_hull_remaining,
        defender_hull_remaining: replay.defender_hull_remaining,
        defender_shield_remaining: replay.defender_shield_remaining,
        attacker_shield_remaining: 0.0,
        events: Vec::new(),
        conqueror_borg_beam_suppression: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_empty_recorded_fight_suite() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/recorded_fights/recorded_fight_suite.json");
        let suite = load_recorded_fight_suite(&path).expect("load manifest");
        assert!(suite.fights.is_empty());
        assert!(suite.profile_id.is_none());
    }

    #[test]
    fn resolve_hostile_id_from_display_name_in_manifest_schema() {
        let fight = RecordedFightEntry {
            id: "test".into(),
            fixture_csv: "fight samples/realta vs takret militia 10.csv".into(),
            profile_id: None,
            ship_id: "realta".into(),
            ship_tier: None,
            ship_level: None,
            hostile_id: None,
            hostile_display_name: Some("Takret Militia".into()),
            hostile_level: Some(10),
            captain: "livis".into(),
            bridge: vec![],
            below_decks: vec![],
            defender_faction_override: None,
            primary_axes: vec![],
            holdout: false,
            bands: MetricBands::default(),
            expect_attacker_won: None,
            officer_anchor: None,
            scenario_seed: 42,
            sim_index: 0,
        };
        let id = resolve_fight_hostile_id(&fight).expect("resolve");
        assert!(
            id == "845501025" || id == "1973028640",
            "unexpected takret id: {id}"
        );
    }
}
