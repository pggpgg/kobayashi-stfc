use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};

const DEFAULT_ALIAS_MAP_PATH: &str = "data/officers/name_aliases.json";
const DEFAULT_CANONICAL_OFFICERS_PATH: &str = "data/officers/officers.canonical.json";
pub const DEFAULT_IMPORT_OUTPUT_PATH: &str = "rosters/roster.imported.json";

/// Path for synced research state (stfc-mod sync). Load with [load_imported_research].
pub const DEFAULT_RESEARCH_IMPORT_PATH: &str = "rosters/research.imported.json";
/// Path for synced buildings state (stfc-mod sync). Load with [load_imported_buildings].
pub const DEFAULT_BUILDINGS_IMPORT_PATH: &str = "rosters/buildings.imported.json";
/// Path for synced ships state (stfc-mod sync). Load with [load_imported_ships].
pub const DEFAULT_SHIPS_IMPORT_PATH: &str = "rosters/ships.imported.json";

// ----- Synced game state (research, buildings, ships) from stfc-mod sync -----

/// One research project level from imported/synced state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEntry {
    pub rid: i64,
    pub level: i64,
}

/// One building level from imported/synced state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingEntry {
    pub bid: i64,
    pub level: i64,
}

/// One player ship from imported/synced state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipEntry {
    pub psid: i64,
    pub tier: i64,
    pub level: i64,
    #[serde(default)]
    pub level_percentage: f64,
    pub hull_id: i64,
    #[serde(default)]
    pub components: Vec<i64>,
}

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

/// Max officer tier (e.g. 3 in STFC). Used when only name is given.
const MAX_OFFICER_TIER: u8 = 3;

/// Max level for a given tier (tier 1 -> 10, tier 2 -> 20, tier 3 -> 30). Used when tier is given but level is not.
fn max_level_for_tier(tier: u8) -> u16 {
    match tier {
        1 => 10,
        2 => 20,
        _ => 30,
    }
}

#[derive(Debug)]
pub enum ImportError {
    Read(std::io::Error),
    Parse(serde_json::Error),
    ParseLine { line: usize, message: String },
    Write(std::io::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read import file: {err}"),
            Self::Parse(err) => write!(f, "failed to parse import JSON: {err}"),
            Self::ParseLine { line, message } => write!(f, "line {line}: {message}"),
            Self::Write(err) => write!(f, "failed to persist import output: {err}"),
        }
    }
}

/// Parses a tier cell: "T3", "Tier 2", "2", etc. Returns None if empty or invalid.
fn parse_tier_cell(s: &str) -> Option<u8> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let s = s
        .strip_prefix("tier")
        .or_else(|| s.strip_prefix("Tier"))
        .or_else(|| s.strip_prefix("tier "))
        .or_else(|| s.strip_prefix("Tier "))
        .or_else(|| s.strip_prefix("T"))
        .or_else(|| s.strip_prefix("t"))
        .or_else(|| s.strip_prefix("T "))
        .or_else(|| s.strip_prefix("t "))
        .unwrap_or(s);
    s.trim().parse::<u8>().ok().filter(|&t| t >= 1)
}

/// Parses a level cell: "lvl 20", "LVL20", "20", etc. Returns None if empty or invalid.
fn parse_level_cell(s: &str) -> Option<u16> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let s = s
        .strip_prefix("lvl")
        .or_else(|| s.strip_prefix("LVL"))
        .or_else(|| s.strip_prefix("Lvl"))
        .or_else(|| s.strip_prefix("lvl "))
        .or_else(|| s.strip_prefix("LVL "))
        .or_else(|| s.strip_prefix("Lvl "))
        .unwrap_or(s);
    s.trim().parse::<u16>().ok()
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

/// Raw record before name resolution: (raw_name, rank, tier, level).
type RawRosterRecord = (String, Option<u8>, Option<u8>, Option<u16>);

