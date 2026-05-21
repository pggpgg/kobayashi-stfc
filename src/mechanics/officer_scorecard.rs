//! Per-officer mechanics scorecard: combat coverage (0–100), unmapped-tag penalties, manual fidelity merge.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::lcars::{
    lcars_effect_coverage, lcars_effect_to_combat_effect_spec_with_report, load_lcars_dir,
    LcarsAbility, LcarsDropReport, LcarsEffect, LcarsOfficer, MechanicCoverageTier, ResolveOptions,
};

use super::coverage::TierCounts;

/// Weight applied to per-effect raw scores when computing [OfficerScorecardRow::combat_weighted].
pub const WEIGHT_CAPTAIN: f64 = 2.0;
pub const WEIGHT_BRIDGE: f64 = 1.5;
pub const WEIGHT_BELOW: f64 = 1.0;

/// Each combat-intent `type: tag` without `:non_combat` adds this much to [OfficerScorecardRow::unmapped_penalty] (capped at 100).
pub const UNMAPPED_TAG_PENALTY_PER_LINE: i32 = 25;

#[derive(Debug, Clone)]
pub struct OfficerScorecardRow {
    pub id: String,
    pub name: String,
    pub combat_n: u32,
    pub cap_ipc: TierCounts,
    pub br_ipc: TierCounts,
    pub bd_ipc: TierCounts,
    pub unmapped_combat_tags: u32,
    /// Mean of raw 0/50/100 over combat-intent effects; `None` if `combat_n == 0`.
    pub combat_avg: Option<i32>,
    /// Weighted mean (captain 2.0, bridge 1.5, below 1.0); `None` if `combat_n == 0`.
    pub combat_weighted: Option<i32>,
    /// Deduction 0–100 from unmapped combat tags; 0 if none.
    pub unmapped_penalty: i32,
    /// `combat_weighted - unmapped_penalty`, clamped 0–100; `None` if `combat_n == 0`.
    pub combat_auto: Option<i32>,
    pub grade: String,
    /// Non-combat tag acknowledgment 0–100.
    pub nc_ack: i32,
    pub noncombat_label: String,
    /// Mean raw 0–100 for combat-intent effects in the captain block only.
    pub cap_score: Option<i32>,
    pub br_score: Option<i32>,
    pub bd_score: Option<i32>,
    pub fidelity: String,
    /// Combat-intent effects dropped by the LCARS→IR adapter at load time due to an unknown
    /// `trigger` (see `effect_trigger_timing`). Populated from [`LcarsDropReport`].
    pub dropped_unknown_trigger: u32,
    /// Combat-intent effects dropped due to an unmapped tag (parallels [`Self::unmapped_combat_tags`]
    /// but sourced from the adapter's drop report — divergence indicates a wiring bug).
    pub dropped_unmapped_tag: u32,
    /// Combat-intent `stat_modify` effects dropped because `stat_to_officer_modifier` couldn't
    /// map the stat name.
    pub dropped_unmapped_stat: u32,
    /// Combat-intent effects dropped because their `condition` block couldn't be represented in
    /// [`crate::data::combat_effect_spec::AbilityConditionSpec`].
    pub dropped_unmapped_condition: u32,
}

/// Internal: rolling drop counters used while building a row.
#[derive(Debug, Default)]
struct DropTallies {
    unmapped_combat_tags: u32,
    unknown_trigger: u32,
    unmapped_tag: u32,
    unmapped_stat: u32,
    unmapped_condition: u32,
}

fn tag_is_non_combat(tag: Option<&str>) -> bool {
    tag.is_some_and(|t| t.to_ascii_lowercase().contains(":non_combat"))
}

/// Effects that count toward combat scoring (excludes economy-only tags).
pub fn effect_is_combat_intent(effect: &LcarsEffect) -> bool {
    if effect.effect_type.trim().eq_ignore_ascii_case("tag") {
        !tag_is_non_combat(effect.tag.as_deref())
    } else {
        true
    }
}

fn tier_raw_score(tier: MechanicCoverageTier) -> i32 {
    match tier {
        MechanicCoverageTier::Implemented => 100,
        MechanicCoverageTier::Partial => 50,
        MechanicCoverageTier::Ignored => 0,
    }
}

