//! Generate a single LCARS monolith (`officers.lcars.yaml`) from officers.canonical.json.
//! Run: cargo run --bin generate_lcars [-- path/to/officers.canonical.json] [--output data/officers]
//!   [--summary data/upstream/data-stfc-space/summary-officer.json]
//!   [--translations data/upstream/data-stfc-space/translations-officer_buffs.json]
//!   [--officer-data-dir data/upstream/data-stfc-space/officers]
//! Output: `<output_dir>/officers.lcars.yaml` (all officers, sorted by id).
//!
//! When `--summary` and `--translations` point at data-stfc-space exports, ability block names are
//! resolved from `officer_ability_name` rows (`loca_id` in summary matches `id` in translations).
//!
//! Canonical `conditions` strings are mapped to LCARS when they match resolver-supported mechanics
//! (enemy/self hull class, armada synonyms, `TargetNotArmada` → LCARS `not`, burning/hull breach,
//! morale, opponent kind, `EnemyHullFaction` via attributes `faction_id`). Unmapped tokens are skipped with a stderr line; emitted conditions may
//! be weaker than in-game (subset `and`). Token mapping is implemented in
//! `kobayashi::lcars::canonical_conditions` (`map_canonical_condition_token`, `canonical_conditions_to_lcars`).
//!
//! **`HullRepair` (cheat-sheet / canonical):** STFC uses trigger `RoundEnd` for “repair hull damage
//! taken last round”; Kobayashi applies that heal at **round start** as `stat_modify` /
//! `hull_hp_repair_prev_round` (resolver `HullRegenPrevRoundFraction`). Regenerated LCARS therefore
//! uses `on_round_start` for this modifier regardless of canonical trigger string.
//!
//! **`Accuracy`:** `MultiplyAdd` with **no** canonical `conditions` and **no** `attributes` maps to
//! passive `stat_modify` / `multiply` with per-rank `1.0 + value` (resolver `accuracy_cb_mult` for
//! hostile dodge / pierce precompute). Rows with officer-stat attributes, non-`MultiplyAdd`
//! operations, or gated conditions stay `accuracy:unmapped` until modeled.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::data::combat_effect_spec::OfficerStat;
use crate::lcars::{
    canonical_conditions_to_lcars, LcarsAbility, LcarsCondition, LcarsDuration, LcarsEffect,
    LcarsFile, LcarsLevelStats, LcarsOfficer, LcarsScaling,
};

/// Canonical/source paths (CWD-relative) consumed by [`build_officer_model_default`] at runtime.
pub const DEFAULT_INPUT: &str = "data/officers/officers.canonical.json";
pub const DEFAULT_SUMMARY: &str = "data/upstream/data-stfc-space/summary-officer.json";
pub const DEFAULT_TRANSLATIONS: &str =
    "data/upstream/data-stfc-space/translations-officer_buffs.json";
pub const DEFAULT_OFFICER_DATA_DIR: &str = "data/upstream/data-stfc-space/officers";

type BuildResult = Result<Vec<LcarsOfficer>, Box<dyn std::error::Error + Send + Sync>>;

/// Build the officer model in-process from canonical JSON (+ upstream stats and translation names).
/// Returns the same `Vec<LcarsOfficer>` (sorted by id) that parsing the generated
/// `officers.lcars.yaml` monolith used to produce — the monolith is no longer a runtime artifact.
/// `skip_names` omits ability display-name resolution (summary/translations not read).
pub fn build_officer_model(
    canonical_path: &Path,
    summary_path: &Path,
    translations_path: &Path,
    officer_data_dir: &Path,
    skip_names: bool,
) -> BuildResult {
    let raw = fs::read_to_string(canonical_path)?;
    let parsed: CanonicalFile = serde_json::from_str(&raw)?;
    let upstream_stats_by_officer = load_upstream_officer_stats(officer_data_dir)?;

    let mut name_ctx = if skip_names {
        NameResolveContext {
            summary_by_officer: HashMap::new(),
            name_by_loca: HashMap::new(),
            upstream_stats_by_officer: HashMap::new(),
        }
    } else {
        load_name_resolve_context(summary_path, translations_path)?
    };
    name_ctx.upstream_stats_by_officer = upstream_stats_by_officer;

    let (officers_by_faction, _names_resolved) =
        convert_officers_to_lcars(parsed.officers, &name_ctx);
    let mut all_officers: Vec<LcarsOfficer> = officers_by_faction.into_values().flatten().collect();
    all_officers.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(all_officers)
}

/// [`build_officer_model`] using the default CWD-relative data paths (the runtime entry point).
pub fn build_officer_model_default() -> BuildResult {
    build_officer_model(
        Path::new(DEFAULT_INPUT),
        Path::new(DEFAULT_SUMMARY),
        Path::new(DEFAULT_TRANSLATIONS),
        Path::new(DEFAULT_OFFICER_DATA_DIR),
        false,
    )
}

/// [`build_officer_model_default`] wrapped in an [`LcarsFile`] — the in-process replacement for
/// `load_lcars_file("data/officers/officers.lcars.yaml")` now that the monolith is not committed.
pub fn build_officer_model_file_default(
) -> Result<LcarsFile, Box<dyn std::error::Error + Send + Sync>> {
    Ok(LcarsFile {
        officers: build_officer_model_default()?,
    })
}
const SHIELD_MAX_FRACTION_ABILITY_IDS: &[&str] = &[
    "1513489186", // Black Ops M'Benga: per-round SHP restore vs non-armada hostiles.
    "3196098481", // Seska: per-round SHP restore vs non-armada hostiles.
    "3634862157", // SNW M'Benga: per-round SHP restore vs non-armada hostiles.
    "3851967764", // Caleb Mir: per-round SHP restore vs Outposts / non-armada hostiles.
    "1166619300", // Genesis Lythe: per-round SHP restore in Wave Defense.
];

#[derive(Debug, Deserialize)]
struct CanonicalFile {
    officers: Vec<CanonicalOfficer>,
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficer {
    id: String,
    name: String,
    #[serde(default)]
    faction: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    rarity: Option<String>,
    #[serde(default, rename = "slot")]
    _slot: Option<String>,
    #[serde(default)]
    source_officer_id: Option<String>,
    #[serde(default)]
    abilities: Vec<CanonicalAbility>,
}

#[derive(Debug, Deserialize)]
struct CanonicalAbility {
    #[serde(default)]
    ability_id: Option<String>,
    #[serde(default)]
    slot: String,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default)]
    modifier: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    attributes: Option<String>,
    #[serde(default, rename = "description")]
    _description: Option<String>,
    #[serde(default)]
    chance_by_rank: Vec<f64>,
    #[serde(default)]
    value_by_rank: Vec<f64>,
    /// Game condition tokens (PascalCase strings). Mapped to LCARS where supported.
    #[serde(default)]
    conditions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SummaryOfficer {
    id: u64,
    #[serde(default)]
    captain_ability: Option<SummaryAbility>,
    #[serde(default)]
    ability: Option<SummaryAbility>,
    #[serde(default)]
    below_decks_ability: Option<SummaryAbility>,
}

#[derive(Debug, Deserialize)]
struct SummaryAbility {
    id: u64,
    loca_id: u64,
}

#[derive(Debug, Deserialize)]
struct TranslationRow {
    id: Option<u64>,
    key: String,
    text: String,
}

