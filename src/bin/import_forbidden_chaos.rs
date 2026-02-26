//! Import forbidden/chaos tech from CSV (e.g. from community spreadsheet).
//! Reads data/import/forbidden_chaos_tech.csv, writes data/forbidden_chaos_tech.json.
//! CSV columns: name, tech_type, tier, stat, value, operator (header row required).
//! Multiple rows with the same name are merged into one record with multiple bonuses.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let input_path = Path::new(&manifest_dir).join("data/import/forbidden_chaos_tech.csv");
    let output_path = Path::new(&manifest_dir).join("data/forbidden_chaos_tech.json");

    let csv_content = fs::read_to_string(&input_path).map_err(|e| {
        format!(
            "Read {}: {}. Create data/import/ and add forbidden_chaos_tech.csv (columns: name, tech_type, tier, stat, value, operator)",
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
                fid: None,
                name: name.clone(),
                tech_type: row.tech_type.trim().to_string(),
                tier: row.tier.and_then(|t| if t > 0 { Some(t) } else { None }),
                bonuses: Vec::new(),
            }
        });
        record.bonuses.push(bonus);
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
    stat: String,
    value: f64,
    operator: String,
}

impl CsvRow {
    fn from_record(record: &csv::StringRecord) -> Result<Self, Box<dyn std::error::Error>> {
        if record.len() < 5 {
            return Err("CSV row needs at least 5 columns: name, tech_type, tier, stat, value, operator".into());
        }
        let name = record.get(0).unwrap_or("").to_string();
        let tech_type = record.get(1).unwrap_or("").to_string();
        let tier = record.get(2).and_then(|s| s.trim().parse::<u32>().ok());
        let stat = record.get(3).unwrap_or("").to_string();
        let value = record.get(4).unwrap_or("0").trim().parse().unwrap_or(0.0);
        let operator = record.get(5).unwrap_or("add").to_string();
        Ok(CsvRow {
            name,
            tech_type,
            tier,
            stat,
            value,
            operator,
        })
    }
}