fn bump_ipc(counts: &mut TierCounts, tier: MechanicCoverageTier) {
    match tier {
        MechanicCoverageTier::Implemented => counts.implemented += 1,
        MechanicCoverageTier::Partial => counts.partial += 1,
        MechanicCoverageTier::Ignored => counts.ignored += 1,
    }
}

fn slot_weight(is_captain: bool, is_bridge: bool, is_below: bool) -> f64 {
    if is_captain {
        WEIGHT_CAPTAIN
    } else if is_bridge {
        WEIGHT_BRIDGE
    } else if is_below {
        WEIGHT_BELOW
    } else {
        1.0
    }
}

fn mean_round(scores: &[i32]) -> Option<i32> {
    if scores.is_empty() {
        return None;
    }
    let sum: i32 = scores.iter().sum();
    Some((sum as f64 / scores.len() as f64).round() as i32)
}

fn weighted_mean(pairs: &[(i32, f64)]) -> Option<i32> {
    if pairs.is_empty() {
        return None;
    }
    let mut num = 0.0_f64;
    let mut den = 0.0_f64;
    for (s, w) in pairs {
        num += *s as f64 * w;
        den += w;
    }
    if den <= 0.0 {
        return None;
    }
    Some((num / den).round() as i32)
}

fn grade_for_combat_auto(score: i32) -> &'static str {
    match score {
        90..=100 => "A",
        80..=89 => "B",
        65..=79 => "C",
        50..=64 => "D",
        _ => "F",
    }
}

/// Classify all `type: tag` effects on the officer for `nc_ack` / `noncombat_label`.
fn analyze_tags(officer: &LcarsOfficer) -> (usize, usize, usize) {
    let mut total = 0usize;
    let mut non_combat = 0usize;
    let mut combatish = 0usize;
    for ab in [
        officer.captain_ability.as_ref(),
        officer.bridge_ability.as_ref(),
        officer.below_decks_ability.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        for eff in &ab.effects {
            if !eff.effect_type.trim().eq_ignore_ascii_case("tag") {
                continue;
            }
            total += 1;
            if tag_is_non_combat(eff.tag.as_deref()) {
                non_combat += 1;
            } else {
                combatish += 1;
            }
        }
    }
    (total, non_combat, combatish)
}

fn nc_ack_and_label(
    total: usize,
    non_combat: usize,
    combatish: usize,
    unmapped_combat_tags: u32,
) -> (i32, String) {
    if total == 0 {
        return (100, "none".to_string());
    }
    if combatish == 0 {
        return (100, "economy_only".to_string());
    }
    if unmapped_combat_tags > 0 {
        return (0, "combat_tag_gaps".to_string());
    }
    if non_combat > 0 {
        return (50, "mixed".to_string());
    }
    (100, "none".to_string())
}

#[allow(clippy::too_many_arguments)]
fn process_ability_effects(
    ability: &LcarsAbility,
    officer: &LcarsOfficer,
    opts: &ResolveOptions,
    is_captain: bool,
    is_bridge: bool,
    is_below: bool,
    ipc: &mut TierCounts,
    raw_scores: &mut Vec<i32>,
    weighted_pairs: &mut Vec<(i32, f64)>,
    tallies: &mut DropTallies,
) {
    let w = slot_weight(is_captain, is_bridge, is_below);
    for (idx, eff) in ability.effects.iter().enumerate() {
        if !effect_is_combat_intent(eff) {
            continue;
        }
        let cov = lcars_effect_coverage(eff, &officer.id, opts);
        bump_ipc(ipc, cov.tier);
        let mut raw = tier_raw_score(cov.tier);
        if eff.effect_type.trim().eq_ignore_ascii_case("tag") {
            let tag_str = eff.tag.as_deref().unwrap_or("");
            // Mapped tags that can be resolved as stat_modify equivalents get the
            // coverage-tier score; only truly unmapped tags are zero.
            let mapped = crate::lcars::effect_spec_adapter::combat_tag_to_stat(tag_str).is_some();
            if mapped {
                // Keep the raw score from cov.tier and IPC counts as-is.
            } else {
                raw = 0;
                tallies.unmapped_combat_tags += 1;
            }
        }
        raw_scores.push(raw);
        weighted_pairs.push((raw, w));

        // Cross-check via the LCARS→IR adapter's drop report so the scorecard reflects every
        // category the adapter recognizes (unknown_trigger / unmapped_stat / unmapped_condition
        // in addition to unmapped_tag). Officer stats are unused here — drop categorization
        // doesn't depend on scaling.
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
        for drop in &report.drops {
            match LcarsDropReport::reason_category(&drop.reason) {
                "unknown_trigger" => tallies.unknown_trigger += 1,
                "unmapped_tag" => tallies.unmapped_tag += 1,
                "unmapped_stat" => tallies.unmapped_stat += 1,
                "unmapped_condition" => tallies.unmapped_condition += 1,
                _ => {} // extra_attack_unsupported / unknown_effect_type — not surfaced here
            }
        }
    }
}

