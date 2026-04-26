//! Maintainer-facing scans: canonical officer `conditions` token frequencies and hostile index
//! `upstream_ship_type` distribution (including undocumented-value triage). Used by
//! `report_unknown_mappings` and the strict `validate_data` report (see [`crate::data::validate`]).
//!
//! For triage context see `docs/CANONICAL_CONDITIONS.md` in the repo.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::data::upstream_hostile_ship_type::{
    upstream_hostile_ship_type_profile, upstream_ship_type_deferral_reason,
    upstream_ship_type_is_explicitly_mapped, upstream_ship_type_is_known_category,
};
use crate::data::validate::is_known_building_condition;
use crate::lcars::is_canonical_officer_condition_resolved;

/// Cap on sample building/officer ids retained per gap row in maintainer reports.
const MAX_GAP_SAMPLES: usize = 4;

#[derive(Debug, Deserialize)]
struct CanonicalFile {
    officers: Vec<CanonicalOfficer>,
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficer {
    id: String,
    name: String,
    abilities: Vec<CanonicalAbility>,
}

#[derive(Debug, Deserialize)]
struct CanonicalAbility {
    slot: String,
    #[serde(default)]
    conditions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HostileIndex {
    #[serde(default)]
    hostiles: Vec<HostileIndexEntry>,
}

#[derive(Debug, Deserialize)]
struct HostileIndexEntry {
    id: String,
    #[serde(default)]
    upstream_ship_type: u32,
}

/// Per-token aggregation from canonical officer JSON.
#[derive(Debug, Clone, Default)]
pub struct CanonicalConditionTokenAgg {
    pub count: usize,
    pub examples: Vec<String>,
}

/// Per-upstream-value aggregation from hostile index JSON.
#[derive(Debug, Clone, Default)]
pub struct HostileUpstreamTypeAgg {
    pub count: usize,
    pub sample_ids: Vec<String>,
}

pub fn md_escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Scan `officers.canonical.json` for non-empty `conditions` tokens and example locations.
pub fn scan_canonical_officer_conditions(
    path: &Path,
) -> Result<HashMap<String, CanonicalConditionTokenAgg>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: CanonicalFile =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut map: HashMap<String, CanonicalConditionTokenAgg> = HashMap::new();

    for officer in &file.officers {
        for ability in &officer.abilities {
            for raw in &ability.conditions {
                let tok = raw.trim();
                if tok.is_empty() {
                    continue;
                }
                let entry = map.entry(tok.to_string()).or_default();
                entry.count += 1;
                if entry.examples.len() < 4 {
                    entry.examples.push(format!(
                        "`{}` {} ({})",
                        md_escape_cell(&officer.id),
                        md_escape_cell(&officer.name),
                        md_escape_cell(&ability.slot)
                    ));
                }
            }
        }
    }

    Ok(map)
}

/// Scan hostile `index.json` rows for `upstream_ship_type` value distribution.
pub fn scan_hostile_index_upstream_ship_types(
    path: &Path,
) -> Result<BTreeMap<u32, HostileUpstreamTypeAgg>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let idx: HostileIndex =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut map: BTreeMap<u32, HostileUpstreamTypeAgg> = BTreeMap::new();

    for row in &idx.hostiles {
        let e = map.entry(row.upstream_ship_type).or_default();
        e.count += 1;
        if e.sample_ids.len() < 4 {
            e.sample_ids.push(row.id.clone());
        }
    }

    Ok(map)
}

/// Sorted rows: token, occurrence count, example strings (markdown-safe cells).
pub fn unmapped_canonical_condition_rows(
    token_map: &HashMap<String, CanonicalConditionTokenAgg>,
) -> Vec<(String, usize, Vec<String>)> {
    let mut unmapped_rows: Vec<(String, usize, Vec<String>)> = Vec::new();

    for (tok, agg) in token_map {
        if !is_canonical_officer_condition_resolved(tok) {
            unmapped_rows.push((tok.clone(), agg.count, agg.examples.clone()));
        }
    }

    unmapped_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    unmapped_rows
}

