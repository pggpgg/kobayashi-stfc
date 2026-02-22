//! Import Syndicate reputation from CSV or .xlsx (e.g. Syndicate Progression spreadsheet).
//! Reads data/import/syndicate_reputation.csv or a given .xlsx path; writes data/syndicate_reputation.json.
//! CSV columns: level, stat, value, operator. For .xlsx, uses "Level By Level Comparison" sheet (wide or long format).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use calamine::Reader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let data_root = Path::new(&manifest_dir).join("data");
    let output_path = data_root.join("syndicate_reputation.json");
    let registry_path = data_root.join("registry.json");

    let by_level: HashMap<u32, Vec<kobayashi::data::syndicate_reputation::SyndicateBonusEntry>> =
        if let Some(path_arg) = std::env::args().nth(1) {
            let path = Path::new(&path_arg);
            if path.extension().map(|e| e == "xlsx").unwrap_or(false) {
                read_from_xlsx(path)?
            } else {
                return Err(format!(
                    "Expected .xlsx path or no argument (to use CSV). Got: {}",
                    path_arg
                )
                .into());
            }
        } else {
            let csv_path = data_root.join("import/syndicate_reputation.csv");
            read_from_csv(&csv_path)?
        };

    let mut levels: Vec<_> = by_level
        .into_iter()
        .map(|(level, bonuses)| kobayashi::data::syndicate_reputation::SyndicateLevelEntry {
            level,
            bonuses,
        })
        .collect();
    levels.sort_by_key(|e| e.level);

    let list = kobayashi::data::syndicate_reputation::SyndicateReputationList {
        source: Some("community_spreadsheet".to_string()),
        last_updated: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
        levels,
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, serde_json::to_string_pretty(&list)?)?;
    println!(
        "Wrote {} levels to {}",
        list.levels.len(),
        output_path.display()
    );

    let mut registry: kobayashi::data::registry::Registry = if registry_path.exists() {
        serde_json::from_str(&fs::read_to_string(&registry_path)?)?
    } else {
        std::collections::HashMap::new()
    };
    registry.insert(
        "syndicate_reputation".to_string(),
        kobayashi::data::registry::DataSetEntry {
            source: "community_spreadsheet".to_string(),
            data_version: None,
            last_updated: list.last_updated.clone(),
            path: "syndicate_reputation.json".to_string(),
        },
    );
    fs::write(&registry_path, serde_json::to_string_pretty(&registry)?)?;
    println!("Updated {}", registry_path.display());

    Ok(())
}

fn read_from_csv(
    input_path: &Path,
) -> Result<
    HashMap<u32, Vec<kobayashi::data::syndicate_reputation::SyndicateBonusEntry>>,
    Box<dyn std::error::Error>,
> {
    let csv_content = fs::read_to_string(input_path).map_err(|e| {
        format!(
            "Read {}: {}. Use CSV (level, stat, value, operator) or pass .xlsx path.",
            input_path.display(),
            e
        )
    })?;
    let mut reader = csv::Reader::from_reader(csv_content.as_bytes());
    let mut by_level: HashMap<
        u32,
        Vec<kobayashi::data::syndicate_reputation::SyndicateBonusEntry>,
    > = HashMap::new();

    for (i, result) in reader.records().enumerate() {
        let record = result?;
        if i == 0 && record.get(0).map(|s| s.eq_ignore_ascii_case("level")).unwrap_or(false) {
            continue;
        }
        let row = CsvRow::from_record(&record)?;
        let bonus = kobayashi::data::syndicate_reputation::SyndicateBonusEntry {
            stat: row.stat.trim().to_string(),
            value: row.value,
            operator: if row.operator.trim().is_empty() {
                "add".to_string()
            } else {
                row.operator.trim().to_string()
            },
        };
        by_level.entry(row.level).or_default().push(bonus);
    }
    Ok(by_level)
}

fn read_from_xlsx(
    path: &Path,
) -> Result<
    HashMap<u32, Vec<kobayashi::data::syndicate_reputation::SyndicateBonusEntry>>,
    Box<dyn std::error::Error>,