fn resolve_and_write_roster(
    path: &str,
    raw_records: &[RawRosterRecord],
) -> Result<ImportReport, ImportError> {
    let alias_map = load_alias_map(DEFAULT_ALIAS_MAP_PATH)?;
    let canonical_by_name = load_canonical_index(DEFAULT_CANONICAL_OFFICERS_PATH)?;

    let mut resolved_by_id: HashMap<String, (usize, RosterEntry)> = HashMap::new();
    let mut duplicate_indices: HashMap<String, Vec<usize>> = HashMap::new();
    let mut conflicts = Vec::new();
    let mut unresolved = Vec::new();
    let mut matched_records = 0usize;
    let mut ambiguous_records = 0usize;

    for (index, (raw_name, rank, tier, level)) in raw_records.iter().enumerate() {
        let raw_name = raw_name.trim();
        if raw_name.is_empty() {
            continue;
        }

        let normalized_input = normalize_key(raw_name);
        let canonical_name = alias_map
            .get(&normalized_input)
            .cloned()
            .unwrap_or_else(|| raw_name.to_string());
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
            rank: *rank,
            tier: *tier,
            level: *level,
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

    let roster_len = roster.len();
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
        total_records: raw_records.len(),
        matched_records,
        unmatched_records: unresolved_count.saturating_sub(ambiguous_records),
        ambiguous_records,
        duplicate_records: duplicates.len(),
        conflict_records: conflict_count,
        critical_failures,
        roster_entries_written: roster_len,
        unresolved,
        duplicates,
        conflicts,
    })
}

pub fn import_spocks_export(path: &str) -> Result<ImportReport, ImportError> {
    let raw = fs::read_to_string(path).map_err(ImportError::Read)?;
    let export: SpocksExport = serde_json::from_str(&raw).map_err(ImportError::Parse)?;
    let records = flatten_export(export);

    let raw_records: Vec<RawRosterRecord> = records
        .iter()
        .map(|r| {
            let raw_name = r
                .name
                .as_ref()
                .or(r.id.as_ref())
                .or(r.officer.as_ref().and_then(|o| o.name.as_ref()))
                .or(r.officer.as_ref().and_then(|o| o.id.as_ref()))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            (raw_name, r.rank, r.tier, r.level)
        })
        .collect();

    resolve_and_write_roster(path, &raw_records)
}