/// Build one scorecard row for a single officer definition.
pub fn scorecard_row_for_officer(
    officer: &LcarsOfficer,
    opts: &ResolveOptions,
    fidelity: &str,
) -> OfficerScorecardRow {
    let mut cap_ipc = TierCounts::default();
    let mut br_ipc = TierCounts::default();
    let mut bd_ipc = TierCounts::default();
    let mut raw_scores: Vec<i32> = Vec::new();
    let mut weighted_pairs: Vec<(i32, f64)> = Vec::new();
    let mut tallies = DropTallies::default();

    if let Some(ref a) = officer.captain_ability {
        process_ability_effects(
            a,
            officer,
            opts,
            true,
            false,
            false,
            &mut cap_ipc,
            &mut raw_scores,
            &mut weighted_pairs,
            &mut tallies,
        );
    }
    if let Some(ref a) = officer.bridge_ability {
        process_ability_effects(
            a,
            officer,
            opts,
            false,
            true,
            false,
            &mut br_ipc,
            &mut raw_scores,
            &mut weighted_pairs,
            &mut tallies,
        );
    }
    if let Some(ref a) = officer.below_decks_ability {
        process_ability_effects(
            a,
            officer,
            opts,
            false,
            false,
            true,
            &mut bd_ipc,
            &mut raw_scores,
            &mut weighted_pairs,
            &mut tallies,
        );
    }

    let combat_n = raw_scores.len() as u32;
    let combat_avg = mean_round(&raw_scores);
    let combat_weighted = weighted_mean(&weighted_pairs);
    let unmapped_penalty =
        (tallies.unmapped_combat_tags as i32 * UNMAPPED_TAG_PENALTY_PER_LINE).min(100);

    let combat_auto = if combat_n == 0 {
        None
    } else {
        let base = combat_weighted.unwrap_or(0);
        Some((base - unmapped_penalty).clamp(0, 100))
    };

    let grade = combat_auto
        .map(grade_for_combat_auto)
        .map(str::to_string)
        .unwrap_or_else(|| "—".to_string());

    let (tag_total, tag_nc, tag_combat) = analyze_tags(officer);
    let (nc_ack, noncombat_label) =
        nc_ack_and_label(tag_total, tag_nc, tag_combat, tallies.unmapped_combat_tags);

    let (cap_score, br_score, bd_score) = slot_raw_means(officer, opts);

    OfficerScorecardRow {
        id: officer.id.clone(),
        name: officer.name.clone(),
        combat_n,
        cap_ipc,
        br_ipc,
        bd_ipc,
        unmapped_combat_tags: tallies.unmapped_combat_tags,
        combat_avg,
        combat_weighted,
        unmapped_penalty,
        combat_auto,
        grade,
        nc_ack,
        noncombat_label,
        cap_score,
        br_score,
        bd_score,
        fidelity: fidelity.to_string(),
        dropped_unknown_trigger: tallies.unknown_trigger,
        dropped_unmapped_tag: tallies.unmapped_tag,
        dropped_unmapped_stat: tallies.unmapped_stat,
        dropped_unmapped_condition: tallies.unmapped_condition,
    }
}

/// Load optional fidelity map from YAML. Expected: top-level `officer_id: "note"` entries.
pub fn load_officer_fidelity_map(path: &Path) -> Result<HashMap<String, String>, String> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_yaml::from_str::<HashMap<String, String>>(&raw).map_err(|e| e.to_string())
}

