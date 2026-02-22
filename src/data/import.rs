use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const DEFAULT_ALIAS_MAP_PATH: &str = "data/officers/name_aliases.json";
const DEFAULT_CANONICAL_OFFICERS_PATH: &str = "data/officers/officers.canonical.json";
pub const DEFAULT_IMPORT_OUTPUT_PATH: &str = "data/officers/roster.imported.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub canonical_officer_id: String,
    pub canonical_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnresolvedEntry {
    pub record_index: usize,
    pub input_name: String,
    pub normalized_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateEntry {
    pub canonical_officer_id: String,
    pub record_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConflictEntry {
    pub canonical_officer_id: String,
    pub first_record_index: usize,
    pub conflicting_record_index: usize,
    pub first_state: RosterEntry,
    pub conflicting_state: RosterEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub source_path: String,
    pub output_path: String,
    pub total_records: usize,
    pub matched_records: usize,
    pub unmatched_records: usize,
    pub ambiguous_records: usize,
    pub duplicate_records: usize,
    pub conflict_records: usize,
    pub critical_failures: usize,
    pub roster_entries_written: usize,
    pub unresolved: Vec<UnresolvedEntry>,
    pub duplicates: Vec<DuplicateEntry>,
    pub conflicts: Vec<ConflictEntry>,
}

impl ImportReport {
    pub fn has_critical_failures(&self) -> bool {
        self.critical_failures > 0
    }
}

#[derive(Debug)]
pub enum ImportError {
    Read(std::io::Error),
    Parse(serde_json::Error),
    Write(std::io::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read import file: {err}"),
            Self::Parse(err) => write!(f, "failed to parse import JSON: {err}"),
            Self::Write(err) => write!(f, "failed to persist import output: {err}"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficersFile {
    officers: Vec<CanonicalOfficer>,
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficer {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SpocksExport {
    Records(Vec<SpocksOfficerRecord>),
    Officers { officers: Vec<SpocksOfficerRecord> },
    Data { data: SpocksData },
    Profile { profile: SpocksProfile },
}

#[derive(Debug, Deserialize)]
struct SpocksData {
    officers: Vec<SpocksOfficerRecord>,
}

#[derive(Debug, Deserialize)]
struct SpocksProfile {
    officers: Vec<SpocksOfficerRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct SpocksOfficerRecord {
    #[serde(
        default,
        alias = "officerId",
        alias = "officer_id",
        alias = "template_id"
    )]
    id: Option<String>,
    #[serde(
        default,
        alias = "officerName",
        alias = "officer_name",
        alias = "title"
    )]
    name: Option<String>,
    #[serde(default)]
    officer: Option<SpocksOfficerRef>,
    #[serde(default, alias = "officerRank")]
    rank: Option<u8>,
    #[serde(default, alias = "officerTier")]
    tier: Option<u8>,
    #[serde(default, alias = "officerLevel")]
    level: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
struct SpocksOfficerRef {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

pub fn import_spocks_export(path: &str) -> Result<ImportReport, ImportError> {
    let raw = fs::read_to_string(path).map_err(ImportError::Read)?;
    let export: SpocksExport = serde_json::from_str(&raw).map_err(ImportError::Parse)?;
    let records = flatten_export(export);

    let alias_map = load_alias_map(DEFAULT_ALIAS_MAP_PATH)?;
    let canonical_by_name = load_canonical_index(DEFAULT_CANONICAL_OFFICERS_PATH)?;

    let mut resolved_by_id: HashMap<String, (usize, RosterEntry)> = HashMap::new();
    let mut duplicate_indices: HashMap<String, Vec<usize>> = HashMap::new();
    let mut conflicts = Vec::new();
    let mut unresolved = Vec::new();
    let mut matched_records = 0usize;
    let mut ambiguous_records = 0usize;

    for (index, record) in records.iter().enumerate() {
        let raw_name = record
            .name
            .as_ref()
            .or(record.id.as_ref())
            .or(record
                .officer
                .as_ref()
                .and_then(|officer| officer.name.as_ref()))
            .or(record
                .officer
                .as_ref()
                .and_then(|officer| officer.id.as_ref()))
            .map(|value| value.trim())
            .unwrap_or("");

        let normalized_input = normalize_key(raw_name);
        let canonical_name = alias_map
            .get(&normalized_input)
            .cloned()
            .unwrap_or_else(|| raw_name.trim().to_string());
        let normalized_name = normalize_key(&canonical_name);

        let Some(candidates) = canonical_by_name.get(&normalized_name) else {
            unresolved.push(UnresolvedEntry {
                record_index: index,
                input_name: raw_name.to_string(),
                normalized_name,
                reason: "no canonical officer match".to_string(),
            });
            continue;
        };

        if candidates.len() > 1 {
            ambiguous_records += 1;
            unresolved.push(UnresolvedEntry {
                record_index: index,
                input_name: raw_name.to_string(),
                normalized_name,
                reason: format!("ambiguous canonical mapping ({} matches)", candidates.len()),
            });
            continue;
        }

        let candidate = &candidates[0];
        matched_records += 1;

        duplicate_indices
            .entry(candidate.id.clone())
            .or_default()
            .push(index);

        let entry = RosterEntry {
            canonical_officer_id: candidate.id.clone(),
            canonical_name: candidate.name.clone(),
            rank: record.rank,
            tier: record.tier,
            level: record.level,
        };

        if let Some((first_index, first_entry)) = resolved_by_id.get(&candidate.id) {
            if first_entry != &entry {
                conflicts.push(ConflictEntry {
                    canonical_officer_id: candidate.id.clone(),
                    first_record_index: *first_index,
                    conflicting_record_index: index,
                    first_state: first_entry.clone(),
                    conflicting_state: entry,
                });
            }
            continue;
        }

        resolved_by_id.insert(candidate.id.clone(), (index, entry));
    }

    let mut duplicates = Vec::new();
    for (canonical_id, indices) in duplicate_indices {
        if indices.len() > 1 {
            duplicates.push(DuplicateEntry {
                canonical_officer_id: canonical_id,
                record_indices: indices,
            });
        }
    }

    duplicates.sort_by(|a, b| a.canonical_officer_id.cmp(&b.canonical_officer_id));

    let mut roster: Vec<RosterEntry> = resolved_by_id
        .into_values()
        .map(|(_, entry)| entry)
        .collect();
    roster.sort_by(|a, b| a.canonical_officer_id.cmp(&b.canonical_officer_id));

    let output_payload = serde_json::json!({
        "source_path": path,
        "officers": roster,
    });

    if let Some(parent) = Path::new(DEFAULT_IMPORT_OUTPUT_PATH).parent() {
        fs::create_dir_all(parent).map_err(ImportError::Write)?;
    }
    let serialized = serde_json::to_string_pretty(&output_payload).map_err(ImportError::Parse)?;
    fs::write(DEFAULT_IMPORT_OUTPUT_PATH, serialized).map_err(ImportError::Write)?;

    let unresolved_count = unresolved.len();
    let conflict_count = conflicts.len();
    let critical_failures = unresolved_count + conflict_count;

    Ok(ImportReport {
        source_path: path.to_string(),
        output_path: DEFAULT_IMPORT_OUTPUT_PATH.to_string(),
        total_records: records.len(),
        matched_records,
        unmatched_records: unresolved_count.saturating_sub(ambiguous_records),
        ambiguous_records,
        duplicate_records: duplicates.len(),
        conflict_records: conflict_count,
        critical_failures,
        roster_entries_written: output_payload["officers"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0),
        unresolved,
        duplicates,
        conflicts,
    })
}

fn flatten_export(export: SpocksExport) -> Vec<SpocksOfficerRecord> {
    match export {
        SpocksExport::Records(records) => records,
        SpocksExport::Officers { officers } => officers,
        SpocksExport::Data { data } => data.officers,
        SpocksExport::Profile { profile } => profile.officers,
    }
}

fn load_alias_map(path: &str) -> Result<HashMap<String, String>, ImportError> {
    let raw = fs::read_to_string(path).map_err(ImportError::Read)?;
    let parsed: HashMap<String, String> = serde_json::from_str(&raw).map_err(ImportError::Parse)?;
    let aliases = parsed
        .into_iter()
        .map(|(alias, canonical)| (normalize_key(&alias), canonical.trim().to_string()))
        .collect();
    Ok(aliases)
}

fn load_canonical_index(path: &str) -> Result<HashMap<String, Vec<CanonicalOfficer>>, ImportError> {
    let raw = fs::read_to_string(path).map_err(ImportError::Read)?;
    let payload: CanonicalOfficersFile = serde_json::from_str(&raw).map_err(ImportError::Parse)?;

    let mut index: HashMap<String, Vec<CanonicalOfficer>> = HashMap::new();
    let mut seen = HashSet::new();

    for officer in payload.officers {
        if !seen.insert(officer.id.clone()) {
            continue;
        }
        index
            .entry(normalize_key(&officer.name))
            .or_default()
            .push(officer);
    }

    Ok(index)
}

fn normalize_key(value: &str) -> String {
    value
        .trim()
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Loads the set of canonical officer IDs from the imported roster file.
/// Returns `None` if the file is missing or invalid (caller should then use the full canonical list).
/// Returns `Some(ids)` to filter crew generation to only officers the player owns.
pub fn load_imported_roster_ids(path: &str) -> Option<HashSet<String>> {
    #[derive(Debug, Deserialize)]
    struct ImportedRosterPayload {
        officers: Vec<RosterEntry>,
    }
    let raw = fs::read_to_string(path).ok()?;
    let payload: ImportedRosterPayload = serde_json::from_str(&raw).ok()?;
    let ids = payload
        .officers
        .into_iter()
        .map(|e| e.canonical_officer_id)
        .collect();
    Some(ids)
}