/// Maintainer Markdown (same shape as the historical `report_unknown_mappings` output).
pub fn format_unknown_mappings_markdown(
    canonical_path: &Path,
    hostile_path: &Path,
    token_map: &HashMap<String, CanonicalConditionTokenAgg>,
    ship_map: &BTreeMap<u32, HostileUpstreamTypeAgg>,
) -> String {
    let mut distinct = 0usize;
    let mut mapped_tokens = 0usize;
    let unmapped_rows = unmapped_canonical_condition_rows(token_map);
    for tok in token_map.keys() {
        distinct += 1;
        if is_canonical_officer_condition_resolved(tok) {
            mapped_tokens += 1;
        }
    }

    let mut out = String::new();
    out.push_str("# Unknown mappings report\n\n");
    out.push_str("## Inputs\n\n");
    out.push_str(&format!(
        "- Canonical officers: `{}`\n- Hostile index: `{}`\n\n",
        canonical_path.display(),
        hostile_path.display()
    ));

    out.push_str("## Canonical condition tokens\n\n");
    out.push_str(
        "See repo path `docs/CANONICAL_CONDITIONS.md` for triage buckets (mapped vs deferred).\n\n",
    );
    out.push_str(&format!(
        "- Distinct non-empty tokens: **{}**\n- Resolved for officer LCARS (`map_canonical_condition_token` and `generate_lcars` merges): **{}**\n- Still unmapped: **{}**\n\n",
        distinct,
        mapped_tokens,
        unmapped_rows.len()
    ));

    if unmapped_rows.is_empty() {
        out.push_str("No unmapped canonical condition tokens.\n\n");
    } else {
        out.push_str("### Unmapped tokens\n\n");
        out.push_str("| Token | Occurrences | Examples |\n| --- | ---: | --- |\n");
        for (tok, count, examples) in &unmapped_rows {
            let ex = examples.join("; ");
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                md_escape_cell(tok),
                count,
                ex
            ));
        }
        out.push('\n');
    }

    out.push_str("## Hostile `upstream_ship_type`\n\n");
    out.push_str("Values from hostile index entries; `explicitly_mapped` matches dedicated `match` arms in `upstream_hostile_ship_type.rs`.\n\n");
    out.push_str("| Value | Hostile rows | Explicitly mapped | `is_armada_target` | Note (static) | Sample ids |\n| --- | ---: | :---: | :---: | --- | --- |\n");

    for (value, agg) in ship_map {
        let profile = upstream_hostile_ship_type_profile(*value);
        let mapped = upstream_ship_type_is_explicitly_mapped(*value);
        let ids = agg
            .sample_ids
            .iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            value,
            agg.count,
            if mapped { "yes" } else { "no" },
            if profile.is_armada_target {
                "yes"
            } else {
                "no"
            },
            md_escape_cell(profile.note),
            ids
        ));
    }

    out.push('\n');

    out.push_str("### Undocumented `upstream_ship_type` values\n\n");
    out.push_str(
        "Values present in the hostile index that are **not** in `KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES` (`upstream_hostile_ship_type.rs`). \
`validate_data` treats these as errors unless listed in `DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES` (warnings).\n\n",
    );

    let mut undocumented: Vec<(u32, &HostileUpstreamTypeAgg)> = ship_map
        .iter()
        .filter(|(v, _)| !upstream_ship_type_is_known_category(**v))
        .map(|(v, agg)| (*v, agg))
        .collect();
    undocumented.sort_by_key(|(v, _)| *v);

    if undocumented.is_empty() {
        out.push_str("None — every index value is documented.\n\n");
    } else {
        out.push_str(
            "| Value | Hostile rows | `validate_data` | Sample ids |\n| --- | ---: | --- | --- |\n",
        );
        for (value, agg) in &undocumented {
            let ids = agg
                .sample_ids
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let validation = match upstream_ship_type_deferral_reason(*value) {
                Some(r) => format!("deferred (warn): {}", md_escape_cell(r)),
                None => "undocumented (error)".to_string(),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                value, agg.count, validation, ids
            ));
        }
        out.push('\n');
    }

    out
}

/// Per-distinct-key aggregation for building bonus mapping gaps.
#[derive(Debug, Clone, Default)]
pub struct BuildingGapAgg {
    /// Total bonus rows (or condition occurrences) referencing this key across all buildings.
    pub count: usize,
    /// Up to [`MAX_GAP_SAMPLES`] deduped building ids that contain the key, in encounter order.
    pub samples: Vec<String>,
}

impl BuildingGapAgg {
    fn record(&mut self, building_id: &str) {
        self.count += 1;
        if self.samples.len() < MAX_GAP_SAMPLES
            && !self.samples.iter().any(|s| s == building_id)
        {
            self.samples.push(building_id.to_string());
        }
    }
}

