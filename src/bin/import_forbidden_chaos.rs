//! Import forbidden/chaos tech from CSV (e.g. from community spreadsheet).
//! Reads data/import/forbidden_chaos_tech.csv, writes data/forbidden_chaos_tech.json.
//! CSV columns: name, tech_type, tier, fid, stat, value, operator (header row required).
//! fid is optional (game ID for sync match); leave empty if unknown. Multiple rows with the same name are merged into one record.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct UpstreamForbiddenTechSummaryEntry {
    /// Game ID (sync `fid`).
    id: i64,
    /// Localization id shared with `translations-forbidden_tech.json`.
    loca_id: i64,
    tech_type: u32,
    #[allow(dead_code)]
    tier_max: u32,
}

#[derive(Debug, Deserialize)]
struct UpstreamTranslationEntry {
    id: Option<i64>,
    key: String,
    text: String,
}

fn normalize_tech_name(s: &str) -> String {
    // Normalize enough for stable joins between CSV names and upstream localized text.
    let mut out = String::new();
    let mut prev_was_ws = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !prev_was_ws {
                out.push(' ');
                prev_was_ws = true;
            }
        } else {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_was_ws = false;
        }
    }
    out.trim().to_string()
}

fn forbidden_tech_loca_id_to_fid_maps(
    summary_path: &Path,
) -> Result<
    (
        HashMap<i64, i64>,             // loca_id -> fid
        HashMap<i64, UpstreamForbiddenTechSummaryEntry>, // fid -> entry
    ),
    Box<dyn std::error::Error>,
> {
    let raw = fs::read_to_string(summary_path)?;
    let entries: Vec<UpstreamForbiddenTechSummaryEntry> = serde_json::from_str(&raw)?;

    let mut loca_id_to_fid = HashMap::new();
    let mut by_fid = HashMap::new();
    for e in entries {
        loca_id_to_fid.insert(e.loca_id, e.id);
        by_fid.insert(e.id, e);
    }
    Ok((loca_id_to_fid, by_fid))
}

fn forbidden_tech_translation_name_to_loca_id_map(
    translations_path: &Path,
) -> Result<HashMap<String, i64>, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(translations_path)?;
    let entries: Vec<UpstreamTranslationEntry> = serde_json::from_str(&raw)?;

    // Prefer "forbidden_tech_name" / "forbidden_tech_desc_name" over short uppercase variants.
    let mut best: HashMap<String, (i64, u8)> = HashMap::new(); // normalized_text -> (loca_id, priority)
    for e in entries {
        let Some(loca_id) = e.id else { continue };
        let text_norm = normalize_tech_name(&e.text);
        if text_norm.is_empty() {
            continue;
        }

        let priority = if e.key.ends_with("forbidden_tech_name") {
            0u8
        } else if e.key.ends_with("forbidden_tech_desc_name") {
            1u8
        } else if e.key.ends_with("forbidden_tech_name_short") {
            2u8
        } else {
            3u8
        };

        match best.get(&text_norm) {
            Some((_, existing_priority)) if *existing_priority <= priority => {}
            _ => {
                best.insert(text_norm, (loca_id, priority));
            }
        }
    }

    Ok(best.into_iter().map(|(k, (loca_id, _))| (k, loca_id)).collect())
}

