//! Load and resolve hostiles and ships by name/id. Graceful fallback when data missing.
//! Ships: data/ships_extended/ (extended schema with tiers/levels). Flat data/ships/ removed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use crate::combat::export_csv::FightExport;
use crate::combat::OpponentFactionTag;
use crate::data::hostile::{
    load_hostile_index, load_hostile_record, HostileIndex, HostileRecord,
    DEFAULT_HOSTILES_INDEX_PATH,
};
use crate::data::hostile_loca::{load_hostile_loca_display_names, strip_stfc_color_tags};
use crate::data::ship::{
    load_extended_ship_index, load_extended_ship_record, CrewSlotUnlock, ExtendedShipIndex,
    ShipRecord, DEFAULT_SHIPS_EXTENDED_DIR,
};

/// Normalize a string for lookup: lowercase, collapse runs of whitespace/underscore into a single
/// `_`, trim leading and trailing separators.
///
/// Single-pass / single-allocation implementation. Replaces a 3-allocation form (`to_lowercase` +
/// `chars().map().collect::<String>()` + `split_whitespace().collect::<Vec<_>>().join("_")`) that
/// showed as the dominant `String::FromIterator<char>` source in profiling. Hot path: called by
/// `resolve_hostile_with_index` and `resolve_ship_with_tier_level` once per index entry per
/// scenario build (so up to O(ships + hostiles) String allocs per GA run on the old version).
pub fn normalize_lookup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    // Suppresses a leading `_` by treating the start of input as if the previous char were a
    // separator. Also collapses runs.
    let mut prev_was_sep = true;
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '_' {
            if !prev_was_sep {
                out.push('_');
                prev_was_sep = true;
            }
        } else {
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_was_sep = false;
        }
    }
    if out.ends_with('_') {
        out.pop();
    }
    out
}

/// Resolve a hostile using a pre-loaded index. Used by DataRegistry.
pub fn resolve_hostile_with_index(
    index: &HostileIndex,
    data_dir: &Path,
    name_or_id: &str,
) -> Option<HostileRecord> {
    let normalized = normalize_lookup(name_or_id);

    if let Some(entry) = index
        .hostiles
        .iter()
        .find(|e| normalize_lookup(&e.id) == normalized)
    {
        return load_hostile_record(data_dir, &entry.id);
    }
    for entry in &index.hostiles {
        let name_level = format!("{}_{}", normalize_lookup(&entry.hostile_name), entry.level);
        if name_level == normalized {
            return load_hostile_record(data_dir, &entry.id);
        }
        let name_space_level = format!("{} {}", normalize_lookup(&entry.hostile_name), entry.level);
        if normalize_lookup(&name_space_level) == normalized {
            return load_hostile_record(data_dir, &entry.id);
        }
    }
    let by_name: Vec<_> = index
        .hostiles
        .iter()
        .filter(|e| normalize_lookup(&e.hostile_name) == normalized)
        .collect();
    if by_name.len() == 1 {
        return load_hostile_record(data_dir, &by_name[0].id);
    }
    None
}

/// Defender faction + hostile metadata resolved from a parsed fight export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FightExportDefenderContext {
    pub defender_faction: OpponentFactionTag,
    pub defender_hull_faction_id: i64,
    pub resolved_hostile_id: Option<String>,
}

/// Parse a defender-faction slug (CLI `--defender-faction`, drift fixtures, import override).
pub fn opponent_faction_tag_from_slug(slug: &str) -> Result<OpponentFactionTag, String> {
    let t = slug.trim();
    if t.is_empty() {
        return Err("defender faction slug requires a non-empty value".to_string());
    }
    OpponentFactionTag::from_data_slug(t).ok_or_else(|| {
        format!(
            "unknown defender faction {t:?}; expected a slug such as klingon, romulan, federation, borg, swarm, or unknown"
        )
    })
}

fn normalize_display_name_for_lookup(name: &str) -> String {
    normalize_lookup(&strip_stfc_color_tags(name))
}

