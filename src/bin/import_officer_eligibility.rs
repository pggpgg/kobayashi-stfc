//! Import the officer eligibility matrix from the community cheat-sheet CSV.
//!
//! Reads `data/upstream/cheat-sheet/raw-officers-*.csv` (one row per ability) and writes
//! `data/officers/eligibility_matrix.json` keyed by `AbilityID`, then updates `data/registry.json`.
//!
//! Join keys: CSV `AbilityID` == our `OfficerAbility.ability_id`; CSV `OfficerId` ==
//! `Officer.source_officer_id` (resolved to a canonical id via `data/officers/id_registry.json`).
//!
//! Usage: `cargo run --bin import_officer_eligibility [path/to/raw-officers.csv]`

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use kobayashi::data::officer::load_canonical_officers;
use kobayashi::data::officer_eligibility::{
    verdict_from_glyph, AbilityEligibility, EligibilityMatrix, EligibilityScenario,
    ScenarioVerdict, DEFAULT_ELIGIBILITY_MATRIX_PATH,
};
use kobayashi::data::registry::merge_registry_entry_with_source;

const DEFAULT_CSV_REL: &str = "data/upstream/cheat-sheet/raw-officers-m90-17rc.csv";

/// Map the cheat-sheet `AbilityType` to our ability `slot` vocabulary.
fn slot_for_ability_type(ability_type: &str) -> Option<&'static str> {
    match ability_type.trim().to_ascii_uppercase().as_str() {
        "CM" => Some("captain"),
        "OA" => Some("officer"),
        "BDA" => Some("below_decks"),
        _ => None,
    }
}

