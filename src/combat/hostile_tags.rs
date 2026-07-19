//! Bitmask tags for NPC hostiles, used to gate ship hull abilities (e.g. Borg Sphere vs Conqueror Borg).

/// Conqueror Borg Suppressor / Obliterator and related Update 89 hostiles.
pub const HOSTILE_TAG_MASK_CONQUEROR_BORG: u32 = 1 << 0;
/// [`crate::combat::conqueror_borg_beams`] — Conqueror Borg Suppressor family (`loca_id` 89050–89052).
pub const HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR: u32 = 1 << 1;
/// [`crate::combat::conqueror_borg_beams`] — Conqueror Borg Obliterator family (`loca_id` 89053–89055).
pub const HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR: u32 = 1 << 2;

/// Aggregation faction hostiles (faction `loca_id` 82001); hyperthermic decay + Recon Locus stabilizer.
pub const HOSTILE_TAG_MASK_AGGREGATION_HOSTILE: u32 = 1 << 3;
/// Gorn Hunter hostiles (Pteran, Acrocanth, Macronyx — `loca_id` 65100–65102); Gorn Eviscerator Hunt the Hunters.
pub const HOSTILE_TAG_MASK_GORN_HUNTER: u32 = 1 << 4;
/// Breen Warship family (Warship, Elite, Primarch — `loca_id` 80600–80602); U.S.S. Vengeance Advanced Sabotage.
pub const HOSTILE_TAG_MASK_BREEN_WARSHIP: u32 = 1 << 5;

/// Map a single data slug (from `ShipAbility` / hostile JSON) to one bit, if known.
pub fn hostile_tag_mask_for_slug(slug: &str) -> Option<u32> {
    let s = slug.trim().to_lowercase().replace('-', "_");
    match s.as_str() {
        "conqueror_borg" => Some(HOSTILE_TAG_MASK_CONQUEROR_BORG),
        "conqueror_borg_suppressor" => Some(HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR),
        "conqueror_borg_obliterator" => Some(HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR),
        "aggregation_hostile" => Some(HOSTILE_TAG_MASK_AGGREGATION_HOSTILE),
        "gorn_hunter" => Some(HOSTILE_TAG_MASK_GORN_HUNTER),
        "breen_warship" => Some(HOSTILE_TAG_MASK_BREEN_WARSHIP),
        _ => None,
    }
}

/// OR together all tag bits from `hostile_tags` on a [`crate::data::hostile::HostileRecord`].
/// Unknown slugs are ignored so forward-compatible data does not break loading.
pub fn mask_from_slugs(slugs: &[String]) -> u32 {
    let mut m = 0u32;
    for s in slugs {
        if let Some(b) = hostile_tag_mask_for_slug(s) {
            m |= b;
        }
    }
    m
}

/// Bits that must all be set on the defender mask for a gated ability to fire.
/// Returns `None` if any required slug is unknown (caller should drop the ability).
pub fn required_mask_from_condition_slugs(slugs: &[String]) -> Option<u32> {
    let mut bits = 0u32;
    for s in slugs {
        bits |= hostile_tag_mask_for_slug(s)?;
    }
    Some(bits)
}
