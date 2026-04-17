//! Maintainer report: canonical officer `conditions` tokens without LCARS mapping, and
//! hostile `upstream_ship_type` values from `data/hostiles/index.json`.
//!
//! Run: `cargo run --bin report_unknown_mappings`
//!   [--canonical path/to/officers.canonical.json] [--hostile-index path/to/index.json] [--output path.md]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use kobayashi::data::upstream_hostile_ship_type::{
    upstream_hostile_ship_type_profile, upstream_ship_type_is_explicitly_mapped,
};
use kobayashi::lcars::is_canonical_condition_mapped;
use serde::Deserialize;

const DEFAULT_CANONICAL: &str = "data/officers/officers.canonical.json";
const DEFAULT_HOSTILE_INDEX: &str = "data/hostiles/index.json";

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

struct Args {
    canonical: PathBuf,
    hostile_index: PathBuf,
    output: Option<PathBuf>,
}

fn parse_args(base: &Path, args: &[String]) -> Args {
    let mut canonical = base.join(DEFAULT_CANONICAL);
    let mut hostile_index = base.join(DEFAULT_HOSTILE_INDEX);
    let mut output: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--canonical" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    canonical = PathBuf::from(p);
                }
            }
            "--hostile-index" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    hostile_index = PathBuf::from(p);
                }
            }
            "--output" | "-o" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    output = Some(PathBuf::from(p));
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: report_unknown_mappings [--canonical PATH] [--hostile-index PATH] [--output PATH]"
                );
                eprintln!("Defaults (relative to crate root): {DEFAULT_CANONICAL}, {DEFAULT_HOSTILE_INDEX}");
                eprintln!("With no --output, Markdown is written to stdout.");
                std::process::exit(0);
            }
            other => {
                eprintln!("report_unknown_mappings: unknown argument {other:?} (try --help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    Args {
        canonical,
        hostile_index,
        output,
    }
}

fn md_escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[derive(Default)]
struct TokenAgg {
    count: usize,
    examples: Vec<String>,
}

fn scan_canonical(path: &Path) -> Result<HashMap<String, TokenAgg>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let file: CanonicalFile = serde_json::from_str(&text)?;
    let mut map: HashMap<String, TokenAgg> = HashMap::new();

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

#[derive(Default, Clone)]
struct ShipTypeAgg {
    count: usize,
    sample_ids: Vec<String>,
}

fn scan_hostile_index(path: &Path) -> Result<BTreeMap<u32, ShipTypeAgg>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let idx: HostileIndex = serde_json::from_str(&text)?;
    let mut map: BTreeMap<u32, ShipTypeAgg> = BTreeMap::new();

    for row in &idx.hostiles {
        let e = map.entry(row.upstream_ship_type).or_default();
        e.count += 1;
        if e.sample_ids.len() < 4 {
            e.sample_ids.push(row.id.clone());
        }
    }

    Ok(map)
}

fn render_report(
    canonical_path: &Path,
    hostile_path: &Path,
    token_map: &HashMap<String, TokenAgg>,
    ship_map: &BTreeMap<u32, ShipTypeAgg>,
) -> String {
    let mut distinct = 0usize;
    let mut mapped_tokens = 0usize;
    let mut unmapped_rows: Vec<(String, usize, Vec<String>)> = Vec::new();

    for (tok, agg) in token_map {
        distinct += 1;
        if is_canonical_condition_mapped(tok) {
            mapped_tokens += 1;
        } else {
            unmapped_rows.push((tok.clone(), agg.count, agg.examples.clone()));
        }
    }

    unmapped_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut out = String::new();
    out.push_str("# Unknown mappings report\n\n");
    out.push_str("## Inputs\n\n");
    out.push_str(&format!(
        "- Canonical officers: `{}`\n- Hostile index: `{}`\n\n",
        canonical_path.display(),
        hostile_path.display()
    ));

    out.push_str("## Canonical condition tokens\n\n");
    out.push_str(&format!(
        "- Distinct non-empty tokens: **{}**\n- Mapped to LCARS: **{}**\n- Unmapped: **{}**\n\n",
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
            if profile.is_armada_target { "yes" } else { "no" },
            md_escape_cell(profile.note),
            ids
        ));
    }

    out.push('\n');
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = Path::new(&manifest_dir);
    let args: Vec<String> = std::env::args().collect();
    let cfg = parse_args(base, &args);

    let token_map = scan_canonical(&cfg.canonical)?;
    let ship_map = scan_hostile_index(&cfg.hostile_index)?;
    let md = render_report(&cfg.canonical, &cfg.hostile_index, &token_map, &ship_map);

    if let Some(out_path) = cfg.output {
        let mut f = fs::File::create(&out_path)?;
        f.write_all(md.as_bytes())?;
        eprintln!("Wrote {}", out_path.display());
    } else {
        print!("{md}");
    }

    Ok(())
}