/// Build rows for all officers in `dir`, merge fidelity, warn on unknown fidelity keys.
pub fn build_officer_scorecard_rows(
    lcars_dir: &Path,
    fidelity_path: &Path,
) -> Result<Vec<OfficerScorecardRow>, Box<dyn std::error::Error + Send + Sync>> {
    let officers = load_lcars_dir(lcars_dir)?;
    let fidelity_map = load_officer_fidelity_map(fidelity_path)
        .map_err(|s| std::io::Error::new(std::io::ErrorKind::InvalidData, s))?;

    let id_set: HashSet<&str> = officers.iter().map(|o| o.id.as_str()).collect();
    for key in fidelity_map.keys() {
        if !id_set.contains(key.as_str()) {
            eprintln!(
                "generate_officer_scorecard: unknown fidelity key (not an LCARS officer id): {key}"
            );
        }
    }

    let opts = ResolveOptions {
        tier: Some(5),
        officer_tiers: None,
        officer_levels: None,
    };

    let mut rows: Vec<OfficerScorecardRow> = officers
        .iter()
        .map(|o| {
            let note = fidelity_map.get(&o.id).map(|s| s.as_str()).unwrap_or("—");
            scorecard_row_for_officer(o, &opts, note)
        })
        .collect();

    rows.sort_by(|a, b| {
        use std::cmp::Ordering;
        let a_empty = a.combat_n == 0;
        let b_empty = b.combat_n == 0;
        match (a_empty, b_empty) {
            (true, true) => a.id.cmp(&b.id),
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => a
                .combat_auto
                .unwrap()
                .cmp(&b.combat_auto.unwrap())
                .then_with(|| b.unmapped_combat_tags.cmp(&a.unmapped_combat_tags))
                .then_with(|| a.id.cmp(&b.id)),
        }
    });

    Ok(rows)
}

fn mean_slot_raw(
    officer: &LcarsOfficer,
    opts: &ResolveOptions,
    pick: fn(&LcarsOfficer) -> Option<&LcarsAbility>,
) -> Option<i32> {
    let ab = pick(officer)?;
    let mut scores = Vec::new();
    for eff in &ab.effects {
        if !effect_is_combat_intent(eff) {
            continue;
        }
        let cov = lcars_effect_coverage(eff, &officer.id, opts);
        let mut raw = tier_raw_score(cov.tier);
        if eff.effect_type.trim().eq_ignore_ascii_case("tag") {
            let tag_str = eff.tag.as_deref().unwrap_or("");
            // Mapped tags use coverage-tier score; only unmapped tags are zero.
            let mapped = crate::lcars::effect_spec_adapter::combat_tag_to_stat(tag_str).is_some();
            if !mapped {
                raw = 0;
            }
        }
        scores.push(raw);
    }
    mean_round(&scores)
}

