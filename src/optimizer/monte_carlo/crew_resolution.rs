//! Crew resolution from officer names and candidate → crew seats/contexts.
//!
//! Officer abilities resolve through the LCARS resolver ([`resolve_crew_to_buff_set`]); there is no
//! placeholder/stub path. An officer name that doesn't resolve simply contributes no seat.

use std::collections::HashMap;

use crate::combat::CrewConfiguration;
use crate::data::officer::Officer;
use crate::lcars::{index_lcars_officers_by_id, resolve_crew_to_buff_set, ResolveOptions};
use crate::optimizer::crew_generator::CrewCandidate;

/// Build a [CrewConfiguration] from officer names (e.g. from a fight export) by resolving them
/// through LCARS — the same full-fidelity path the optimizer/sim use.
/// Convention: captain = Officer One, bridge = Officer Two then Officer Three, below_decks = [].
/// Empty or "--" names are skipped; a name that doesn't resolve contributes no seat. Tiers may be
/// given as `Name (TN)` suffixes; otherwise the officer's max rank is used.
pub fn crew_from_officer_names(
    captain: Option<&str>,
    bridge: Vec<String>,
    below_decks: Vec<String>,
) -> CrewConfiguration {
    let officers = crate::lcars::build_officer_model_default().unwrap_or_default();
    let by_id = index_lcars_officers_by_id(officers);
    let mut name_to_id: HashMap<String, String> = HashMap::new();
    for o in by_id.values() {
        name_to_id.insert(normalize_lookup_key(&o.name), o.id.clone());
        name_to_id.insert(normalize_lookup_key(&o.id), o.id.clone());
    }

    // Resolve each slot to an LCARS id, capturing any `(TN)` tier suffix. Scoped so the borrow of
    // `officer_tiers` is released before it is moved into `ResolveOptions`.
    let mut officer_tiers: HashMap<String, u8> = HashMap::new();
    let (captain_id, bridge_ids, below_ids) = {
        let mut to_id = |raw: &str| -> Option<String> {
            if is_empty_or_placeholder(raw) {
                return None;
            }
            let (name, tier) = split_name_and_tier(raw);
            let id = name_to_id.get(&normalize_lookup_key(&name)).cloned()?;
            if let Some(t) = tier {
                officer_tiers.insert(id.clone(), t);
            }
            Some(id)
        };
        let cap = captain.and_then(&mut to_id).unwrap_or_default();
        let br: Vec<String> = bridge.iter().filter_map(|n| to_id(n)).collect();
        let bd: Vec<String> = below_decks.iter().filter_map(|n| to_id(n)).collect();
        (cap, br, bd)
    };

    let options = ResolveOptions {
        tier: None,
        officer_tiers: (!officer_tiers.is_empty()).then_some(officer_tiers),
        officer_levels: None,
    };
    // Empty captain id (unresolved) is skipped by the resolver, like any absent id.
    resolve_crew_to_buff_set(&captain_id, &bridge_ids, &below_ids, &by_id, &options)
        .to_crew_config()
        .clone()
}

/// Unique canonical officer ids for captain + bridge + below (tier suffixes stripped).
///
/// Used for Conqueror Borg **Evolutionary Assimilation** (instant loss checks the same ids, via
/// [`crate::combat::SimulationConfig::attacker_roster_officer_ids`]).
pub(crate) fn roster_officer_ids_from_candidate(
    candidate: &CrewCandidate,
    officers_by_name: &HashMap<String, Officer>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |raw: &str| {
        if is_empty_or_placeholder(raw) {
            return;
        }
        let (lookup_name, _) = split_name_and_tier(raw);
        let Some(o) = officers_by_name.get(&normalize_lookup_key(&lookup_name)) else {
            return;
        };
        if !out.iter().any(|id| id == &o.id) {
            out.push(o.id.clone());
        }
    };
    push(&candidate.captain);
    for n in &candidate.bridge {
        push(n);
    }
    for n in &candidate.below_decks {
        push(n);
    }
    out
}

pub(crate) fn index_officers_by_name(officers: Vec<Officer>) -> HashMap<String, Officer> {
    officers
        .into_iter()
        .map(|officer| (normalize_lookup_key(&officer.name), officer))
        .collect()
}

fn is_empty_or_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.eq_ignore_ascii_case("--")
}

pub(crate) fn normalize_lookup_key(value: &str) -> String {
    // The filter keeps only ASCII alphanumerics, so `to_ascii_lowercase` is correct here and
    // far cheaper than `char::to_lowercase` (which returns an iterator per char to handle
    // Unicode case-folding). Pre-allocate to avoid grow-by-1 reallocations.
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

pub(crate) fn split_name_and_tier(input: &str) -> (String, Option<u8>) {
    let trimmed = input.trim();
    if let Some(open) = trimmed.rfind('(') {
        if trimmed.ends_with(')') {
            let inner = &trimmed[open + 1..trimmed.len() - 1];
            if let Some(rest) = inner.strip_prefix('T').or_else(|| inner.strip_prefix('t')) {
                if let Ok(tier) = rest.parse::<u8>() {
                    return (trimmed[..open].trim().to_string(), Some(tier));
                }
            }
        }
    }
    (trimmed.to_string(), None)
}

pub(crate) fn hash_identifier(value: &str) -> u64 {
    value.bytes().fold(14695981039346656037u64, |acc, b| {
        (acc ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}

pub(crate) fn seeded_variance(seed: u64) -> f64 {
    let mixed = seed
        .wrapping_add(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let unit = (mixed as f64) / (u64::MAX as f64);
    0.85 + (unit * 0.30)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_name_and_tier_parses_suffix() {
        assert_eq!(
            split_name_and_tier("Harry Kim (T5)"),
            ("Harry Kim".to_string(), Some(5))
        );
        assert_eq!(split_name_and_tier("Picard"), ("Picard".to_string(), None));
    }

    #[test]
    fn normalize_lookup_key_strips_non_alphanumeric_and_lowercases() {
        assert_eq!(normalize_lookup_key("Five of Eleven!"), "fiveofeleven");
    }

    #[test]
    fn is_empty_or_placeholder_detects_blanks_and_dashes() {
        assert!(is_empty_or_placeholder(""));
        assert!(is_empty_or_placeholder("  --  "));
        assert!(!is_empty_or_placeholder("Picard"));
    }
}