/// Resolve a hostile by game display name + level (from fight-export summary rows).
///
/// Returns `Ok(None)` when no index entry matches. When multiple entries match the same
/// display name and level, picks deterministically if they share the same combat faction tag
/// and upstream `faction.id`; otherwise returns an error listing candidate ids.
pub fn resolve_hostile_by_display_name(display_name: &str, level: u32) -> Result<Option<HostileRecord>, String> {
    let cache = HOSTILE_DISPLAY_RESOLVE_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let key = format!("{}|{level}", normalize_display_name_for_lookup(display_name));
    if let Some(hit) = cache.read().expect("hostile display cache poisoned").get(&key) {
        return hit.clone();
    }
    let resolved = resolve_hostile_by_display_name_uncached(display_name, level);
    cache
        .write()
        .expect("hostile display cache poisoned")
        .insert(key, resolved.clone());
    resolved
}

type HostileDisplayResolveCache = HashMap<String, Result<Option<HostileRecord>, String>>;

static HOSTILE_DISPLAY_RESOLVE_CACHE: OnceLock<RwLock<HostileDisplayResolveCache>> = OnceLock::new();

static HOSTILE_LOCA_DISPLAY_NAMES: OnceLock<HashMap<u64, String>> = OnceLock::new();

fn hostile_loca_display_names() -> &'static HashMap<u64, String> {
    HOSTILE_LOCA_DISPLAY_NAMES.get_or_init(|| {
        load_hostile_loca_display_names(Path::new(env!("CARGO_MANIFEST_DIR")))
    })
}

fn resolve_hostile_by_display_name_uncached(
    display_name: &str,
    level: u32,
) -> Result<Option<HostileRecord>, String> {
    let index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH)
        .ok_or_else(|| "hostile index missing".to_string())?;
    let data_dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH)
        .parent()
        .ok_or_else(|| "hostile data directory missing".to_string())?;
    resolve_hostile_by_display_name_with_index(
        &index,
        data_dir,
        hostile_loca_display_names(),
        display_name,
        level,
    )
}

pub(crate) fn resolve_hostile_by_display_name_with_index(
    index: &HostileIndex,
    data_dir: &Path,
    loca_names: &HashMap<u64, String>,
    display_name: &str,
    level: u32,
) -> Result<Option<HostileRecord>, String> {
    let normalized = normalize_display_name_for_lookup(display_name);
    if normalized.is_empty() {
        return Ok(None);
    }

    let mut candidate_ids: Vec<String> = index
        .hostiles
        .iter()
        .filter(|e| e.level == level)
        .filter_map(|e| {
            let loca_id = e.loca_id?;
            let label = loca_names.get(&loca_id)?;
            if normalize_display_name_for_lookup(label) == normalized {
                Some(e.id.clone())
            } else {
                None
            }
        })
        .collect();

    if candidate_ids.is_empty() {
        return Ok(None);
    }

    candidate_ids.sort_by(|a, b| {
        match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a_id), Ok(b_id)) => a_id.cmp(&b_id).then_with(|| a.cmp(b)),
            _ => a.cmp(b),
        }
    });

    if candidate_ids.len() == 1 {
        return load_hostile_record(data_dir, &candidate_ids[0])
            .ok_or_else(|| format!("hostile record missing for id {}", candidate_ids[0]))
            .map(Some);
    }

    let mut records = Vec::with_capacity(candidate_ids.len());
    for id in &candidate_ids {
        let Some(rec) = load_hostile_record(data_dir, id) else {
            return Err(format!("hostile record missing for id {id}"));
        };
        records.push(rec);
    }

    disambiguate_hostile_records(display_name, level, records).map(Some)
}

fn disambiguate_hostile_records(
    display_name: &str,
    level: u32,
    records: Vec<HostileRecord>,
) -> Result<HostileRecord, String> {
    if records.is_empty() {
        return Err("disambiguate_hostile_records: empty candidate list".to_string());
    }
    if records.len() == 1 {
        return Ok(records.into_iter().next().expect("len checked"));
    }

    let first_tag = records[0].opponent_faction_tag();
    let first_faction_id = records[0].faction.as_ref().map(|f| f.id).unwrap_or(-1);
    if records.iter().all(|r| {
        r.opponent_faction_tag() == first_tag
            && r.faction.as_ref().map(|f| f.id).unwrap_or(-1) == first_faction_id
    }) {
        return Ok(records
            .into_iter()
            .min_by(|a, b| a.id.cmp(&b.id))
            .expect("non-empty"));
    }

    let ids: Vec<_> = records.iter().map(|r| r.id.as_str()).collect();
    Err(format!(
        "ambiguous hostile display name {display_name:?} level {level}: conflicting faction tags among ids {}",
        ids.join(", ")
    ))
}

