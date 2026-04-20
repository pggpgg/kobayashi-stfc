//! Maintainer-facing scans: canonical officer `conditions` token frequencies and hostile index
//! `upstream_ship_type` distribution. Used by `report_unknown_mappings` and the strict
//! `validate_data` report (see [`crate::data::validate`]).
//!
//! For triage context see `docs/CANONICAL_CONDITIONS.md` in the repo.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::data::upstream_hostile_ship_type::{
    upstream_hostile_ship_type_profile, upstream_ship_type_is_explicitly_mapped,
};
use crate::lcars::is_canonical_officer_condition_resolved;

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
    out
}
