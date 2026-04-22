//! Reverse-engineered mapping from data.stfc.space hostile `ship_type` (integer) to combat semantics.
//!
//! This is **not** hull class (battleship / explorer / interceptor); those come from upstream
//! `hull_type` → normalized `ship_class` via [`crate::data::hostile::hostile_hull_type_raw_to_ship_class`].
//! The JSON field `ship_type` is a separate game enum whose
//! labels are mostly in client localization (e.g. `armada_target_label` → "ARMADA TARGET" for `1`).
//!
//! Maintainer enumeration of ids observed in the hostile index (labels, evidence, row counts):
//! `docs/UPSTREAM_HOSTILE_SHIP_TYPES.md`.
//!
//! Add new `match` arms here only when combat semantics are confirmed; unmapped values use
//! [`UpstreamHostileShipTypeProfile::default`].

/// Per-upstream-`ship_type` flags for simulator behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamHostileShipTypeProfile {
    /// When true, the defender is an **armada target**: use [`crate::combat::ShipType::Armada`] for
    /// mitigation coefficients, pierce-through vs that class, and LCARS `defender_ship_type_is` /
    /// opponent-class gates tied to `armada`.
    pub is_armada_target: bool,
    /// Maintainer-facing note (provenance / UI string / stfc.space observation).
    pub note: &'static str,
}

impl Default for UpstreamHostileShipTypeProfile {
    fn default() -> Self {
        Self {
            is_armada_target: false,
            note: "unmapped upstream ship_type; hull-derived ship_class only",
        }
    }
}

/// Distinct `upstream_ship_type` values documented in `docs/UPSTREAM_HOSTILE_SHIP_TYPES.md`.
///
/// When a hostile refresh introduces a new category id, extend this slice and the doc together.
/// Numeric **9** is intentionally omitted until it appears in upstream data.
pub const KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES: &[u32] =
    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14];

/// Upstream category ids that appear in committed data **before** maintainer triage lands in
/// [`KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES`] and the reference doc.
///
/// - Must be sorted by `u32` and unique (enforced by a unit test).
/// - [`crate::data::validate::validate_hostiles_dataset`] emits a **warning** (not an error) for
///   these rows until they are promoted to `KNOWN` (and documented) or removed from this list.
pub const DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES: &[(u32, &str)] = &[];

fn deferral_reason_lookup<'a>(ship_type: u32, deferrals: &[(u32, &'a str)]) -> Option<&'a str> {
    deferrals
        .binary_search_by_key(&ship_type, |(v, _)| *v)
        .ok()
        .map(|i| deferrals[i].1)
}

/// Maintainer allow-list: when present, `validate_data` warns instead of erroring for this id.
#[inline]
pub fn upstream_ship_type_deferral_reason(ship_type: u32) -> Option<&'static str> {
    deferral_reason_lookup(ship_type, DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES)
}

/// True when `ship_type` is a maintainer-documented hostile category (not necessarily a dedicated combat `match` arm).
#[inline]
pub fn upstream_ship_type_is_known_category(ship_type: u32) -> bool {
    KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES
        .binary_search(&ship_type)
        .is_ok()
}

/// True when [`upstream_hostile_ship_type_profile`] uses a dedicated `match` arm (not the `_` fallback).
///
/// Keep this aligned with every non-`_` arm in [`upstream_hostile_ship_type_profile`].
pub fn upstream_ship_type_is_explicitly_mapped(ship_type: u32) -> bool {
    matches!(ship_type, 1)
}

/// Resolve combat semantics for the upstream hostile `ship_type` field (`HostileRecord::upstream_ship_type`).
pub fn upstream_hostile_ship_type_profile(ship_type: u32) -> UpstreamHostileShipTypeProfile {
    match ship_type {
        1 => UpstreamHostileShipTypeProfile {
            is_armada_target: true,
            note: "ARMADA TARGET (loca key armada_target_label); data.stfc.space hostiles/*.json ship_type: 1",
        },
        _ => UpstreamHostileShipTypeProfile::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_type_1_is_armada_target() {
        let p = upstream_hostile_ship_type_profile(1);
        assert!(p.is_armada_target);
    }

    #[test]
    fn ship_type_zero_unmapped() {
        let p = upstream_hostile_ship_type_profile(0);
        assert!(!p.is_armada_target);
    }

    #[test]
    fn explicit_mapping_matches_profile_arms() {
        assert!(upstream_ship_type_is_explicitly_mapped(1));
        assert!(!upstream_ship_type_is_explicitly_mapped(0));
        assert!(!upstream_ship_type_is_explicitly_mapped(2));
    }

    #[test]
    fn known_categories_sorted_unique() {
        for w in KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES.windows(2) {
            assert!(
                w[0] < w[1],
                "KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES must be sorted unique"
            );
        }
    }

    #[test]
    fn known_category_examples() {
        assert!(upstream_ship_type_is_known_category(0));
        assert!(upstream_ship_type_is_known_category(14));
        assert!(!upstream_ship_type_is_known_category(9));
        assert!(!upstream_ship_type_is_known_category(99));
    }

    #[test]
    fn deferral_reason_lookup_sorted_pairs() {
        let d: &[(u32, &str)] = &[(2, "a"), (40, "b")];
        assert_eq!(super::deferral_reason_lookup(2, d), Some("a"));
        assert_eq!(super::deferral_reason_lookup(40, d), Some("b"));
        assert_eq!(super::deferral_reason_lookup(3, d), None);
    }

    #[test]
    fn deferred_upstream_ship_types_sorted_unique() {
        for w in DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES must be sorted unique by u32"
            );
        }
    }

    #[test]
    fn empty_deferral_table_yields_no_reason() {
        assert_eq!(upstream_ship_type_deferral_reason(9), None);
        assert_eq!(upstream_ship_type_deferral_reason(0), None);
    }
}