fn defender_context_from_hostile(rec: &HostileRecord) -> FightExportDefenderContext {
    FightExportDefenderContext {
        defender_faction: rec.opponent_faction_tag(),
        defender_hull_faction_id: rec.faction.as_ref().map(|f| f.id).unwrap_or(0),
        resolved_hostile_id: Some(rec.id.clone()),
    }
}

/// Resolve defender faction for a parsed fight export.
///
/// Precedence: explicit `faction_slug_override` wins, then enemy summary display name + level
/// lookup, then [`OpponentFactionTag::Unknown`].
pub fn defender_faction_for_fight_export(
    export: &FightExport,
    faction_slug_override: Option<&str>,
) -> Result<FightExportDefenderContext, String> {
    if let Some(slug) = faction_slug_override {
        return Ok(FightExportDefenderContext {
            defender_faction: opponent_faction_tag_from_slug(slug)?,
            defender_hull_faction_id: 0,
            resolved_hostile_id: None,
        });
    }

    match (export.enemy_player_name.as_deref(), export.enemy_ship_level) {
        (Some(name), Some(level)) => match resolve_hostile_by_display_name(name, level)? {
            Some(rec) => Ok(defender_context_from_hostile(&rec)),
            None => Ok(FightExportDefenderContext {
                defender_faction: OpponentFactionTag::Unknown,
                defender_hull_faction_id: 0,
                resolved_hostile_id: None,
            }),
        },
        _ => Ok(FightExportDefenderContext {
            defender_faction: OpponentFactionTag::Unknown,
            defender_hull_faction_id: 0,
            resolved_hostile_id: None,
        }),
    }
}

/// Process-wide memoization for [`resolve_hostile`]. The bundled hostile index is ~1 MB of JSON;
/// re-reading + re-parsing it (and scanning every entry with [`normalize_lookup`]) on each call is
/// the dominant cost when the analytical prefilter sorts ~10^5 candidates, all resolving the same
/// hostile. Data files are static for a process lifetime (same assumption as the record-level LRU
/// in `data_registry.rs`), so caching the resolved record by lookup key is sound. Negative results
/// are cached too, to avoid re-scanning the index on repeated misses.
static HOSTILE_RESOLVE_CACHE: OnceLock<RwLock<HashMap<String, Option<HostileRecord>>>> =
    OnceLock::new();

/// Resolve a hostile by id or by "name level" / "name_level". Returns None if index missing or no match.
///
/// Memoized process-wide (see [`HOSTILE_RESOLVE_CACHE`]). The first call for a given key parses the
/// hostile index once; subsequent calls are an O(1) lookup + a small record clone.
pub fn resolve_hostile(name_or_id: &str) -> Option<HostileRecord> {
    let cache = HOSTILE_RESOLVE_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(hit) = cache
        .read()
        .expect("hostile cache poisoned")
        .get(name_or_id)
    {
        return hit.clone();
    }
    let resolved = resolve_hostile_uncached(name_or_id);
    cache
        .write()
        .expect("hostile cache poisoned")
        .insert(name_or_id.to_string(), resolved.clone());
    resolved
}

/// Uncached resolution body for [`resolve_hostile`].
fn resolve_hostile_uncached(name_or_id: &str) -> Option<HostileRecord> {
    let index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH)?;
    let data_dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH).parent()?;
    resolve_hostile_with_index(&index, data_dir, name_or_id)
}

/// Resolve a ship by id or ship_name. Returns None if index missing or no match.
pub fn resolve_ship(name_or_id: &str) -> Option<ShipRecord> {
    resolve_ship_with_tier_level(name_or_id, None, None)
}