#[derive(Debug, Clone)]
struct UpstreamOfficerStats {
    stats: Vec<LcarsLevelStats>,
    max_level_by_rank: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct UpstreamOfficer {
    #[serde(default)]
    stats: Vec<LcarsLevelStats>,
    #[serde(default)]
    ranks: Vec<UpstreamOfficerRank>,
}

#[derive(Debug, Deserialize)]
struct UpstreamOfficerRank {
    rank: u8,
    max_level: u32,
}

struct NameResolveContext {
    summary_by_officer: HashMap<u64, SummaryOfficer>,
    name_by_loca: HashMap<u64, String>,
    upstream_stats_by_officer: HashMap<u64, UpstreamOfficerStats>,
}

fn load_name_resolve_context(
    summary_path: &Path,
    translations_path: &Path,
) -> Result<NameResolveContext, Box<dyn std::error::Error + Send + Sync>> {
    let mut summary_by_officer: HashMap<u64, SummaryOfficer> = HashMap::new();
    if summary_path.is_file() {
        let raw = fs::read_to_string(summary_path)?;
        let list: Vec<SummaryOfficer> = serde_json::from_str(&raw)?;
        for o in list {
            summary_by_officer.insert(o.id, o);
        }
    } else {
        eprintln!(
            "Warning: summary not found at {} — ability names will use placeholders.",
            summary_path.display()
        );
    }

    let mut name_by_loca: HashMap<u64, String> = HashMap::new();
    if translations_path.is_file() {
        let raw = fs::read_to_string(translations_path)?;
        let rows: Vec<TranslationRow> = serde_json::from_str(&raw)?;
        for row in rows {
            if row.key != "officer_ability_name" {
                continue;
            }
            let Some(id) = row.id else {
                continue;
            };
            name_by_loca
                .entry(id)
                .or_insert_with(|| row.text.trim().to_string());
        }
    } else {
        eprintln!(
            "Warning: translations not found at {} — ability names will use placeholders.",
            translations_path.display()
        );
    }

    Ok(NameResolveContext {
        summary_by_officer,
        name_by_loca,
        upstream_stats_by_officer: HashMap::new(),
    })
}

fn load_upstream_officer_stats(
    officer_data_dir: &Path,
) -> Result<HashMap<u64, UpstreamOfficerStats>, Box<dyn std::error::Error + Send + Sync>> {
    let mut out: HashMap<u64, UpstreamOfficerStats> = HashMap::new();
    if !officer_data_dir.is_dir() {
        eprintln!(
            "Warning: upstream officer data dir not found at {} — LCARS officer stats omitted.",
            officer_data_dir.display()
        );
        return Ok(out);
    }

    for entry in fs::read_dir(officer_data_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(parse_numeric_id)
        else {
            continue;
        };
        let raw = fs::read_to_string(&path)?;
        let parsed: UpstreamOfficer = serde_json::from_str(&raw)?;
        let mut stats = parsed.stats;
        stats.sort_by_key(|s| s.level);

        let mut max_level_by_rank: Vec<u32> = Vec::new();
        for rank in parsed.ranks {
            if rank.rank == 0 {
                continue;
            }
            let idx = (rank.rank as usize).saturating_sub(1);
            if max_level_by_rank.len() <= idx {
                max_level_by_rank.resize(idx + 1, 0);
            }
            max_level_by_rank[idx] = rank.max_level;
        }

        if !stats.is_empty() || !max_level_by_rank.is_empty() {
            out.insert(
                id,
                UpstreamOfficerStats {
                    stats,
                    max_level_by_rank,
                },
            );
        }
    }

    Ok(out)
}

fn faction_to_filename(faction: &str) -> String {
    let normalized = faction
        .to_lowercase()
        .replace([' ', '-'], "_")
        .replace("section_31", "section31");
    match normalized.as_str() {
        "" | "unknown" | "faction" => "independent".to_string(),
        _ => normalized,
    }
}

fn convert_officers_to_lcars(
    officers: Vec<CanonicalOfficer>,
    ctx: &NameResolveContext,
) -> (HashMap<String, Vec<LcarsOfficer>>, u64) {
    let mut by_faction: HashMap<String, Vec<LcarsOfficer>> = HashMap::new();
    let mut names_resolved: u64 = 0;

    for officer in officers {
        let faction_key = officer.faction.as_deref().unwrap_or("Unknown");
        let faction_key = faction_to_filename(faction_key);

        let (lcars, n) = convert_officer(officer, ctx);
        names_resolved += n;
        by_faction.entry(faction_key).or_default().push(lcars);
    }

    (by_faction, names_resolved)
}

fn parse_numeric_id(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<u64>() {
        return Some(v);
    }
    if let Ok(f) = s.parse::<f64>() {
        let u = f as u64;
        if f >= 0.0 && (f - u as f64).abs() < 1.0 {
            return Some(u);
        }
    }
    None
}

fn loca_for_game_ability(summary: &SummaryOfficer, game_ability_id: u64) -> Option<u64> {
    if let Some(a) = &summary.captain_ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    if let Some(a) = &summary.ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    if let Some(a) = &summary.below_decks_ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    None
}

fn resolve_ability_display_name(
    ability: &CanonicalAbility,
    summary: Option<&SummaryOfficer>,
    ctx: &NameResolveContext,
) -> Option<String> {
    let aid = ability.ability_id.as_deref()?;
    let game_id = parse_numeric_id(aid)?;
    let so = summary?;
    let loca = loca_for_game_ability(so, game_id)?;
    let name = ctx.name_by_loca.get(&loca).cloned()?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn ability_block_name(
    seat_label: &str,
    abilities: &[CanonicalAbility],
    summary: Option<&SummaryOfficer>,
    ctx: &NameResolveContext,
    officer_name: &str,
) -> (String, bool) {
    for a in abilities {
        if let Some(n) = resolve_ability_display_name(a, summary, ctx) {
            return (n, true);
        }
    }
    (format!("{} ({})", officer_name, seat_label), false)
}

fn convert_officer(o: CanonicalOfficer, ctx: &NameResolveContext) -> (LcarsOfficer, u64) {
    let summary = o
        .source_officer_id
        .as_deref()
        .and_then(parse_numeric_id)
        .and_then(|id| ctx.summary_by_officer.get(&id));
    let upstream = o
        .source_officer_id
        .as_deref()
        .and_then(parse_numeric_id)
        .and_then(|id| ctx.upstream_stats_by_officer.get(&id));

    let mut captain_abs: Vec<CanonicalAbility> = Vec::new();
    let mut bridge_abs: Vec<CanonicalAbility> = Vec::new();
    let mut below_abs: Vec<CanonicalAbility> = Vec::new();

    for ability in o.abilities {
        match ability.slot.to_lowercase().as_str() {
            "captain" => captain_abs.push(ability),
            "officer" => bridge_abs.push(ability),
            "below" | "below_decks" => below_abs.push(ability),
            _ => bridge_abs.push(ability),
        }
    }

    let mut names_resolved: u64 = 0;

    let captain_ability = if !captain_abs.is_empty() {
        let effects: Vec<LcarsEffect> = captain_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Captain", &captain_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    let bridge_ability = if !bridge_abs.is_empty() {
        let effects: Vec<LcarsEffect> = bridge_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Bridge", &bridge_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    let below_decks_ability = if !below_abs.is_empty() {
        let effects: Vec<LcarsEffect> = below_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Below Decks", &below_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    (
        LcarsOfficer {
            id: o.id,
            name: o.name,
            faction: o.faction,
            rarity: o.rarity,
            group: o.group,
            captain_ability,
            bridge_ability,
            below_decks_ability,
            stats: upstream.map(|s| s.stats.clone()).unwrap_or_default(),
            max_level_by_rank: upstream
                .map(|s| s.max_level_by_rank.clone())
                .unwrap_or_default(),
        },
        names_resolved,
    )
}

/// Maps one canonical `value_by_rank` sample to the numeric stored in LCARS `value` / `scaling.values`
/// (same rules as [map_modifier] for [MappedEffect::StatModify]).
fn transform_canonical_to_lcars_value(modifier: &str, op: &str, val: f64) -> f64 {
    match modifier {
        "CritChance" | "CritDamage" => val,
        "AllDamage" | "OfficerStatAttack" => {
            if op.eq_ignore_ascii_case("MultiplyAdd") {
                1.0 + val
            } else {
                val
            }
        }
        "Accuracy" => {
            if op.eq_ignore_ascii_case("MultiplyAdd") {
                1.0 + val
            } else {
                val
            }
        }
        "ShipArmor" | "OfficerStatDefense" => val,
        "AllDefenses" => {
            if op.eq_ignore_ascii_case("MultiplySub") {
                -val
            } else {
                val
            }
        }
        "ArmorPiercing" | "AllPiercing" => val,
        "ShieldHPMax" | "HullHPMax" => 1.0 + val,
        "ApexShred" | "ApexBarrier" => val,
        "IsolyticDamage" | "IsolyticDefense" => val,
        "IsolyticCascade" | "IsolyticCascadeDamage" => val,
        // Shots bonus is applied as additive fraction B_shots in the engine (`1.0 + B_shots` multiplier).
        "ShotsPerAttack" => val,
        "ShieldHPRepair"
        | "ShieldRegen"
        | "ShieldRepairPrevRound"
        | "HullHPRepair"
        | "HullRegen"
        | "HullRepair" => val,
        _ => val,
    }
}

fn effect_condition_from_canonical(
    a: &CanonicalAbility,
    officer_name: &str,
    ability_label: &str,
) -> Option<LcarsCondition> {
    let attrs = a.attributes.as_deref().unwrap_or("");
    let wants_hull_faction = a
        .conditions
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case("EnemyHullFaction"));
    let wants_combat_battle_type = a
        .conditions
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case("CombatBattleType"));
    let wants_target_max_level = a
        .conditions
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case("TargetMaxLevel"));
    let wants_hull_health_below_start = a.conditions.iter().any(|c| {
        c.trim()
            .eq_ignore_ascii_case("HullHealthBelowStartOfCombat")
    });
    let wants_hull_health_below = a
        .conditions
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case("HullHealthBelow"));
    let wants_hull_health_above = a
        .conditions
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case("HullHealthAbove"));
    let filtered: Vec<String> = a
        .conditions
        .iter()
        .filter(|c| {
            let t = c.trim();
            !t.eq_ignore_ascii_case("EnemyHullFaction")
                && !t.eq_ignore_ascii_case("CombatBattleType")
                && !t.eq_ignore_ascii_case("TargetMaxLevel")
                && !t.eq_ignore_ascii_case("HullHealthBelowStartOfCombat")
                && !t.eq_ignore_ascii_case("HullHealthBelow")
                && !t.eq_ignore_ascii_case("HullHealthAbove")
        })
        .cloned()
        .collect();
    let mut conditions: Vec<LcarsCondition> =
        canonical_conditions_to_lcars(&filtered, officer_name, ability_label)
            .into_iter()
            .collect();

    if wants_hull_faction {
        match faction_id_from_canonical_attributes(attrs) {
            Some(fid) => conditions.push(LcarsCondition {
                condition_type: "defender_hull_faction_id".to_string(),
                stat: None,
                threshold_pct: None,
                min: None,
                max: None,
                faction: None,
                group: None,
                min_members: None,
                tag: None,
                ship_type: None,
                faction_id: Some(fid),
                ship_id: None,
                enemy_type: None,
                battle_types: None,
                conditions: None,
            }),
            None => {
                eprintln!(
                    "generate_lcars: EnemyHullFaction without parseable faction_id in attributes \
                     (officer {officer_name:?}, ability {ability_label:?})"
                );
            }
        }
    }

    if wants_combat_battle_type {
        match battle_types_from_canonical_attributes(attrs) {
            Some(ids) if !ids.is_empty() => conditions.push(LcarsCondition {
                condition_type: "combat_battle_type_any".to_string(),
                stat: None,
                threshold_pct: None,
                min: None,
                max: None,
                faction: None,
                group: None,
                min_members: None,
                tag: None,
                ship_type: None,
                faction_id: None,
                ship_id: None,
                enemy_type: None,
                battle_types: Some(ids),
                conditions: None,
            }),
            _ => eprintln!(
                "generate_lcars: CombatBattleType without parseable battle_types in attributes \
                 (officer {officer_name:?}, ability {ability_label:?})"
            ),
        }
    }

    if wants_target_max_level {
        match max_level_from_canonical_attributes(attrs) {
            Some(max_level) => conditions.push(LcarsCondition {
                condition_type: "defender_level_at_most".to_string(),
                stat: None,
                threshold_pct: None,
                min: None,
                max: Some(max_level),
                faction: None,
                group: None,
                min_members: None,
                tag: None,
                ship_type: None,
                faction_id: None,
                ship_id: None,
                enemy_type: None,
                battle_types: None,
                conditions: None,
            }),
            None => eprintln!(
                "generate_lcars: TargetMaxLevel without parseable max_level in attributes \
                 (officer {officer_name:?}, ability {ability_label:?})"
            ),
        }
    }

    for token in [
        (wants_hull_health_above, "HullHealthAbove"),
        (
            wants_hull_health_below_start,
            "HullHealthBelowStartOfCombat",
        ),
        (wants_hull_health_below, "HullHealthBelow"),
    ]
    .into_iter()
    .filter_map(|(enabled, tok)| enabled.then_some(tok))
    {
        match hull_health_condition_from_canonical_attributes(token, attrs, a.target.as_deref()) {
            Some(c) => conditions.push(c),
            None => eprintln!(
                "generate_lcars: {token} without parseable percentage in attributes \
                 (officer {officer_name:?}, ability {ability_label:?})"
            ),
        }
    }

    match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(LcarsCondition {
            condition_type: "and".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: None,
            faction_id: None,
            ship_id: None,
            enemy_type: None,
            battle_types: None,
            conditions: Some(conditions),
        }),
    }
}