/// Derive a stable `data_version` from the CSV file stem
/// (`raw-officers-m90-17rc` -> `cheat-sheet-m90-17rc`).
fn data_version_from_stem(stem: &str) -> String {
    match stem.strip_prefix("raw-officers-") {
        Some(rest) => format!("cheat-sheet-{rest}"),
        None => format!("cheat-sheet-{stem}"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let repo = Path::new(&manifest_dir);

    let csv_arg = std::env::args().nth(1);
    let csv_path = match &csv_arg {
        Some(p) => repo.join(p),
        None => repo.join(DEFAULT_CSV_REL),
    };
    let output_path = repo.join(DEFAULT_ELIGIBILITY_MATRIX_PATH);
    let id_registry_path = repo.join("data/officers/id_registry.json");
    let canonical_path = repo.join("data/officers/officers.canonical.json");

    let source_note = csv_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("raw-officers.csv")
        .to_string();
    let data_version = data_version_from_stem(
        csv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown"),
    );

    // source_officer_id -> canonical id
    let id_registry: HashMap<String, String> = match fs::read_to_string(&id_registry_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(e) => {
            eprintln!(
                "[import_officer_eligibility] warning: cannot read {}: {e} (canonical ids omitted)",
                id_registry_path.display()
            );
            HashMap::new()
        }
    };

    let csv_content = fs::read_to_string(&csv_path).map_err(|e| {
        format!(
            "Read {}: {e}. Provide the cheat-sheet CSV (default: {DEFAULT_CSV_REL}).",
            csv_path.display()
        )
    })?;
    let mut reader = csv::Reader::from_reader(csv_content.as_bytes());

    // Header name -> column index (robust to reordering and trailing junk columns).
    let headers = reader.headers()?.clone();
    let header_idx: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_string(), i))
        .collect();
    let want = |name: &str| header_idx.get(name).copied();

    let i_ability_id = want("AbilityID").ok_or("CSV missing required column: AbilityID")?;
    let i_ability_type = want("AbilityType").ok_or("CSV missing required column: AbilityType")?;
    let i_officer_id = want("OfficerId");
    let i_conditional_reason = want("Conditional_Reason");

    // (scenario, tag column index, optional reason column index)
    let mut scenario_cols: Vec<(EligibilityScenario, usize, Option<usize>)> = Vec::new();
    for scn in EligibilityScenario::ALL {
        match want(scn.csv_tag_column()) {
            Some(tag_i) => {
                let reason_i = want(&format!("{}_Reason", scn.csv_tag_column()));
                scenario_cols.push((scn, tag_i, reason_i));
            }
            None => eprintln!(
                "[import_officer_eligibility] warning: CSV missing scenario column {}",
                scn.csv_tag_column()
            ),
        }
    }

    let mut abilities: BTreeMap<String, AbilityEligibility> = BTreeMap::new();
    let mut rows = 0usize;
    let mut skipped_blank_id = 0usize;
    let mut skipped_unknown_type = 0usize;
    let mut duplicate_ids = 0usize;
    let mut omitted_cells = 0usize;
    let mut type_counts: HashMap<String, usize> = HashMap::new();

    for result in reader.records() {
        let rec = result?;
        rows += 1;

        let ability_id = rec.get(i_ability_id).unwrap_or("").trim();
        if ability_id.is_empty() {
            skipped_blank_id += 1;
            continue;
        }
        let ability_type_raw = rec.get(i_ability_type).unwrap_or("").trim();
        let Some(slot) = slot_for_ability_type(ability_type_raw) else {
            eprintln!(
                "[import_officer_eligibility] warning: unknown AbilityType '{ability_type_raw}' (ability {ability_id}); skipping row"
            );
            skipped_unknown_type += 1;
            continue;
        };
        if abilities.contains_key(ability_id) {
            eprintln!(
                "[import_officer_eligibility] warning: duplicate AbilityID {ability_id}; keeping first"
            );
            duplicate_ids += 1;
            continue;
        }

        let source_officer_id = i_officer_id
            .and_then(|i| rec.get(i))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let canonical_officer_id = source_officer_id
            .as_deref()
            .and_then(|sid| id_registry.get(sid).cloned());
        let conditional_reason = i_conditional_reason
            .and_then(|i| rec.get(i))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let mut scenarios: BTreeMap<String, ScenarioVerdict> = BTreeMap::new();
        for (scn, tag_i, reason_i) in &scenario_cols {
            let cell = rec.get(*tag_i).unwrap_or("");
            match verdict_from_glyph(cell) {
                Some(verdict) => {
                    let reason = reason_i
                        .and_then(|ri| rec.get(ri))
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    scenarios.insert(
                        scn.as_key().to_string(),
                        ScenarioVerdict { verdict, reason },
                    );
                }
                None => {
                    // Blank/unparseable cell: omit it. A missing scenario is treated as a coverage
                    // gap (falls back to the legacy heuristic) rather than silently excluding.
                    omitted_cells += 1;
                }
            }
        }

        *type_counts
            .entry(ability_type_raw.to_ascii_uppercase())
            .or_default() += 1;

        abilities.insert(
            ability_id.to_string(),
            AbilityEligibility {
                ability_id: ability_id.to_string(),
                source_officer_id,
                canonical_officer_id,
                ability_type: ability_type_raw.to_ascii_uppercase(),
                slot: slot.to_string(),
                conditional_reason,
                scenarios,
            },
        );
    }

    let matrix = EligibilityMatrix {
        data_version: Some(data_version.clone()),
        source: Some("community_spreadsheet".to_string()),
        source_note: Some(source_note),
        imported_at: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
        abilities,
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, serde_json::to_string_pretty(&matrix)?)?;
    println!(
        "Wrote {} abilities to {}",
        matrix.abilities.len(),
        output_path.display()
    );

    merge_registry_entry_with_source(
        repo,
        "officer_eligibility",
        "community_spreadsheet",
        &data_version,
        "officers/eligibility_matrix.json",
    )?;
    println!("Updated data/registry.json (officer_eligibility, {data_version})");

    // --- Coverage report (stderr) ---
    eprintln!("--- coverage ---");
    eprintln!(
        "CSV rows: {rows} (blank-id: {skipped_blank_id}, unknown-type: {skipped_unknown_type}, duplicate-id: {duplicate_ids}, omitted-cells: {omitted_cells})"
    );
    for (t, n) in [("CM", "captain"), ("OA", "officer"), ("BDA", "below_decks")] {
        eprintln!("  {t} ({n}): {}", type_counts.get(t).copied().unwrap_or(0));
    }

    match load_canonical_officers(&canonical_path) {
        Ok(officers) => {
            let csv_ids: HashSet<&str> = matrix.abilities.keys().map(String::as_str).collect();
            let mut catalog_ids: HashSet<String> = HashSet::new();
            let mut catalog_by_slot: HashMap<String, usize> = HashMap::new();
            let mut missing_by_slot: HashMap<String, usize> = HashMap::new();
            for o in &officers {
                for a in &o.abilities {
                    let Some(id) = a.ability_id.as_deref() else {
                        continue;
                    };
                    catalog_ids.insert(id.to_string());
                    *catalog_by_slot.entry(a.slot.clone()).or_default() += 1;
                    if !csv_ids.contains(id) {
                        *missing_by_slot.entry(a.slot.clone()).or_default() += 1;
                    }
                }
            }
            let csv_not_in_catalog = csv_ids
                .iter()
                .filter(|id| !catalog_ids.contains(**id))
                .count();
            eprintln!("Catalog abilities by slot (count / missing-from-CSV):");
            for slot in ["captain", "officer", "below_decks"] {
                eprintln!(
                    "  {slot}: {} / {}",
                    catalog_by_slot.get(slot).copied().unwrap_or(0),
                    missing_by_slot.get(slot).copied().unwrap_or(0),
                );
            }
            eprintln!("CSV ability ids not present in catalog: {csv_not_in_catalog}");
        }
        Err(e) => eprintln!(
            "[import_officer_eligibility] warning: cannot load canonical officers for coverage report: {e}"
        ),
    }

    Ok(())
}