/// Imports a roster from a comma-separated .txt file (name,tier,level per line).
/// Uses the csv crate so names containing a comma can be quoted (e.g. `"Kirk, James",3,45`).
/// Skips optional header "name,tier,level". Applies failsafe defaults: name only -> max tier+level; name+tier only -> max level for that tier.
pub fn import_roster_csv(path: &str) -> Result<ImportReport, ImportError> {
    let content = fs::read_to_string(path).map_err(ImportError::Read)?;
    let mut raw_records: Vec<RawRosterRecord> = Vec::new();
    let mut skip_next_if_header = true;

    let mut rdr = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .has_headers(false)
        .from_reader(Cursor::new(content));

    for (record_index, record) in rdr.records().enumerate() {
        let line_num = record_index + 1;
        let record = record.map_err(|e| ImportError::ParseLine {
            line: line_num,
            message: e.to_string(),
        })?;

        let name = record.get(0).unwrap_or("").trim();
        if name.is_empty() {
            let has_other = !record.get(1).unwrap_or("").trim().is_empty()
                || !record.get(2).unwrap_or("").trim().is_empty();
            if has_other {
                return Err(ImportError::ParseLine {
                    line: line_num,
                    message: "missing officer name".to_string(),
                });
            }
            continue;
        }
        if skip_next_if_header && name.eq_ignore_ascii_case("name") {
            skip_next_if_header = false;
            continue;
        }
        skip_next_if_header = false;

        let mut tier = parse_tier_cell(record.get(1).unwrap_or(""));
        let mut level = parse_level_cell(record.get(2).unwrap_or(""));

        if tier.is_none() && level.is_none() {
            tier = Some(MAX_OFFICER_TIER);
            level = Some(max_level_for_tier(MAX_OFFICER_TIER));
        } else if level.is_none() {
            if let Some(t) = tier {
                level = Some(max_level_for_tier(t));
            }
        }

        raw_records.push((name.to_string(), None, tier, level));
    }

    resolve_and_write_roster(path, &raw_records)
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

/// True if a roster entry represents an actually unlocked officer (not just "in roster" with 0/0/0).
fn is_unlocked(entry: &RosterEntry) -> bool {
    let r = entry.rank.unwrap_or(0);
    let l = entry.level.unwrap_or(0);
    r > 0 || l > 0
}

/// Loads the set of canonical officer IDs from the imported roster file.
/// Returns `None` if the file is missing or invalid (caller should then use the full canonical list).
/// Returns `Some(ids)` to filter crew generation to only officers the player owns.
pub fn load_imported_roster_ids(path: &str) -> Option<HashSet<String>> {
    load_imported_roster_ids_inner(path, false)
}

/// Like `load_imported_roster_ids` but only includes officers that are actually unlocked
/// (rank > 0 or level > 0). Use for "owned only" UI so officers synced as 0/0/0 (not yet unlocked) are excluded.
pub fn load_imported_roster_ids_unlocked_only(path: &str) -> Option<HashSet<String>> {
    load_imported_roster_ids_inner(path, true)
}

fn load_imported_roster_ids_inner(path: &str, unlocked_only: bool) -> Option<HashSet<String>> {
    #[derive(Debug, Deserialize)]
    struct ImportedRosterPayload {
        officers: Vec<RosterEntry>,
    }
    let raw = fs::read_to_string(path).ok()?;
    let payload: ImportedRosterPayload = serde_json::from_str(&raw).ok()?;
    let ids = payload
        .officers
        .into_iter()
        .filter(|e| !unlocked_only || is_unlocked(e))
        .map(|e| e.canonical_officer_id)
        .collect();
    Some(ids)
}

// ----- Loaders for synced research / buildings / ships -----

#[derive(Debug, Deserialize)]
struct ImportedResearchFile {
    research: Vec<ResearchEntry>,
}

#[derive(Debug, Deserialize)]
struct ImportedBuildingsFile {
    buildings: Vec<BuildingEntry>,
}

#[derive(Debug, Deserialize)]
struct ImportedShipsFile {
    ships: Vec<ShipEntry>,
}

/// Loads research entries from a synced/imported JSON file (e.g. [DEFAULT_RESEARCH_IMPORT_PATH]).
/// Returns `None` if the file is missing or invalid.
pub fn load_imported_research(path: &str) -> Option<Vec<ResearchEntry>> {
    let raw = fs::read_to_string(path).ok()?;
    let payload: ImportedResearchFile = serde_json::from_str(&raw).ok()?;
    Some(payload.research)
}

/// Loads building entries from a synced/imported JSON file (e.g. [DEFAULT_BUILDINGS_IMPORT_PATH]).
/// Returns `None` if the file is missing or invalid.
pub fn load_imported_buildings(path: &str) -> Option<Vec<BuildingEntry>> {
    let raw = fs::read_to_string(path).ok()?;
    let payload: ImportedBuildingsFile = serde_json::from_str(&raw).ok()?;
    Some(payload.buildings)
}

/// Loads ship entries from a synced/imported JSON file (e.g. [DEFAULT_SHIPS_IMPORT_PATH]).
/// Returns `None` if the file is missing or invalid.
pub fn load_imported_ships(path: &str) -> Option<Vec<ShipEntry>> {
    let raw = fs::read_to_string(path).ok()?;
    let payload: ImportedShipsFile = serde_json::from_str(&raw).ok()?;
    Some(payload.ships)
}