/// Aggregated mapping gaps for the buildings dataset.
///
/// - `opaque_buff_stats`: `BonusEntry.stat` values starting with `buff_*` that
///   `normalize_profile_combat_stat` (in [`crate::data::profile`]) does not lower into a combat key,
///   so these bonuses never reach the simulator.
/// - `unknown_conditions`: `BonusEntry.conditions` tokens that
///   [`is_known_building_condition`] does not recognize, so building-mode filtering treats them as
///   pass-through. Confirm semantics before extending the allowlist.
#[derive(Debug, Clone, Default)]
pub struct BuildingBonusGapsReport {
    pub opaque_buff_stats: BTreeMap<String, BuildingGapAgg>,
    pub unknown_conditions: BTreeMap<String, BuildingGapAgg>,
}

impl BuildingBonusGapsReport {
    pub fn is_empty(&self) -> bool {
        self.opaque_buff_stats.is_empty() && self.unknown_conditions.is_empty()
    }
}

/// Walk `buildings_dir/index.json` plus per-building files and collect mapping gaps.
///
/// Errors only on I/O / JSON failures for the index file; per-building file errors are silently
/// skipped so the maintainer report does not fail when a single record is malformed (the structural
/// `validate_buildings_dataset` flow surfaces those as separate diagnostics).
pub fn scan_building_bonus_gaps(buildings_dir: &Path) -> Result<BuildingBonusGapsReport, String> {
    let index_path = buildings_dir.join("index.json");
    let raw = fs::read_to_string(&index_path)
        .map_err(|e| format!("read {}: {e}", index_path.display()))?;
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", index_path.display()))?;

    let buildings = payload
        .get("buildings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{}: missing 'buildings' array", index_path.display()))?;

    let mut report = BuildingBonusGapsReport::default();

    for entry in buildings {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let Some(id) = obj
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let file_stem = obj
            .get("file")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        let path = buildings_dir.join(format!("{file_stem}.json"));
        let Ok(rec_raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(rec) = serde_json::from_str::<Value>(&rec_raw) else {
            continue;
        };
        let Some(levels) = rec.get("levels").and_then(Value::as_array) else {
            continue;
        };

        for level in levels {
            let Some(bonuses) = level.get("bonuses").and_then(Value::as_array) else {
                continue;
            };
            for bonus in bonuses {
                let Some(bo) = bonus.as_object() else {
                    continue;
                };
                if let Some(stat) = bo.get("stat").and_then(Value::as_str) {
                    if stat.starts_with("buff_") {
                        report
                            .opaque_buff_stats
                            .entry(stat.to_string())
                            .or_default()
                            .record(id);
                    }
                }
                if let Some(conds) = bo.get("conditions").and_then(Value::as_array) {
                    for c in conds {
                        if let Some(s) = c.as_str() {
                            if !is_known_building_condition(s) {
                                report
                                    .unknown_conditions
                                    .entry(s.to_string())
                                    .or_default()
                                    .record(id);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(report)
}

/// Maintainer Markdown for the building bonus mapping gap tables (matches the shape historically
/// emitted by `report_building_mapping_gaps`).
pub fn format_building_bonus_gaps_markdown(
    report: &BuildingBonusGapsReport,
    buildings_dir: &Path,
) -> String {
    let mut out = String::new();
    out.push_str("# Building bonus mapping gaps\n\n");
    out.push_str(&format!("Directory: `{}`\n\n", buildings_dir.display()));

    out.push_str("## Opaque `buff_*` stats\n\n");
    out.push_str(
        "These keys are not merged into the player combat profile (see `merge_building_bonuses_into_profile` / `normalize_profile_combat_stat` in `src/data/profile.rs`).\n\n",
    );
    if report.opaque_buff_stats.is_empty() {
        out.push_str("None.\n\n");
    } else {
        out.push_str("| Stat | Bonus rows | Sample building ids |\n");
        out.push_str("| --- | ---: | --- |\n");
        for (k, v) in &report.opaque_buff_stats {
            let samples = v.samples.join(", ");
            out.push_str(&format!("| `{k}` | {} | {samples} |\n", v.count));
        }
        out.push('\n');
    }

    out.push_str("## Conditions not in `is_known_building_condition`\n\n");
    if report.unknown_conditions.is_empty() {
        out.push_str("None.\n\n");
    } else {
        out.push_str("| Condition | Occurrences | Sample building ids |\n");
        out.push_str("| --- | ---: | --- |\n");
        for (k, v) in &report.unknown_conditions {
            let samples = v.samples.join(", ");
            out.push_str(&format!("| `{k}` | {} | {samples} |\n", v.count));
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("{label}_{nanos}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_index(dir: &Path, ids: &[&str]) {
        let entries: Vec<String> = ids
            .iter()
            .map(|id| format!(r#"{{"id":"{id}","building_name":"{id}","file":"{id}"}}"#))
            .collect();
        let body = format!(r#"{{"buildings":[{}]}}"#, entries.join(","));
        fs::write(dir.join("index.json"), body).unwrap();
    }

    #[test]
    fn scan_building_bonus_gaps_collects_buff_stats_and_unknown_conditions() {
        let dir = unique_tmp_dir("mapping_gap_buildings");
        write_index(&dir, &["alpha", "beta"]);
        fs::write(
            dir.join("alpha.json"),
            r#"{"id":"alpha","building_name":"alpha","levels":[
                {"level":1,"bonuses":[
                    {"stat":"buff_unknown_x","value":0.1,"operator":"add"},
                    {"stat":"weapon_damage","value":0.05,"operator":"add",
                     "conditions":["mystery_condition"]}
                ]}
            ]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("beta.json"),
            r#"{"id":"beta","building_name":"beta","levels":[
                {"level":1,"bonuses":[
                    {"stat":"buff_unknown_x","value":0.2,"operator":"add"},
                    {"stat":"hull_hp","value":0.05,"operator":"add",
                     "conditions":["ship_combat_only","mystery_condition"]}
                ]}
            ]}"#,
        )
        .unwrap();

        let report = scan_building_bonus_gaps(&dir).expect("scan");
        let _ = fs::remove_dir_all(&dir);

        let buff = report
            .opaque_buff_stats
            .get("buff_unknown_x")
            .expect("buff key");
        assert_eq!(buff.count, 2);
        assert_eq!(buff.samples, vec!["alpha".to_string(), "beta".to_string()]);

        let cond = report
            .unknown_conditions
            .get("mystery_condition")
            .expect("condition key");
        assert_eq!(cond.count, 2);
        assert_eq!(cond.samples, vec!["alpha".to_string(), "beta".to_string()]);
        assert!(!report.unknown_conditions.contains_key("ship_combat_only"));
    }

    #[test]
    fn scan_building_bonus_gaps_caps_samples_and_dedupes_per_building() {
        let dir = unique_tmp_dir("mapping_gap_buildings_dedupe");
        let ids = ["a", "b", "c", "d", "e"];
        write_index(&dir, &ids);
        for id in &ids {
            fs::write(
                dir.join(format!("{id}.json")),
                format!(
                    r#"{{"id":"{id}","building_name":"{id}","levels":[
                        {{"level":1,"bonuses":[
                            {{"stat":"buff_repeat","value":0.1,"operator":"add"}},
                            {{"stat":"buff_repeat","value":0.2,"operator":"add"}}
                        ]}}
                    ]}}"#
                ),
            )
            .unwrap();
        }
        let report = scan_building_bonus_gaps(&dir).expect("scan");
        let _ = fs::remove_dir_all(&dir);

        let agg = report.opaque_buff_stats.get("buff_repeat").expect("agg");
        assert_eq!(agg.count, ids.len() * 2, "every bonus row counts");
        assert_eq!(
            agg.samples.len(),
            MAX_GAP_SAMPLES,
            "samples capped at MAX_GAP_SAMPLES"
        );
        assert_eq!(
            agg.samples,
            vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()],
            "samples are deduped per building and preserve encounter order"
        );
    }

    #[test]
    fn format_building_bonus_gaps_markdown_renders_tables() {
        let mut report = BuildingBonusGapsReport::default();
        report
            .opaque_buff_stats
            .entry("buff_x".to_string())
            .or_default()
            .record("alpha");
        report
            .unknown_conditions
            .entry("mystery_condition".to_string())
            .or_default()
            .record("beta");

        let md = format_building_bonus_gaps_markdown(&report, Path::new("data/buildings"));
        assert!(md.contains("# Building bonus mapping gaps"));
        assert!(md.contains("| `buff_x` | 1 | alpha |"));
        assert!(md.contains("| `mystery_condition` | 1 | beta |"));
    }
}