/// Resolve a ship by id or ship_name, with optional tier and level (1-based).
/// Uses data/ships_extended only (Option B: extended schema with tiers/levels, resolved at request time).
/// Defaults to tier=1, level=1 when tier/level not specified.
/// Process-wide memoization for [`resolve_ship_with_tier_level`], keyed by (name/id, tier, level).
/// Same rationale as [`HOSTILE_RESOLVE_CACHE`]: the fallback scenario path re-resolves the attacker
/// ship per candidate during the analytical prefilter sort. The ship index is small (~12 KB) but
/// the per-call parse + `to_ship_record` tier/level resolution still adds up across ~10^5 calls.
#[allow(clippy::type_complexity)]
static SHIP_RESOLVE_CACHE: OnceLock<
    RwLock<HashMap<(String, Option<u32>, Option<u32>), Option<ShipRecord>>>,
> = OnceLock::new();

pub fn resolve_ship_with_tier_level(
    name_or_id: &str,
    tier: Option<u32>,
    level: Option<u32>,
) -> Option<ShipRecord> {
    let cache = SHIP_RESOLVE_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let key = (name_or_id.to_string(), tier, level);
    if let Some(hit) = cache.read().expect("ship cache poisoned").get(&key) {
        return hit.clone();
    }
    let resolved = resolve_ship_with_tier_level_uncached(name_or_id, tier, level);
    cache
        .write()
        .expect("ship cache poisoned")
        .insert(key, resolved.clone());
    resolved
}

/// Uncached resolution body for [`resolve_ship_with_tier_level`].
fn resolve_ship_with_tier_level_uncached(
    name_or_id: &str,
    tier: Option<u32>,
    level: Option<u32>,
) -> Option<ShipRecord> {
    let normalized = normalize_lookup(name_or_id);
    let extended_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);

    if !extended_dir.is_dir() {
        return None;
    }
    let ext_index = load_extended_ship_index(extended_dir)?;
    let id = ext_index
        .ships
        .iter()
        .find(|e| {
            normalize_lookup(&e.id) == normalized || normalize_lookup(&e.ship_name) == normalized
        })
        .map(|e| e.id.as_str())?;
    let extended = load_extended_ship_record(extended_dir, id)?;
    extended.to_ship_record(tier.or(Some(1)), level.or(Some(1)))
}

/// Resolve a ship using a pre-loaded index (avoids re-reading index.json from disk).
/// Returns None if the ship id is not found or the individual record file is missing.
pub fn resolve_ship_with_tier_level_from_index(
    index: &ExtendedShipIndex,
    extended_dir: &Path,
    name_or_id: &str,
    tier: Option<u32>,
    level: Option<u32>,
) -> Option<ShipRecord> {
    let normalized = normalize_lookup(name_or_id);
    let id = index
        .ships
        .iter()
        .find(|e| {
            normalize_lookup(&e.id) == normalized || normalize_lookup(&e.ship_name) == normalized
        })
        .map(|e| e.id.as_str())?;
    let extended = load_extended_ship_record(extended_dir, id)?;
    extended.to_ship_record(tier.or(Some(1)), level.or(Some(1)))
}

/// Return available tier and level numbers plus below-decks unlock schedule. From `data/ships_extended`.
/// Returns None if no extended ship file.
pub fn ship_tiers_levels_and_crew_slots(
    name_or_id: &str,
) -> Option<(Vec<u32>, Vec<u32>, Vec<CrewSlotUnlock>)> {
    let normalized = normalize_lookup(name_or_id);
    let extended_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    if !extended_dir.is_dir() {
        return None;
    }
    let ext_index = load_extended_ship_index(extended_dir)?;
    let id = ext_index
        .ships
        .iter()
        .find(|e| {
            normalize_lookup(&e.id) == normalized || normalize_lookup(&e.ship_name) == normalized
        })
        .map(|e| e.id.as_str())?;
    let extended = load_extended_ship_record(extended_dir, id)?;
    let tiers: Vec<u32> = extended.tiers.iter().map(|t| t.tier).collect();
    let levels: Vec<u32> = extended.levels.iter().map(|l| l.level).collect();
    Some((tiers, levels, extended.crew_slots))
}