/// Exposed for tests: per-slot mean raw (captain / bridge / below ability blocks only).
pub fn slot_raw_means(
    officer: &LcarsOfficer,
    opts: &ResolveOptions,
) -> (Option<i32>, Option<i32>, Option<i32>) {
    let cap = mean_slot_raw(officer, opts, |o| o.captain_ability.as_ref());
    let br = mean_slot_raw(officer, opts, |o| o.bridge_ability.as_ref());
    let bd = mean_slot_raw(officer, opts, |o| o.below_decks_ability.as_ref());
    (cap, br, bd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcars::LcarsOfficer;

    fn opts() -> ResolveOptions {
        ResolveOptions {
            tier: Some(5),
            officer_tiers: None,
            officer_levels: None,
        }
    }

    #[test]
    fn only_non_combat_tag_has_combat_n_zero() {
        let officer = LcarsOfficer {
            id: "test-nc".into(),
            name: "Test".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(crate::lcars::LcarsAbility {
                name: "C".into(),
                effects: vec![LcarsEffect {
                    effect_type: "tag".into(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: None,
                    duration: None,
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: Some("cargocapacity:non_combat".into()),
                    accumulate: None,
                    decay: None,
                }],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let row = scorecard_row_for_officer(&officer, &opts(), "—");
        assert_eq!(row.combat_n, 0);
        assert_eq!(row.unmapped_combat_tags, 0);
        assert_eq!(row.nc_ack, 100);
        assert_eq!(row.noncombat_label, "economy_only");
    }

    #[test]
    fn mapped_combat_tag_does_not_set_combat_tag_gaps_label() {
        let officer = LcarsOfficer {
            id: "test-mapped".into(),
            name: "Test".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(crate::lcars::LcarsAbility {
                name: "C".into(),
                effects: vec![LcarsEffect {
                    effect_type: "tag".into(),
                    stat: None,
                    target: Some("self".into()),
                    operator: Some("multiply".into()),
                    value: Some(1.08),
                    trigger: Some("passive".into()),
                    duration: Some(crate::lcars::LcarsDuration::Permanent("permanent".into())),
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: Some("officerstatall:unmapped".into()),
                    accumulate: None,
                    decay: None,
                }],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let row = scorecard_row_for_officer(&officer, &opts(), "—");
        assert_eq!(row.unmapped_combat_tags, 0);
        assert_eq!(row.nc_ack, 100);
        assert_eq!(row.noncombat_label, "none");
    }

    #[test]
    fn unmapped_tag_counts_and_penalizes() {
        let officer = LcarsOfficer {
            id: "test-um".into(),
            name: "Test".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(crate::lcars::LcarsAbility {
                name: "C".into(),
                effects: vec![LcarsEffect {
                    effect_type: "tag".into(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: None,
                    duration: None,
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: Some("x:unmapped".into()),
                    accumulate: None,
                    decay: None,
                }],
            }),
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let row = scorecard_row_for_officer(&officer, &opts(), "—");
        assert_eq!(row.combat_n, 1);
        assert_eq!(row.unmapped_combat_tags, 1);
        assert_eq!(row.unmapped_penalty, 25);
        assert_eq!(row.combat_weighted, Some(0));
        assert_eq!(row.combat_auto, Some(0));
        assert_eq!(row.nc_ack, 0);
        assert_eq!(row.noncombat_label, "combat_tag_gaps");
    }

    #[test]
    fn captain_weight_pulls_above_avg_when_bridge_is_worse() {
        let officer = LcarsOfficer {
            id: "test-w".into(),
            name: "Test".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: Some(crate::lcars::LcarsAbility {
                name: "Cap".into(),
                effects: vec![LcarsEffect {
                    effect_type: "extra_attack".into(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: None,
                    duration: None,
                    scaling: None,
                    condition: None,
                    chance: Some(0.5),
                    multiplier: Some(2.0),
                    tag: None,
                    accumulate: None,
                    decay: None,
                }],
            }),
            bridge_ability: Some(crate::lcars::LcarsAbility {
                name: "Br".into(),
                effects: vec![LcarsEffect {
                    effect_type: "tag".into(),
                    stat: None,
                    target: None,
                    operator: None,
                    value: None,
                    trigger: None,
                    duration: None,
                    scaling: None,
                    condition: None,
                    chance: None,
                    multiplier: None,
                    tag: Some("x:unmapped".into()),
                    accumulate: None,
                    decay: None,
                }],
            }),
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let row = scorecard_row_for_officer(&officer, &opts(), "—");
        // raw: 100, 0 → avg 50; weighted: (100*2 + 0*1.5)/3.5 ≈ 57
        assert_eq!(row.combat_avg, Some(50));
        assert_eq!(row.combat_weighted, Some(57));
        assert_eq!(row.unmapped_penalty, 25);
        assert_eq!(row.combat_auto, Some(32));
    }

    #[test]
    fn fidelity_note_passes_through_to_row() {
        let officer = LcarsOfficer {
            id: "x".into(),
            name: "X".into(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
            stats: Vec::new(),
            max_level_by_rank: Vec::new(),
        };
        let row = scorecard_row_for_officer(&officer, &opts(), "Manual review: wrong target.");
        assert_eq!(row.fidelity, "Manual review: wrong target.");
    }

    #[test]
    fn fidelity_map_parses_flat_yaml() {
        let dir = std::env::temp_dir();
        let p = dir.join("fidelity_test.yaml");
        fs::write(&p, "a-officer: \"hello pipe\"\n").unwrap();
        let m = load_officer_fidelity_map(&p).unwrap();
        assert_eq!(m.get("a-officer").map(|s| s.as_str()), Some("hello pipe"));
        let _ = fs::remove_file(p);
    }
}