fn convert_ability_to_effect(a: &CanonicalAbility, officer_name: &str) -> Option<LcarsEffect> {
    let modifier = a.modifier.as_deref().unwrap_or("");
    let mapped = map_modifier(modifier, a)?;
    let trigger = if modifier == "HullRepair" || modifier == "ShieldRepairPrevRound" {
        "on_round_start"
    } else if matches!(&mapped, MappedEffect::StatModify(ref stat, _, _) if stat == "accuracy") {
        // Resolver folds this into `static_buffs.accuracy_cb_mult` (combat-begin timing). Canonical
        // RoundStart / ShipLaunched still becomes `passive` so the accuracy loop picks it up.
        "passive"
    } else {
        map_trigger(a.trigger.as_deref().unwrap_or("ShipLaunched"))
    };
    let target = map_target(a);
    let op = a.operation.as_deref().unwrap_or("Add");
    let ability_label = a.ability_id.as_deref().unwrap_or(modifier);
    let cond = effect_condition_from_canonical(a, officer_name, ability_label);

    match mapped {
        MappedEffect::Tag(tag_name) => {
            let chance_scaling = if modifier.eq_ignore_ascii_case("AddRandomState") {
                scaling_for_add_random_state(a.attributes.as_deref(), &a.chance_by_rank)
            } else if a.chance_by_rank.len() > 1 {
                scaling_from_ranks(&[], &a.chance_by_rank, modifier, None)
            } else {
                None
            };
            let chance_field = if a.chance_by_rank.len() == 1 {
                a.chance_by_rank.first().copied()
            } else {
                None
            };
            let duration = if modifier.eq_ignore_ascii_case("AddRandomState") {
                num_rounds_from_attributes(a.attributes.as_deref())
                    .map(|n| LcarsDuration::Rounds { rounds: n })
                    .unwrap_or(LcarsDuration::Rounds { rounds: 3 })
            } else if modifier.eq_ignore_ascii_case("AllReloadSpeed")
                || modifier.eq_ignore_ascii_case("AllLoadSpeed")
            {
                reload_speed_tag_duration(a)
            } else {
                num_rounds_from_attributes(a.attributes.as_deref())
                    .map(|n| LcarsDuration::Rounds { rounds: n })
                    .unwrap_or(LcarsDuration::Permanent("permanent".to_string()))
            };
            Some(LcarsEffect {
                effect_type: "tag".to_string(),
                stat: None,
                target: Some(target.to_string()),
                operator: Some(map_lcars_operator(op)),
                value: a.value_by_rank.first().copied(),
                trigger: Some(trigger.to_string()),
                duration: Some(duration),
                scaling: chance_scaling,
                condition: cond.clone(),
                chance: chance_field,
                multiplier: None,
                tag: Some(tag_name),
                accumulate: None,
                decay: None,
            })
        }
        MappedEffect::State(state_type, chance) => {
            let effect_type = match state_type {
                StateType::Morale => "morale",
                StateType::Assimilated => "assimilated",
                StateType::HullBreach => "hull_breach",
                StateType::Burning => "burning",
            };
            // Resolver prefers `chance` over `scaling`; omit fixed chance when ranks vary so
            // `chance_values` / `chance_at_rank` apply.
            let chance_field = if a.chance_by_rank.len() > 1 {
                None
            } else {
                Some(chance)
            };
            Some(LcarsEffect {
                effect_type: effect_type.to_string(),
                stat: None,
                target: Some(target.to_string()),
                operator: None,
                value: None,
                trigger: Some(trigger.to_string()),
                duration: Some(LcarsDuration::Permanent("permanent".to_string())),
                scaling: scaling_from_ranks(&[], &a.chance_by_rank, "AddState", None),
                condition: cond.clone(),
                chance: chance_field,
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            })
        }
        MappedEffect::StatModify(stat, operator, value_rank1) => {
            let lcars_values: Vec<f64> = a
                .value_by_rank
                .iter()
                .copied()
                .map(|v| transform_canonical_to_lcars_value(modifier, op, v))
                .collect();
            let scaling = scaling_from_ranks(
                &lcars_values,
                &a.chance_by_rank,
                modifier,
                a.attributes.as_deref(),
            );
            let value_field = if lcars_values.len() > 1 {
                None
            } else {
                Some(lcars_values.first().copied().unwrap_or(value_rank1))
            };
            let duration = if stat == "shots_per_attack" {
                LcarsDuration::Rounds {
                    rounds: num_rounds_from_attributes(a.attributes.as_deref()).unwrap_or(2),
                }
            } else if matches!(
                stat.as_str(),
                "officer_attack" | "officer_defense" | "officer_health" | "officer_stat_all"
            ) {
                // Officer-stat keys honor canonical `num_rounds` (e.g. Kirk OfficerStatAll
                // `num_rounds=1` → first-round-only), matching the Tag path. Other stat_modify
                // effects (shield drain, weapon damage, …) stay permanent unless handled above.
                num_rounds_from_attributes(a.attributes.as_deref())
                    .map(|n| LcarsDuration::Rounds { rounds: n })
                    .unwrap_or(LcarsDuration::Permanent("permanent".to_string()))
            } else {
                LcarsDuration::Permanent("permanent".to_string())
            };
            Some(LcarsEffect {
                effect_type: "stat_modify".to_string(),
                stat: Some(stat),
                target: Some(target.to_string()),
                operator: Some(operator),
                value: value_field,
                trigger: Some(trigger.to_string()),
                duration: Some(duration),
                scaling,
                condition: cond,
                chance: None,
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            })
        }
    }
}

