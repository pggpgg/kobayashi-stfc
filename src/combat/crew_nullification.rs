//! Some hostiles apply **crew nullification**: listed canonical officers contribute **no** combat
//! effects (no dynamic seats, combat-begin static buffs, or extra-attack proc) while that hostile
//! is the defending NPC. Data: hostile JSON [`crate::data::hostile::HostileRecord::hostile_tags`]
//! includes slug `crew_nullification` (see normalizer).

use std::collections::HashSet;

use crate::combat::abilities::CrewSeatContext;
use crate::combat::hostile_tags::HOSTILE_TAG_MASK_CREW_NULLIFICATION;

/// Canonical LCARS / roster ids (same as `data/officers/officers.canonical.json` `id` values).
/// Update 89: Conqueror Borg Suppressor / Obliterator; align with in-game “no effect” captains.
pub const NULLIFIED_OFFICER_IDS: &[&str] = &[
    "kathryn-janeway-bd4a19",
    "ent-e-picard-556227",
    "pike-1e7d0d",
];

#[inline]
pub fn officer_id_nullified_by_crew_nullification(officer_id: &str) -> bool {
    NULLIFIED_OFFICER_IDS
        .iter()
        .any(|&id| id.eq_ignore_ascii_case(officer_id))
}

/// When `Some`, pass to [`crate::lcars::resolve_crew_to_buff_set`] so nullified officers are skipped entirely.
pub fn nullified_officer_id_set_for_mask(
    defender_hostile_tag_mask: u32,
    defender_is_npc_hostile: bool,
) -> Option<HashSet<String>> {
    if !defender_is_npc_hostile {
        return None;
    }
    if defender_hostile_tag_mask & HOSTILE_TAG_MASK_CREW_NULLIFICATION == 0 {
        return None;
    }
    Some(
        NULLIFIED_OFFICER_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    )
}

/// Defense in depth for legacy name-based [`crate::optimizer::monte_carlo::crew_resolution::build_crew_seats`]
/// rows that still carry `officer_id`.
pub fn filter_crew_seats_for_crew_nullification(
    seats: Vec<CrewSeatContext>,
    defender_hostile_tag_mask: u32,
    defender_is_npc_hostile: bool,
) -> Vec<CrewSeatContext> {
    let Some(ids) = nullified_officer_id_set_for_mask(defender_hostile_tag_mask, defender_is_npc_hostile)
    else {
        return seats;
    };
    seats
        .into_iter()
        .filter(|s| match &s.officer_id {
            Some(oid) => !ids.contains(oid),
            None => true,
        })
        .collect()
}
