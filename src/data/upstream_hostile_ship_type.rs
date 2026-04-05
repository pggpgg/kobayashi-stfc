//! Reverse-engineered mapping from data.stfc.space hostile `ship_type` (integer) to combat semantics.
//!
//! This is **not** hull class (battleship / explorer / interceptor); those come from upstream
//! `hull_type` → normalized `ship_class`. The JSON field `ship_type` is a separate game enum whose
//! labels are mostly in client localization (e.g. `armada_target_label` → "ARMADA TARGET" for `1`).
//!
//! Add new `match` arms here as ids are identified; unmapped values use [`UpstreamHostileShipTypeProfile::default`].

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
}