enum MappedEffect {
    Tag(String),
    State(StateType, f64),
    StatModify(String, String, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StateType {
    Morale,
    Assimilated,
    HullBreach,
    Burning,
}

/// Canonical `attributes` use `state=8` (Morale), `state=4` (Hull Breach), etc.
/// Parse the numeric id so `state=2` does not match `state=20`.
/// Parse `faction_id=<i64>` from canonical ability `attributes` (comma-separated `key=value` pairs).
fn faction_id_from_canonical_attributes(raw: &str) -> Option<i64> {
    for part in raw.split(',') {
        let mut it = part.trim().splitn(2, '=');
        let key = it.next()?.trim();
        if !key.eq_ignore_ascii_case("faction_id") {
            continue;
        }
        let val = it.next()?.trim();
        if val.is_empty() {
            return None;
        }
        return val.parse::<i64>().ok();
    }
    None
}

fn percentage_from_canonical_attributes(raw: &str) -> Option<f64> {
    for part in raw.split(',') {
        let mut it = part.trim().splitn(2, '=');
        let key = it.next()?.trim();
        if !key.eq_ignore_ascii_case("percentage") {
            continue;
        }
        let val = it.next()?.trim();
        if val.is_empty() {
            return None;
        }
        let parsed = val.parse::<f64>().ok()?;
        if !parsed.is_finite() {
            return None;
        }
        return Some(parsed.clamp(0.0, 1.0));
    }
    None
}

fn battle_types_from_canonical_attributes(raw: &str) -> Option<Vec<u32>> {
    let lower = raw.to_ascii_lowercase();
    let key = "battle_types=";
    let idx = lower.find(key)?;
    let after = &raw[idx + key.len()..];
    let start = after.find('[')?;
    let rest = &after[start + 1..];
    let end = rest.find(']')?;
    let inner = &rest[..end];
    let mut out = Vec::new();
    for token in inner.split(',') {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        let id = t.parse::<u32>().ok()?;
        out.push(id);
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

fn max_level_from_canonical_attributes(raw: &str) -> Option<u32> {
    for part in raw.split(',') {
        let mut it = part.trim().splitn(2, '=');
        let key = it.next()?.trim();
        if !key.eq_ignore_ascii_case("max_level") {
            continue;
        }
        let val = it.next()?.trim();
        if val.is_empty() {
            return None;
        }
        return val.parse::<u32>().ok();
    }
    None
}

fn officer_stat_from_canonical_attributes(raw: &str) -> Option<OfficerStat> {
    for part in raw.split(',') {
        let mut it = part.trim().splitn(2, '=');
        let key = it.next()?.trim();
        if !key.eq_ignore_ascii_case("officer_stat") {
            continue;
        }
        let val = it.next()?.trim();
        return match val.parse::<u32>().ok()? {
            1 => Some(OfficerStat::Attack),
            2 => Some(OfficerStat::Defense),
            3 => Some(OfficerStat::Health),
            _ => None,
        };
    }
    None
}

fn is_shield_max_fraction_ability(a: &CanonicalAbility) -> bool {
    a.ability_id
        .as_deref()
        .is_some_and(|id| SHIELD_MAX_FRACTION_ABILITY_IDS.contains(&id))
}

fn hull_health_condition_from_canonical_attributes(
    token: &str,
    attrs: &str,
    target: Option<&str>,
) -> Option<LcarsCondition> {
    let threshold_pct = percentage_from_canonical_attributes(attrs)?;
    let stat = if target
        .map(|t| t.to_ascii_lowercase().contains("enemy"))
        .unwrap_or(false)
    {
        "hull_hp"
    } else {
        "attacker_hull_hp"
    };
    let ty = if token.eq_ignore_ascii_case("HullHealthAbove") {
        "stat_above"
    } else {
        "stat_below"
    };
    Some(LcarsCondition {
        condition_type: ty.to_string(),
        stat: Some(stat.to_string()),
        threshold_pct: Some(threshold_pct),
        min: None,
        max: None,
        faction: None,
        group: None,
        min_members: None,
        tag: None,
        ship_type: None,
        faction_id: None,
        ship_id: None,
        enemy_type: None,
        battle_types: None,
        conditions: None,
    })
}

/// Parse `multi_state=[8, 4, 2]` from canonical ability attributes.
/// Integers are STFC state ids (8=Morale, 4=HullBreach, 2=Burning, 64=Assimilated); used as relative weights.
pub(crate) fn multi_state_from_attributes(raw: Option<&str>) -> Option<Vec<u32>> {
    let raw = raw?;
    let start = raw
        .find("multi_state=[")
        .or_else(|| raw.find("multi_state =["))?;
    let after_bracket = start + raw[start..].find('[')? + 1;
    let close = raw[after_bracket..].find(']')? + after_bracket;
    let inner = raw[after_bracket..close].trim();
    let ids: Vec<u32> = inner
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// LCARS scaling for `AddRandomState`: rank proc chances + `multi_state` weight ids in `values`.
fn scaling_for_add_random_state(
    attributes: Option<&str>,
    chance_by_rank: &[f64],
) -> Option<LcarsScaling> {
    let state_ids = multi_state_from_attributes(attributes).unwrap_or_else(|| vec![8, 4, 2]);
    let max_rank = chance_by_rank.len().max(state_ids.len()).max(1) as u8;
    Some(LcarsScaling {
        base: None,
        per_rank: None,
        max_rank: Some(max_rank),
        base_chance: None,
        values: Some(state_ids.into_iter().map(|n| n as f64).collect()),
        chance_values: if chance_by_rank.len() > 1 {
            Some(chance_by_rank.to_vec())
        } else {
            None
        },
        officer_stat: None,
    })
}

/// Parse `num_rounds=N` from canonical ability `attributes` (comma-separated `key=value` pairs).
fn num_rounds_from_attributes(raw: Option<&str>) -> Option<u32> {
    let raw = raw?;
    for part in raw.split(',') {
        let mut it = part.trim().splitn(2, '=');
        let key = it.next()?.trim();
        if !key.eq_ignore_ascii_case("num_rounds") {
            continue;
        }
        let val = it.next()?.trim();
        if val.is_empty() {
            return None;
        }
        let n: u32 = val.parse().ok()?;
        return Some(n.max(1));
    }
    None
}

fn add_state_type_from_attributes(raw: &str) -> Option<StateType> {
    let attrs = raw.to_lowercase();
    if let Some(idx) = attrs.find("state=") {
        let rest = &attrs[idx + "state=".len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(id) = digits.parse::<u32>() {
            return match id {
                8 => Some(StateType::Morale),
                64 => Some(StateType::Assimilated),
                4 => Some(StateType::HullBreach),
                2 => Some(StateType::Burning),
                _ => None,
            };
        }
    }
    if attrs.contains("state8") || attrs.contains("morale") {
        return Some(StateType::Morale);
    }
    if attrs.contains("state64") || attrs.contains("assimilat") {
        return Some(StateType::Assimilated);
    }
    if attrs.contains("state4") || attrs.contains("hullbreach") {
        return Some(StateType::HullBreach);
    }
    if attrs.contains("state2") || attrs.contains("burning") {
        return Some(StateType::Burning);
    }
    None
}

fn map_modifier(modifier: &str, a: &CanonicalAbility) -> Option<MappedEffect> {
    let val = a.value_by_rank.first().copied().unwrap_or(0.0);
    let op = a.operation.as_deref().unwrap_or("Add");
    let chance = a.chance_by_rank.first().copied().unwrap_or(1.0);

    let result = match modifier {
        "CritChance" => MappedEffect::StatModify("crit_chance".into(), "add".into(), val),
        "CritDamage" => MappedEffect::StatModify("crit_damage".into(), "add".into(), val),
        "AllDamage" | "OfficerStatAttack" => {
            let (op_str, v) = if op.eq_ignore_ascii_case("MultiplyAdd") {
                ("multiply", 1.0 + val)
            } else {
                ("add", val)
            };
            MappedEffect::StatModify("weapon_damage".into(), op_str.into(), v)
        }
        "Accuracy" => {
            let attrs = a.attributes.as_deref().unwrap_or("").trim();
            if !a.conditions.is_empty()
                || !attrs.is_empty()
                || !op.eq_ignore_ascii_case("MultiplyAdd")
            {
                MappedEffect::Tag("accuracy:unmapped".into())
            } else {
                MappedEffect::StatModify("accuracy".into(), "multiply".into(), 1.0 + val)
            }
        }
        "ShipArmor" | "OfficerStatDefense" => {
            MappedEffect::StatModify("armor".into(), "add".into(), val)
        }
        "OfficerStatHealth" => {
            let v = if op.eq_ignore_ascii_case("MultiplyAdd") {
                1.0 + val
            } else {
                val
            };
            MappedEffect::StatModify("officer_health".into(), "multiply".into(), v)
        }
        "OfficerStatAll" => {
            let v = if op.eq_ignore_ascii_case("MultiplyAdd") {
                1.0 + val
            } else {
                val
            };
            MappedEffect::StatModify("officer_stat_all".into(), "multiply".into(), v)
        }
        "AllReloadSpeed" | "AllLoadSpeed" => MappedEffect::Tag(map_allreloadspeed_tag(a, op)),
        "CptManeuverEffect" => MappedEffect::Tag("cptmaneuvereffect:unmapped".into()),
        "AddRandomState" => MappedEffect::Tag("addrandomstate:unmapped".into()),
        "OffAbilityEffect" => MappedEffect::Tag("offabilityeffect:unmapped".into()),
        "AllDefenses" => {
            if op.eq_ignore_ascii_case("MultiplySub") || op.eq_ignore_ascii_case("MultiplyBaseSub")
            {
                MappedEffect::StatModify("shield_mitigation".into(), "add".into(), -val)
            } else {
                MappedEffect::StatModify("armor".into(), "add".into(), val)
            }
        }
        "ArmorPiercing" | "AllPiercing" => {
            MappedEffect::StatModify("shield_pierce".into(), "add".into(), val)
        }
        "ShieldHPMax" => MappedEffect::StatModify("shield_hp".into(), "multiply".into(), 1.0 + val),
        "Shields" => MappedEffect::StatModify("shield_hp".into(), "multiply".into(), val),
        "HullHPMax" => MappedEffect::StatModify("hull_hp".into(), "multiply".into(), 1.0 + val),
        "ApexShred" => MappedEffect::StatModify("apex_shred".into(), "add".into(), val),
        "ApexBarrier" => MappedEffect::StatModify("apex_barrier".into(), "add".into(), val),
        "IsolyticDamage" => MappedEffect::StatModify("isolytic_damage".into(), "add".into(), val),
        "IsolyticDefense" => MappedEffect::StatModify("isolytic_defense".into(), "add".into(), val),
        "IsolyticCascade" | "IsolyticCascadeDamage" => {
            MappedEffect::StatModify("isolytic_cascade_damage".into(), "add".into(), val)
        }
        "ShotsPerAttack" => MappedEffect::StatModify("shots_per_attack".into(), "add".into(), val),
        "ArmadaLoot" => MappedEffect::Tag("armadaloot:non_combat".into()),
        "ShieldHPRepair" | "ShieldRegen" => MappedEffect::StatModify(
            if is_shield_max_fraction_ability(a) {
                "shield_regen_max_fraction".into()
            } else {
                "shield_regen".into()
            },
            // Honor canonical operation: enemy drains are `MultiplySub`/`MultiplyBaseSub` (→ `sub`,
            // compiled to DefenderShieldDrainPerRound); self repairs are `MultiplyAdd`/`Add`.
            // Hardcoding `add` here was wrong for drains (only SNW Sam Kirk was hand-fixed).
            map_lcars_operator(a.operation.as_deref().unwrap_or("Add")),
            val,
        ),
        // Fraction of hull damage taken last round; engine applies at round start.
        "HullRepair" => {
            MappedEffect::StatModify("hull_hp_repair_prev_round".into(), "add".into(), val)
        }
        // Fraction of gross shield damage taken last round; engine applies at round start.
        "ShieldRepairPrevRound" => {
            MappedEffect::StatModify("shield_hp_repair_prev_round".into(), "add".into(), val)
        }
        "HullHPRepair" | "HullRegen" => MappedEffect::StatModify(
            "hull_hp_repair".into(),
            // Honor canonical operation (enemy hull drains use `MultiplySub`/`MultiplyBaseSub`).
            map_lcars_operator(a.operation.as_deref().unwrap_or("Add")),
            val,
        ),
        "AddState" => {
            let attrs = a.attributes.as_deref().unwrap_or("");
            match add_state_type_from_attributes(attrs) {
                Some(st) => MappedEffect::State(st, chance),
                None => MappedEffect::Tag(format!("add_state:{}", modifier.to_lowercase())),
            }
        }
        // Modifiers known to be out-of-combat (economy, travel, post-combat rewards, loot drops,
        // ability-side resources). Adding to this list is preferred to the `:unmapped` fallback —
        // it removes them from `validate_data --coverage` noise so combat-relevant drops surface.
        // Verify a candidate is non-combat (i.e. has no in-fight effect on damage / hull / shield /
        // hit chance / proc chance) before adding it here.
        "MiningRate"
        | "MiningReward"
        | "CargoCapacity"
        | "CargoProtection"
        | "FactionPointsGain"
        | "PveChestLootMultiplierLimitedResources"
        | "HostileLoot"
        | "CombatScavenger"
        | "SkillCloakingDuration"
        | "SkillCloakingCooldown"
        | "SkillCuttingBeamPvPBaseDamagePercentage"
        | "SkillCuttingBeamAbilityCost"
        | "Omega13Cooldown"
        | "VoyagerAsaCE"
        | "WarpSpeed"
        | "WarpDistance"
        | "ImpulseSpeed"
        | "JumpAndTowCostEff"
        | "RepairTime"
        | "RepairCostsPost"
        | "CombatXPReward"
        | "CombatPveRewards"
        | "CombatDilithiumReward"
        | "CombatParsteelReward"
        | "CombatTritaniumReward"
        | "TrelliumRewards"
        | "ActianVenomAndNanoprobeLoot"
        | "ArtifactTokenLoot"
        | "BrokenShipPartsLoot"
        | "GornHostileVolatileLoot"
        | "HirogenRelicAndBiotoxinLoot"
        | "WokAugmentAllLootRewards"
        | "XindiHostileLoot"
        // June 2026 batch (Emerald Chain / Starfleet Academy): loot, repair, and overworld
        // cutting-beam economy modifiers with no in-fight effect on damage / hull / shield.
        | "TechnologicalDistinctivenessLoot" // Jirali (bridge): cutting-beam loot.
        | "CuttingBeamCharge" // Jirali (captain): extra Borg Cube cutting-beam charge.
        | "RepairSpeedCascade" // Jirali (below): ship repair-speed cascade.
        | "OutpostMedalsAndPlunderLoot" => {
            // Genesis Lythe (below): outpost retaliation medals / plunder loot.
            MappedEffect::Tag(format!("{}:non_combat", modifier.to_lowercase()))
        }
        _ => MappedEffect::Tag(format!("{}:unmapped", modifier.to_lowercase())),
    };

    Some(result)
}

fn map_trigger(canonical: &str) -> &'static str {
    match canonical {
        "ShipLaunched" => "passive",
        "CombatStart" => "on_combat_start",
        "RoundStart" => "on_round_start",
        "RoundEnd" => "on_round_end",
        "CriticalShotFired" => "on_critical",
        "EnemyTakesHit" | "HitTaken" => "on_hit",
        "ShieldsDepleted" => "on_shield_break",
        "Kill" | "EnemyKilled" => "on_kill",
        _ => "passive",
    }
}

fn map_lcars_operator(op: &str) -> String {
    match op.trim().to_ascii_lowercase().as_str() {
        "sub" | "multiplysub" | "multiply_sub" | "mul_sub" => "sub".to_string(),
        "multiplyadd" | "multiply_add" | "mul_add" => "multiply".to_string(),
        "multiply" | "mul" => "multiply".to_string(),
        "set" => "set".to_string(),
        other => other.to_string(),
    }
}

/// STFC `AllReloadSpeed` / `AllLoadSpeed`: enemy `Add` delays defender fire; self `Sub` recharges attacker weapons.
fn map_allreloadspeed_tag(a: &CanonicalAbility, op: &str) -> String {
    let target = map_target(a);
    let op_l = op.trim().to_ascii_lowercase();
    if target == "enemy"
        && (op_l == "add" || op_l == "multiplyadd" || op_l == "multiply_add" || op_l == "mul_add")
    {
        "allreloadspeed:enemy_delay".into()
    } else if target == "self"
        && (op_l == "sub" || op_l == "multiplysub" || op_l == "multiply_sub" || op_l == "mul_sub")
    {
        "allreloadspeed:self_recharge".into()
    } else {
        eprintln!(
            "warning: unmapped AllReloadSpeed/AllLoadSpeed target={target} op={op} officer ability={:?}",
            a.ability_id
        );
        "allreloadspeed:unmapped".into()
    }
}

fn reload_speed_tag_duration(a: &CanonicalAbility) -> LcarsDuration {
    num_rounds_from_attributes(a.attributes.as_deref())
        .map(|n| LcarsDuration::Rounds { rounds: n })
        .or_else(|| {
            a.value_by_rank.first().copied().and_then(|v| {
                if v.is_finite() && (1.0..=10.0).contains(&v) && (v - v.round()).abs() < 1e-9 {
                    Some(LcarsDuration::Rounds {
                        rounds: v.round() as u32,
                    })
                } else {
                    None
                }
            })
        })
        .unwrap_or(LcarsDuration::Rounds { rounds: 1 })
}

fn map_target(a: &CanonicalAbility) -> &'static str {
    let t = a.target.as_deref().unwrap_or("").to_lowercase();
    if t.contains("enemybridge") {
        "enemy_bridge"
    } else if t.contains("enemy") {
        "enemy"
    } else {
        "self"
    }
}

fn scaling_from_ranks(
    value_by_rank: &[f64],
    chance_by_rank: &[f64],
    _modifier: &str,
    attributes: Option<&str>,
) -> Option<LcarsScaling> {
    if value_by_rank.is_empty() && chance_by_rank.is_empty() {
        return None;
    }

    let max_rank = value_by_rank.len().max(chance_by_rank.len()).max(1) as u8;
    let values = if !value_by_rank.is_empty() {
        Some(value_by_rank.to_vec())
    } else {
        None
    };

    let chance_values = if !chance_by_rank.is_empty() {
        Some(chance_by_rank.to_vec())
    } else {
        None
    };

    Some(LcarsScaling {
        base: None,
        per_rank: None,
        max_rank: Some(max_rank),
        base_chance: None,
        values,
        chance_values,
        officer_stat: if value_by_rank.is_empty() {
            None
        } else {
            attributes.and_then(officer_stat_from_canonical_attributes)
        },
    })
}

#[cfg(test)]
mod build_model_tests {
    use super::*;
    use crate::lcars::LcarsFile;

    /// Safety net for retiring the monolith: the in-process officer model must serialize to the
    /// committed `officers.lcars.yaml` byte-for-byte, so building in-process can't change officer
    /// resolution. (Guarded on the file still existing; once retired, this becomes a no-op.)
    #[test]
    fn build_officer_model_matches_committed_yaml() {
        let path = std::path::Path::new("data/officers/officers.lcars.yaml");
        if !path.exists() {
            return;
        }
        let built = build_officer_model_default().expect("build_officer_model_default");
        let serialized =
            serde_yaml::to_string(&LcarsFile { officers: built }).expect("serialize model");
        let committed = std::fs::read_to_string(path).expect("read committed yaml");
        assert_eq!(
            serialized, committed,
            "in-process officer model must equal the committed officers.lcars.yaml"
        );
    }
}

#[cfg(test)]
mod canonical_condition_tests {
    use super::*;

    #[test]
    fn add_state_canonical_attributes_map_morale() {
        assert_eq!(
            add_state_type_from_attributes("num_rounds=1, state=8"),
            Some(StateType::Morale)
        );
        assert_eq!(
            add_state_type_from_attributes("num_rounds=3, state=4"),
            Some(StateType::HullBreach)
        );
    }

    #[test]
    fn add_state_does_not_treat_state_20_as_burning() {
        assert_eq!(add_state_type_from_attributes("state=20"), None);
    }

    #[test]
    fn faction_id_from_attributes_parses_typical_row() {
        assert_eq!(
            super::faction_id_from_canonical_attributes("faction_id=1750120904, officer_stat=3"),
            Some(1750120904_i64)
        );
        assert_eq!(
            super::faction_id_from_canonical_attributes("officer_stat=3"),
            None
        );
    }

    #[test]
    fn percentage_from_attributes_parses_and_clamps() {
        assert_eq!(
            super::percentage_from_canonical_attributes("percentage=0.7, foo=1"),
            Some(0.7)
        );
        assert_eq!(
            super::percentage_from_canonical_attributes("percentage=2.5"),
            Some(1.0)
        );
        assert_eq!(
            super::percentage_from_canonical_attributes("percentage=-0.2"),
            Some(0.0)
        );
        assert_eq!(super::percentage_from_canonical_attributes("foo=1"), None);
    }

    #[test]
    fn battle_types_from_attributes_parses_and_dedups() {
        assert_eq!(
            super::battle_types_from_canonical_attributes("battle_types=[9, 4, 4, 2]"),
            Some(vec![2, 4, 9])
        );
        assert_eq!(
            super::battle_types_from_canonical_attributes("foo=1,battle_types=[4]"),
            Some(vec![4])
        );
        assert_eq!(super::battle_types_from_canonical_attributes("foo=1"), None);
    }

    #[test]
    fn max_level_from_attributes_parses() {
        assert_eq!(
            super::max_level_from_canonical_attributes("max_level=70"),
            Some(70)
        );
        assert_eq!(
            super::max_level_from_canonical_attributes("foo=1,max_level=51"),
            Some(51)
        );
        assert_eq!(super::max_level_from_canonical_attributes("foo=1"), None);
    }

    #[test]
    fn officer_stat_from_attributes_maps_stfc_ids() {
        assert_eq!(
            super::officer_stat_from_canonical_attributes("num_rounds=1, officer_stat=1"),
            Some(OfficerStat::Attack)
        );
        assert_eq!(
            super::officer_stat_from_canonical_attributes("officer_stat=2"),
            Some(OfficerStat::Defense)
        );
        assert_eq!(
            super::officer_stat_from_canonical_attributes("officer_stat=3"),
            Some(OfficerStat::Health)
        );
        assert_eq!(
            super::officer_stat_from_canonical_attributes("officer_stat=99"),
            None
        );
        assert_eq!(super::officer_stat_from_canonical_attributes("foo=1"), None);
    }

    #[test]
    fn scaling_from_ranks_emits_officer_stat_clause() {
        let scaling = super::scaling_from_ranks(
            &[15.0, 25.0],
            &[1.0, 1.0],
            "AllDefenses",
            Some("officer_stat=3"),
        )
        .expect("scaling");
        assert_eq!(scaling.officer_stat, Some(OfficerStat::Health));
    }

    #[test]
    fn spock_accuracy_maps_to_passive_multiply_scaling() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "ability_id": "869555258",
            "modifier": "Accuracy",
            "operation": "MultiplyAdd",
            "trigger": "RoundStart",
            "target": "SelfShip",
            "chance_by_rank": [1.0, 1.0, 1.0],
            "value_by_rank": [0.15, 0.05, 0.1],
            "conditions": []
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "Spock").expect("effect");
        assert_eq!(e.effect_type, "stat_modify");
        assert_eq!(e.stat.as_deref(), Some("accuracy"));
        assert_eq!(e.operator.as_deref(), Some("multiply"));
        assert_eq!(e.trigger.as_deref(), Some("passive"));
        let vals = e.scaling.as_ref().unwrap().values.as_ref().unwrap();
        assert_eq!(vals, &vec![1.15, 1.05, 1.1]);
    }

    #[test]
    fn curated_shield_repair_rows_emit_max_fraction_stat() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "ability_id": "3196098481",
            "modifier": "ShieldHPRepair",
            "operation": "MultiplyAdd",
            "trigger": "RoundStart",
            "target": "SelfShip",
            "chance_by_rank": [1.0, 1.0],
            "value_by_rank": [0.12, 0.16],
            "conditions": []
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "Seska").expect("effect");
        assert_eq!(e.stat.as_deref(), Some("shield_regen_max_fraction"));

        let ordinary: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "ability_id": "not-curated",
            "modifier": "ShieldHPRepair",
            "operation": "MultiplyAdd",
            "trigger": "RoundStart",
            "target": "SelfShip",
            "chance_by_rank": [1.0],
            "value_by_rank": [10.0],
            "conditions": []
        }))
        .unwrap();
        let e = convert_ability_to_effect(&ordinary, "Other").expect("effect");
        assert_eq!(e.stat.as_deref(), Some("shield_regen"));
    }

    #[test]
    fn enemy_hull_faction_merges_with_mapped_conditions() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "modifier": "AllDamage",
            "operation": "MultiplyAdd",
            "trigger": "CombatStart",
            "target": "SelfShip",
            "conditions": ["EnemyHullFaction", "EnemyHostile"],
            "attributes": "faction_id=1750120904",
            "value_by_rank": [1.0],
            "chance_by_rank": []
        }))
        .unwrap();
        let c = super::effect_condition_from_canonical(&a, "Test", "AllDamage").expect("cond");
        assert_eq!(c.condition_type, "and");
        let kids = c.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "defender_is_npc_hostile");
        assert_eq!(kids[1].condition_type, "defender_hull_faction_id");
        assert_eq!(kids[1].faction_id, Some(1750120904));
    }

    #[test]
    fn convert_add_state_harry_kim_style_to_morale_lcars() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "modifier": "AddState",
            "attributes": "num_rounds=1, state=8",
            "trigger": "RoundStart",
            "target": "SelfShip",
            "chance_by_rank": [0.1, 0.15, 0.3, 0.6, 1.0],
            "value_by_rank": [1.0, 1.0, 1.0, 1.0, 1.0]
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "Harry Kim").expect("maps");
        assert_eq!(e.effect_type, "morale");
        assert_eq!(e.trigger.as_deref(), Some("on_round_start"));
        assert!(e.scaling.is_some());
        assert_eq!(
            e.scaling
                .as_ref()
                .unwrap()
                .chance_values
                .as_ref()
                .unwrap()
                .len(),
            5
        );
    }

    #[test]
    fn hull_repair_maps_to_prev_round_stat_and_round_start() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "officer",
            "trigger": "RoundEnd",
            "modifier": "HullRepair",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "chance_by_rank": [1.0, 1.0, 1.0, 1.0, 1.0],
            "value_by_rank": [0.05, 0.1, 0.15, 0.2, 0.35],
            "conditions": []
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "PIC Hugh").expect("effect");
        assert_eq!(e.effect_type, "stat_modify");
        assert_eq!(e.stat.as_deref(), Some("hull_hp_repair_prev_round"));
        assert_eq!(e.trigger.as_deref(), Some("on_round_start"));
        assert!(e.scaling.is_some());
        let vals = e.scaling.as_ref().unwrap().values.as_ref().unwrap();
        assert_eq!(vals.len(), 5);
        assert!((vals[0] - 0.05).abs() < 1e-9);
    }

    #[test]
    fn num_rounds_from_attributes_parses_typical_row() {
        assert_eq!(
            super::num_rounds_from_attributes(Some("num_rounds=2")),
            Some(2)
        );
        assert_eq!(
            super::num_rounds_from_attributes(Some("foo=1, num_rounds=5, bar=2")),
            Some(5)
        );
        assert_eq!(super::num_rounds_from_attributes(Some("")), None);
        assert_eq!(super::num_rounds_from_attributes(None), None);
    }

    #[test]
    fn multi_state_from_attributes_parses_bracket_list() {
        assert_eq!(
            super::multi_state_from_attributes(Some("multi_state=[8, 4, 2], num_rounds=3")),
            Some(vec![8, 4, 2])
        );
        assert_eq!(
            super::multi_state_from_attributes(Some("multi_state=[4,2,64]")),
            Some(vec![4, 2, 64])
        );
    }

    #[test]
    fn add_random_state_emits_chance_values_and_multi_state_weights() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "modifier": "AddRandomState",
            "operation": "Set",
            "trigger": "RoundStart",
            "target": "EnemyShip",
            "attributes": "multi_state=[8, 4, 2], num_rounds=3",
            "conditions": ["EnemyHostile"],
            "chance_by_rank": [0.4, 0.45, 0.55, 0.75, 1.0],
            "value_by_rank": [1.0, 1.0, 1.0, 1.0, 1.0]
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "Zeph").expect("effect");
        assert_eq!(e.tag.as_deref(), Some("addrandomstate:unmapped"));
        let scaling = e.scaling.as_ref().expect("scaling");
        assert_eq!(
            scaling.chance_values.as_ref().unwrap(),
            &vec![0.4, 0.45, 0.55, 0.75, 1.0]
        );
        assert_eq!(scaling.values.as_ref().unwrap(), &vec![8.0, 4.0, 2.0]);
    }

    #[test]
    fn fcm_data_shots_maps_to_lcars_with_round_duration_and_and_condition() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "officer",
            "trigger": "RoundStart",
            "modifier": "ShotsPerAttack",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "attributes": "num_rounds=2",
            "conditions": ["TargetNotSoloArmada", "TargetIsArmada"],
            "chance_by_rank": [1.0, 1.0, 1.0],
            "value_by_rank": [0.4, 0.3, 0.8]
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "FCM Data").expect("effect");
        assert_eq!(e.effect_type, "stat_modify");
        assert_eq!(e.stat.as_deref(), Some("shots_per_attack"));
        assert_eq!(e.trigger.as_deref(), Some("on_round_start"));
        match e.duration.as_ref() {
            Some(LcarsDuration::Rounds { rounds }) => assert_eq!(*rounds, 2),
            other => panic!("expected rounds duration, got {other:?}"),
        }
        let cond = e.condition.as_ref().expect("cond");
        assert_eq!(cond.condition_type, "and");
        let kids = cond.conditions.as_ref().expect("kids");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "engagement_includes");
        assert_eq!(kids[0].enemy_type.as_deref(), Some("group_armadas"));
        assert_eq!(kids[1].condition_type, "defender_ship_type_is");
        assert_eq!(kids[1].ship_type.as_deref(), Some("armada"));
    }

    #[test]
    fn hull_health_below_token_merges_to_stat_below_attacker_hull_threshold() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "officer",
            "trigger": "RoundStart",
            "modifier": "AllDamage",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "attributes": "percentage=0.6",
            "conditions": ["HullHealthBelow"],
            "chance_by_rank": [1.0],
            "value_by_rank": [0.1]
        }))
        .unwrap();
        let c = super::effect_condition_from_canonical(&a, "Chang", "AllDamage").expect("cond");
        assert_eq!(c.condition_type, "stat_below");
        assert_eq!(c.stat.as_deref(), Some("attacker_hull_hp"));
        assert_eq!(c.threshold_pct, Some(0.6));
    }

    #[test]
    fn hull_health_below_start_and_hull_health_above_merge_with_other_conds() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "captain",
            "trigger": "RoundStart",
            "modifier": "AddState",
            "operation": "Set",
            "target": "EnemyShip",
            "attributes": "num_rounds=1, percentage=0.3, state=8",
            "conditions": ["EnemyPlayer", "HullHealthBelowStartOfCombat", "HullHealthAbove"],
            "chance_by_rank": [0.8],
            "value_by_rank": [1.0]
        }))
        .unwrap();
        let c = super::effect_condition_from_canonical(&a, "TOS Kirk", "AddState").expect("cond");
        assert_eq!(c.condition_type, "and");
        let kids = c.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].condition_type, "defender_is_player_ship");
        assert_eq!(kids[1].condition_type, "stat_above");
        assert_eq!(kids[1].stat.as_deref(), Some("hull_hp"));
        assert_eq!(kids[1].threshold_pct, Some(0.3));
        assert_eq!(kids[2].condition_type, "stat_below");
        assert_eq!(kids[2].stat.as_deref(), Some("hull_hp"));
        assert_eq!(kids[2].threshold_pct, Some(0.3));
    }

    #[test]
    fn combat_battle_type_merges_to_condition_from_attributes() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "officer",
            "trigger": "CombatStart",
            "modifier": "AllPiercing",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "attributes": "battle_types=[4, 9, 4]",
            "conditions": ["CombatBattleType", "TargetHasHullBreach"],
            "chance_by_rank": [1.0],
            "value_by_rank": [0.1]
        }))
        .unwrap();
        let c = super::effect_condition_from_canonical(&a, "Data", "AllPiercing").expect("cond");
        assert_eq!(c.condition_type, "and");
        let kids = c.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "defender_hull_breach");
        assert_eq!(kids[1].condition_type, "combat_battle_type_any");
        assert_eq!(kids[1].battle_types, Some(vec![4, 9]));
    }

    #[test]
    fn target_max_level_merges_to_defender_level_condition() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "captain",
            "trigger": "CombatStart",
            "modifier": "AllDamage",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "attributes": "max_level=70",
            "conditions": ["TargetMaxLevel", "EnemyHostile"],
            "chance_by_rank": [1.0],
            "value_by_rank": [0.2]
        }))
        .unwrap();
        let c = super::effect_condition_from_canonical(&a, "Bones", "AllDamage").expect("cond");
        assert_eq!(c.condition_type, "and");
        let kids = c.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "defender_is_npc_hostile");
        assert_eq!(kids[1].condition_type, "defender_level_at_most");
        assert_eq!(kids[1].max, Some(70));
    }

    #[test]
    fn ent_e_data_isolytic_maps_enemy_hostile_and_target_not_armada() {
        let a: CanonicalAbility = serde_json::from_value(serde_json::json!({
            "slot": "officer",
            "trigger": "CombatStart",
            "modifier": "IsolyticCascadeDamage",
            "operation": "MultiplyAdd",
            "target": "SelfShip",
            "conditions": ["EnemyHostile", " TargetNotArmada"],
            "chance_by_rank": [1.0, 1.0, 1.0, 1.0, 1.0],
            "value_by_rank": [0.1, 0.15, 0.2, 0.3, 0.4]
        }))
        .unwrap();
        let e = convert_ability_to_effect(&a, "Ent-E Data").expect("effect");
        assert_eq!(e.stat.as_deref(), Some("isolytic_cascade_damage"));
        assert_eq!(e.trigger.as_deref(), Some("on_combat_start"));
        let cond = e.condition.as_ref().expect("cond");
        assert_eq!(cond.condition_type, "and");
        let kids = cond.conditions.as_ref().expect("kids");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "defender_is_npc_hostile");
        assert_eq!(kids[1].condition_type, "not");
        let inner = kids[1].conditions.as_ref().expect("not inner")[0].clone();
        assert_eq!(inner.condition_type, "defender_ship_type_is");
        assert_eq!(inner.ship_type.as_deref(), Some("armada"));
    }
}
