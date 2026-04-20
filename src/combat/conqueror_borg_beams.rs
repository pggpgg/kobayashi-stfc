//! **Quantum Resonance Beam** (Conqueror Borg Suppressor) and **Hyperthermic Resonance Beam**
//! (Conqueror Borg Obliterator), Update 89. Source: official Borg Sphere highlight / in-game strings
//! (`translations-ship_buffs`: 89101–89105 Quantum, 89107–89109 Hyperthermic).
//!
//! **Quantum Nullification Pulse** on the Borg Sphere is [`AbilityEffect::ConquerorBorgBeamSuppression`]
//! at combat begin vs tagged Conqueror Borg; it disables both resonance beams when that effect
//! resolves. This module also treats the **Borg Sphere hull** (`attacker` id `borg_sphere`) as
//! having effective suppression so instant-loss beams do not fire when hull identity is known even
//! if crew resolution omits the marker effect.

use crate::combat::hostile_tags::{
    HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR, HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR,
};
use crate::combat::rng::Rng;

/// True when the attacking ship is the Borg Sphere extended hull id (`borg_sphere`).
#[inline]
pub fn attacker_hull_is_borg_sphere(ship_id: &str) -> bool {
    ship_id.trim().eq_ignore_ascii_case("borg_sphere")
}

/// Beam suppression from [`AbilityEffect::ConquerorBorgBeamSuppression`], or implicit from Borg Sphere hull.
#[inline]
pub fn effective_conqueror_borg_beam_suppression(
    crew_combat_begin_suppression: bool,
    attacker_ship_id: &str,
) -> bool {
    crew_combat_begin_suppression || attacker_hull_is_borg_sphere(attacker_ship_id)
}

/// **Quantum Resonance Beam:** destroys any player ship that is not a Borg Sphere when the beam is active.
#[inline]
pub fn quantum_resonance_beam_instant_loss(
    defender_is_npc_hostile: bool,
    defender_hostile_tag_mask: u32,
    effective_beam_suppression: bool,
    attacker_ship_id: &str,
) -> bool {
    if !defender_is_npc_hostile || effective_beam_suppression {
        return false;
    }
    if defender_hostile_tag_mask & HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR == 0 {
        return false;
    }
    !attacker_hull_is_borg_sphere(attacker_ship_id)
}

/// **Hyperthermic Resonance Beam:** destroys non–Borg Sphere ships; vs Borg Sphere, 80% chance of
/// immediate hull elimination (Hyperthermic Decay) when the beam is active.
#[inline]
pub fn hyperthermic_resonance_beam_instant_loss(
    defender_is_npc_hostile: bool,
    defender_hostile_tag_mask: u32,
    effective_beam_suppression: bool,
    attacker_ship_id: &str,
    seed: u64,
) -> bool {
    if !defender_is_npc_hostile || effective_beam_suppression {
        return false;
    }
    if defender_hostile_tag_mask & HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR == 0 {
        return false;
    }
    if !attacker_hull_is_borg_sphere(attacker_ship_id) {
        return true;
    }
    let mut r = Rng::new(seed ^ 0x48425F48_54484D52);
    let u = r.next_u64();
    (u as f64 / u64::MAX as f64) < 0.8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantum_resonance_kills_non_sphere_vs_suppressor_when_unsuppressed() {
        assert!(quantum_resonance_beam_instant_loss(
            true,
            HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR,
            false,
            "uss_voyager",
        ));
    }

    #[test]
    fn quantum_resonance_spares_borg_sphere_hull_without_crew_marker() {
        assert!(!quantum_resonance_beam_instant_loss(
            true,
            HOSTILE_TAG_MASK_CONQUEROR_BORG_SUPPRESSOR,
            false,
            "borg_sphere",
        ));
    }

    #[test]
    fn hyperthermic_kills_non_sphere_vs_obliterator() {
        assert!(hyperthermic_resonance_beam_instant_loss(
            true,
            HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR,
            false,
            "explorer",
            1,
        ));
    }

    #[test]
    fn hyperthermic_borg_sphere_80_percent_is_deterministic_by_seed() {
        let hit = (0_u64..50_000).find(|&s| {
            hyperthermic_resonance_beam_instant_loss(
                true,
                HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR,
                false,
                "borg_sphere",
                s,
            )
        });
        let miss = (0_u64..50_000).find(|&s| {
            !hyperthermic_resonance_beam_instant_loss(
                true,
                HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR,
                false,
                "borg_sphere",
                s,
            )
        });
        assert!(hit.is_some() && miss.is_some());
    }
}