/// Like [`ship_tiers_levels_and_crew_slots`] but uses a pre-loaded index to avoid re-reading index.json.
pub fn ship_tiers_levels_and_crew_slots_from_index(
    index: &ExtendedShipIndex,
    extended_dir: &Path,
    name_or_id: &str,
) -> Option<(Vec<u32>, Vec<u32>, Vec<CrewSlotUnlock>)> {
    let normalized = normalize_lookup(name_or_id);
    let id = index
        .ships
        .iter()
        .find(|e| {
            normalize_lookup(&e.id) == normalized || normalize_lookup(&e.ship_name) == normalized
        })
        .map(|e| e.id.as_str())?;
    let extended = load_extended_ship_record(extended_dir, id)?;
    let tiers: Vec<u32> = extended.tiers.iter().map(|t| t.tier).collect();
    let levels: Vec<u32> = extended.levels.iter().map(|l| l.level).collect();
    Some((tiers, levels, extended.crew_slots))
}

/// Return available tier and level numbers for a ship (by id or name). From data/ships_extended.
/// Returns (tiers, levels); if no extended data, returns None.
pub fn ship_tiers_levels(name_or_id: &str) -> Option<(Vec<u32>, Vec<u32>)> {
    ship_tiers_levels_and_crew_slots(name_or_id).map(|(t, l, _)| (t, l))
}

/// Resolve defender faction for standalone `kobayashi simulate` (and similar CLI paths).
///
/// Precedence: `faction_slug` from `--defender-faction` wins over `hostile_lookup` from `--hostile`.
/// If neither is set, returns [`OpponentFactionTag::Unknown`] (same as [`crate::combat::simulate_combat`]).
pub fn defender_faction_for_cli_simulate(
    faction_slug: Option<&str>,
    hostile_lookup: Option<&str>,
) -> Result<OpponentFactionTag, String> {
    if let Some(slug) = faction_slug {
        return opponent_faction_tag_from_slug(slug);
    }
    if let Some(hostile) = hostile_lookup {
        let key = hostile.trim();
        if key.is_empty() {
            return Err("--hostile requires a non-empty value".to_string());
        }
        let rec = resolve_hostile(key).ok_or_else(|| {
            format!(
                "could not resolve hostile {key:?} from data/hostiles index (try numeric id or \"name level\")"
            )
        })?;
        return Ok(rec.opponent_faction_tag());
    }
    Ok(OpponentFactionTag::Unknown)
}

/// Upstream `faction.id` from a resolved hostile (`--hostile`), or `0` when not applicable.
pub fn defender_hull_faction_id_for_cli_simulate(hostile_lookup: Option<&str>) -> i64 {
    let Some(key) = hostile_lookup.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return 0;
    };
    resolve_hostile(key)
        .and_then(|rec| rec.faction.map(|f| f.id))
        .unwrap_or(0)
}

#[cfg(test)]
mod defender_faction_cli_tests {
    use super::defender_faction_for_cli_simulate;
    use super::defender_faction_for_fight_export;
    use super::defender_hull_faction_id_for_cli_simulate;
    use super::disambiguate_hostile_records;
    use super::opponent_faction_tag_from_slug;
    use super::resolve_hostile_by_display_name;
    use crate::combat::export_csv::FightExport;
    use crate::combat::{OpponentFactionTag, ShipType};

    #[test]
    fn explicit_slug_and_none_default() {
        assert_eq!(
            defender_faction_for_cli_simulate(Some("klingon"), None).unwrap(),
            OpponentFactionTag::Klingon
        );
        assert_eq!(
            defender_faction_for_cli_simulate(Some("mirror-universe"), None).unwrap(),
            OpponentFactionTag::MirrorUniverse
        );
        assert_eq!(
            defender_faction_for_cli_simulate(Some("unknown"), None).unwrap(),
            OpponentFactionTag::Unknown
        );
        assert_eq!(
            defender_faction_for_cli_simulate(None, None).unwrap(),
            OpponentFactionTag::Unknown
        );
    }

    #[test]
    fn bad_slug_errors() {
        assert!(defender_faction_for_cli_simulate(Some("not_a_real_faction"), None).is_err());
    }

    #[test]
    fn explicit_slug_wins_over_hostile_token() {
        assert_eq!(
            defender_faction_for_cli_simulate(Some("romulan"), Some("2918121098")).unwrap(),
            OpponentFactionTag::Romulan
        );
    }

    #[test]
    fn hostile_numeric_id_resolves_when_data_present() {
        let tag = defender_faction_for_cli_simulate(None, Some("2918121098"));
        assert!(
            tag.is_ok(),
            "bundled hostiles should resolve default optimize id: {tag:?}"
        );
    }

