//! Bitmask tags for NPC hostiles, used to gate ship hull abilities (e.g. Borg Sphere vs Conqueror Borg).

/// Conqueror Borg Suppressor / Obliterator and related Update 89 hostiles.
pub const HOSTILE_TAG_MASK_CONQUEROR_BORG: u32 = 1 << 0;

/// Map a single data slug (from `ShipAbility` / hostile JSON) to one bit, if known.
pub fn hostile_tag_mask_for_slug(slug: &str) -> Option<u32> {
    let s = slug.trim().to_lowercase().replace('-', "_");
    match s.as_str() {
        "conqueror_borg" => Some(HOSTILE_TAG_MASK_CONQUEROR_BORG),
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
