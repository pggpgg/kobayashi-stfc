//! Load and resolve hostiles and ships by name/id. Graceful fallback when data missing.
//! Ships: data/ships_extended/ (extended schema with tiers/levels). Flat data/ships/ removed.

use std::path::Path;

use crate::combat::OpponentFactionTag;
use crate::data::hostile::{
    load_hostile_index, load_hostile_record, HostileIndex, HostileRecord,
    DEFAULT_HOSTILES_INDEX_PATH,
};
use crate::data::ship::{
    load_extended_ship_index, load_extended_ship_record, CrewSlotUnlock, ShipRecord,
    DEFAULT_SHIPS_EXTENDED_DIR,
};

/// Normalize a string for lookup: lowercase, collapse runs of whitespace/underscore into a single
/// `_`, trim leading and trailing separators.
///
/// Single-pass / single-allocation implementation. Replaces a 3-allocation form (`to_lowercase` +
/// `chars().map().collect::<String>()` + `split_whitespace().collect::<Vec<_>>().join("_")`) that
/// showed as the dominant `String::FromIterator<char>` source in profiling. Hot path: called by
/// `resolve_hostile_with_index` and `resolve_ship_with_tier_level` once per index entry per
/// scenario build (so up to O(ships + hostiles) String allocs per GA run on the old version).
fn normalize_lookup(s: &str) -> String {
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

/// Resolve a hostile by id or by "name level" / "name_level". Returns None if index missing or no match.
pub fn resolve_hostile(name_or_id: &str) -> Option<HostileRecord> {
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
pub fn resolve_ship_with_tier_level(
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
        let t = slug.trim();
        if t.is_empty() {
            return Err("--defender-faction requires a non-empty value".to_string());
        }
        return OpponentFactionTag::from_data_slug(t).ok_or_else(|| {
            format!(
                "unknown --defender-faction {t:?}; expected a slug such as klingon, romulan, federation, borg, swarm, or unknown"
            )
        });
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
    use super::defender_hull_faction_id_for_cli_simulate;
    use crate::combat::OpponentFactionTag;

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
}