    #[test]
    fn hostile_lookup_sets_defender_hull_faction_id_when_present() {
        let id = defender_hull_faction_id_for_cli_simulate(Some("2918121098"));
        assert_ne!(id, 0, "bundled hostile should carry faction.id");
    }

    #[test]
    fn resolve_hostile_by_display_name_finds_takret_militia_level_10() {
        let rec = resolve_hostile_by_display_name("Takret Militia", 10)
            .expect("lookup")
            .expect("takret should resolve");
        assert!(
            rec.id == "845501025" || rec.id == "1973028640",
            "unexpected takret id: {}",
            rec.id
        );
    }

    #[test]
    fn resolve_hostile_by_display_name_romulan_intruder_level_38() {
        let rec = resolve_hostile_by_display_name("Romulan Intruder", 38)
            .expect("lookup")
            .expect("romulan intruder should resolve uniquely");
        assert_eq!(rec.id, "287240");
        assert_eq!(rec.opponent_faction_tag(), OpponentFactionTag::Romulan);
    }

    #[test]
    fn defender_faction_for_fight_export_auto_romulan_intruder() {
        let export = FightExport {
            attacker_won: true,
            rounds: 1,
            defender_hull_remaining: 0.0,
            defender_shield_remaining: 0.0,
            total_damage: 0.0,
            player_fleet: Default::default(),
            enemy_fleet: Default::default(),
            events: vec![],
            player_ship_name: None,
            player_officer_one: None,
            player_officer_two: None,
            player_officer_three: None,
            attacker_ship_type: ShipType::Battleship,
            enemy_player_name: Some("Romulan Intruder".into()),
            enemy_ship_level: Some(38),
            enemy_ship_strength: None,
        };
        let ctx = defender_faction_for_fight_export(&export, None).expect("resolve");
        assert_eq!(ctx.defender_faction, OpponentFactionTag::Romulan);
        assert_eq!(ctx.resolved_hostile_id.as_deref(), Some("287240"));
        assert_ne!(ctx.defender_hull_faction_id, 0);
    }

    #[test]
    fn defender_faction_for_fight_export_override_wins() {
        let export = FightExport {
            attacker_won: true,
            rounds: 1,
            defender_hull_remaining: 0.0,
            defender_shield_remaining: 0.0,
            total_damage: 0.0,
            player_fleet: Default::default(),
            enemy_fleet: Default::default(),
            events: vec![],
            player_ship_name: None,
            player_officer_one: None,
            player_officer_two: None,
            player_officer_three: None,
            attacker_ship_type: ShipType::Battleship,
            enemy_player_name: Some("Romulan Intruder".into()),
            enemy_ship_level: Some(38),
            enemy_ship_strength: None,
        };
        let ctx = defender_faction_for_fight_export(&export, Some("klingon")).expect("resolve");
        assert_eq!(ctx.defender_faction, OpponentFactionTag::Klingon);
        assert!(ctx.resolved_hostile_id.is_none());
        assert_eq!(ctx.defender_hull_faction_id, 0);
    }

    #[test]
    fn opponent_faction_tag_from_slug_rejects_unknown_token() {
        assert!(opponent_faction_tag_from_slug("not_a_real_faction").is_err());
    }

    #[test]
    fn disambiguate_hostile_records_errors_on_conflicting_factions() {
        let klingon: HostileRecord = serde_json::from_str(
            r#"{
            "id":"9001","hostile_name":"H","level":5,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":4153667145,"loca_id":2}
        }"#,
        )
        .expect("klingon json");
        let romulan: HostileRecord = serde_json::from_str(
            r#"{
            "id":"9002","hostile_name":"H","level":5,"ship_class":"battleship",
            "armor":1.0,"shield_deflection":1.0,"dodge":1.0,"hull_health":100.0,"shield_health":50.0,
            "faction":{"id":669838839,"loca_id":3}
        }"#,
        )
        .expect("romulan json");
        let err = disambiguate_hostile_records("Conflict Hostile", 5, vec![klingon, romulan])
            .expect_err("conflicting factions");
        assert!(
            err.contains("ambiguous hostile display name"),
            "unexpected err: {err}"
        );
    }
}