fn expected_catalog_tech_type_from_upstream(
    upstream_tech_type: u32,
) -> Option<&'static str> {
    match upstream_tech_type {
        // Verified from `Ablative Armor` in upstream summary:
        //   summary tech_type=0 => CSV tech_type="forbidden"
        0 => Some("forbidden"),
        1 => Some("chaos"),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let input_path = Path::new(&manifest_dir).join("data/import/forbidden_chaos_tech.csv");
    let output_path = Path::new(&manifest_dir).join("data/forbidden_chaos_tech.json");

    let csv_content = fs::read_to_string(&input_path).map_err(|e| {
        format!(
            "Read {}: {}. Create data/import/ and add forbidden_chaos_tech.csv (columns: name, tech_type, tier, fid, stat, value, operator)",
            input_path.display(),
            e
        )
    })?;

    let mut reader = csv::Reader::from_reader(csv_content.as_bytes());
    let mut by_name: HashMap<String, kobayashi::data::forbidden_chaos::ForbiddenChaosRecord> = HashMap::new();

    for (i, result) in reader.records().enumerate() {
        let record = result?;
        if i == 0 && record.get(0).map(|s| s.eq_ignore_ascii_case("name")).unwrap_or(false) {
            continue;
        }
        let row = CsvRow::from_record(&record)?;
        let name = row.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let bonus = kobayashi::data::forbidden_chaos::BonusEntry {
            stat: row.stat.trim().to_string(),
            value: row.value,
            operator: if row.operator.trim().is_empty() {
            "add".to_string()
        } else {
            row.operator.trim().to_string()
        },
        };
        let record = by_name.entry(name.clone()).or_insert_with(|| {
            kobayashi::data::forbidden_chaos::ForbiddenChaosRecord {
                fid: row.fid,
                name: name.clone(),
                tech_type: row.tech_type.trim().to_string(),
                tier: row.tier.and_then(|t| if t > 0 { Some(t) } else { None }),
                bonuses: Vec::new(),
            }
        });
        if record.fid.is_none() && row.fid.is_some() {
            record.fid = row.fid;
        }
        record.bonuses.push(bonus);
    }

    // Fill missing `fid` by joining:
    //   CSV catalog `name` -> upstream `translations-forbidden_tech.json` (text -> loca_id)
    //   upstream `loca_id` -> upstream `summary-forbidden_tech.json` (loca_id -> id/fid)
    //
    // This makes the catalog usable for sync-based merging (`profiles/*/forbidden_tech.imported.json`)
    // without forcing the CSV to manually maintain `fid` values.
    let upstream_summary_path =
        Path::new(&manifest_dir).join("data/upstream/data-stfc-space/summary-forbidden_tech.json");
    let upstream_translations_path =
        Path::new(&manifest_dir).join("data/upstream/data-stfc-space/translations-forbidden_tech.json");
    if upstream_summary_path.is_file() && upstream_translations_path.is_file() {
        let (loca_id_to_fid, upstream_by_fid) = match forbidden_tech_loca_id_to_fid_maps(&upstream_summary_path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[import_forbidden_chaos] warning: failed loading upstream summary: {e}"
                );
                (HashMap::new(), HashMap::new())
            }
        };
        let name_to_loca_id = match forbidden_tech_translation_name_to_loca_id_map(&upstream_translations_path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[import_forbidden_chaos] warning: failed loading upstream translations: {e}"
                );
                HashMap::new()
            }
        };

        for record in by_name.values_mut() {
            if record.fid.is_some() {
                continue;
            }
            let name_norm = normalize_tech_name(&record.name);
            let Some(loca_id) = name_to_loca_id.get(&name_norm) else {
                continue;
            };
            let Some(fid) = loca_id_to_fid.get(loca_id) else {
                continue;
            };
            record.fid = Some(*fid);

            if let Some(upstream_entry) = upstream_by_fid.get(fid) {
                if let Some(expected) =
                    expected_catalog_tech_type_from_upstream(upstream_entry.tech_type)
                {
                    let actual = record.tech_type.to_ascii_lowercase();
                    if !actual.is_empty() && actual != expected {
                        eprintln!(
                            "[import_forbidden_chaos] warning: tech_type mismatch for '{}' (catalog='{}', upstream='{}', fid={})",
                            record.name,
                            actual,
                            expected,
                            fid
                        );
                    }
                }
            }
        }
    }

    let items: Vec<_> = by_name.into_values().collect();
    let list = kobayashi::data::forbidden_chaos::ForbiddenChaosList {
        source: Some("community_spreadsheet".to_string()),
        last_updated: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
        items,
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, serde_json::to_string_pretty(&list)?)?;
    println!("Wrote {} items to {}", list.items.len(), output_path.display());
    Ok(())
}

struct CsvRow {
    name: String,
    tech_type: String,
    tier: Option<u32>,
    fid: Option<i64>,
    stat: String,
    value: f64,
    operator: String,
}

impl CsvRow {
    fn from_record(record: &csv::StringRecord) -> Result<Self, Box<dyn std::error::Error>> {
        if record.len() < 5 {
            return Err("CSV row needs at least 5 columns: name, tech_type, tier, [fid], stat, value, operator".into());
        }
        let name = record.get(0).unwrap_or("").to_string();
        let tech_type = record.get(1).unwrap_or("").to_string();
        let tier = record.get(2).and_then(|s| s.trim().parse::<u32>().ok());
        if record.len() >= 7 {
            let fid_col = record.get(3).unwrap_or("").trim();
            let fid = if fid_col.is_empty() {
                None
            } else {
                fid_col.parse::<i64>().ok()
            };
            return Ok(CsvRow {
                name,
                tech_type,
                tier,
                fid,
                stat: record.get(4).unwrap_or("").to_string(),
                value: record.get(5).unwrap_or("0").trim().parse().unwrap_or(0.0),
                operator: record.get(6).unwrap_or("add").to_string(),
            });
        }
        Ok(CsvRow {
            name,
            tech_type,
            tier,
            fid: None,
            stat: record.get(3).unwrap_or("").to_string(),
            value: record.get(4).unwrap_or("0").trim().parse().unwrap_or(0.0),
            operator: record.get(5).unwrap_or("add").to_string(),
        })
    }
}