> {
    let mut wb = calamine::open_workbook_auto(path)?;
    let names = wb.sheet_names();
    let sheet_name = names
        .iter()
        .find(|s: &&String| s.contains("Level By Level") || s.contains("Comparison"))
        .or(names.first())
        .ok_or("No sheets in workbook")?;
    let range = wb.worksheet_range(sheet_name)?;
    let mut by_level: HashMap<
        u32,
        Vec<kobayashi::data::syndicate_reputation::SyndicateBonusEntry>,
    > = HashMap::new();

    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    if rows.is_empty() {
        return Ok(by_level);
    }
    let ncols = rows[0].len();

    // Detect format: long = few columns with level, stat, value
    let header = rows[0];
    let looks_long = ncols >= 3
        && ncols <= 5
        && (cell_trim(header.get(0)).eq_ignore_ascii_case("level")
            || cell_trim(header.get(1)).eq_ignore_ascii_case("stat"));

    if looks_long {
        for row in rows.iter().skip(1) {
            let lvl = cell_to_u32(row.get(0));
            let stat = cell_trim(row.get(1)).to_string();
            let val = cell_to_f64(row.get(2));
            if lvl == 0 && stat.is_empty() {
                continue;
            }
            if stat.is_empty() {
                continue;
            }
            let op = cell_trim(row.get(3));
            let operator = if op.is_empty() { "add" } else { op }.to_string();
            by_level.entry(lvl).or_default().push(
                kobayashi::data::syndicate_reputation::SyndicateBonusEntry {
                    stat,
                    value: val,
                    operator,
                },
            );
        }
    } else {
        // Wide: e.g. Syndicate Progression "Level By Level Comparison" - multi-row header, then data
        // Find first data row (first cell is numeric level >= 1)
        let data_start = rows
            .iter()
            .position(|row| cell_to_u32(row.get(0)) >= 1)
            .unwrap_or(rows.len());
        if data_start >= rows.len() {
            return Ok(by_level);
        }
        // Build stat name per column from up to 3 header rows with carry-forward (merged cells).
        // For each row r, use the last non-empty cell in row r from column 0..=j so merged
        // regions (e.g. "Officer Stats" spanning many cols, then "Officer Attack", then "51-60")
        // produce unique labels per column: "Officer_Stats_>_Officer_Attack_>_51-60".
        let stat_names: Vec<String> = (0..ncols)
            .map(|j| {
                let parts: Vec<String> = (0..data_start.min(3))
                    .filter_map(|r| {
                        let row = rows.get(r)?;
                        let mut last = String::new();
                        for c in 0..=j.min(row.len().saturating_sub(1)) {
                            if let Some(cell) = row.get(c) {
                                let s = cell_trim(Some(cell)).to_string();
                                if !s.is_empty() {
                                    last = s;
                                }
                            }
                        }
                        if last.is_empty() {
                            None
                        } else {
                            Some(last)
                        }
                    })
                    .collect();
                let joined = parts.join(" > ").trim().to_string();
                if joined.is_empty() {
                    format!("col_{}", j)
                } else {
                    // Normalize: spaces -> underscores for machine-friendly keys
                    joined.replace(' ', "_").replace('\n', " ").trim().to_string()
                }
            })
            .collect();
        for row in rows.iter().skip(data_start) {
            let lvl = cell_to_u32(row.get(0));
            if lvl == 0 {
                continue;
            }
            for (j, cell) in row.iter().enumerate().skip(1) {
                if j >= stat_names.len() {
                    break;
                }
                if cell_is_empty(cell) {
                    continue;
                }
                let val = cell_to_f64(Some(cell));
                let stat = stat_names[j].clone();
                if stat.is_empty() {
                    continue;
                }
                by_level.entry(lvl).or_default().push(
                    kobayashi::data::syndicate_reputation::SyndicateBonusEntry {
                        stat,
                        value: val,
                        operator: "add".to_string(),
                    },
                );
            }
        }
    }

    Ok(by_level)
}

fn cell_trim(d: Option<&calamine::Data>) -> &str {
    match d {
        Some(calamine::Data::String(s)) => s.trim(),
        _ => "",
    }
}

fn cell_to_u32(d: Option<&calamine::Data>) -> u32 {
    match d {
        Some(calamine::Data::Int(i)) => (*i).max(0) as u32,
        Some(calamine::Data::Float(f)) => (*f).max(0.0) as u32,
        Some(calamine::Data::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn cell_to_f64(d: Option<&calamine::Data>) -> f64 {
    match d {
        Some(calamine::Data::Float(f)) => *f,
        Some(calamine::Data::Int(i)) => *i as f64,
        Some(calamine::Data::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn cell_is_empty(d: &calamine::Data) -> bool {
    match d {
        calamine::Data::Empty => true,
        calamine::Data::String(s) => s.trim().is_empty(),
        _ => false,
    }
}

struct CsvRow {
    level: u32,
    stat: String,
    value: f64,
    operator: String,
}

impl CsvRow {
    fn from_record(record: &csv::StringRecord) -> Result<Self, Box<dyn std::error::Error>> {
        if record.len() < 3 {
            return Err("CSV row needs at least 3 columns: level, stat, value (operator optional)".into());
        }
        let level = record.get(0).unwrap_or("0").trim().parse().unwrap_or(0);
        let stat = record.get(1).unwrap_or("").to_string();
        let value = record.get(2).unwrap_or("0").trim().parse().unwrap_or(0.0);
        let operator = record.get(3).unwrap_or("add").to_string();
        Ok(CsvRow {
            level,
            stat,
            value,
            operator,
        })
    }
}
