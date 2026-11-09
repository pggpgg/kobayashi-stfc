//! **Evolutionary Assimilation** (Conqueror Borg Suppressor / Obliterator, Update 89).
//!
//! If certain officers are on your attacking roster against a **Conqueror Borg** NPC hostile, the
//! hostile **destroys your ship** (combat logs: hull eliminated while shields can remain). The
//! canonical officer ids below match Help Center / in-game “forbidden” captains for this family
//! (Janeway, Enterprise‑E Picard, Pike) until upstream provides an explicit machine-readable list.
//!
//! Quantum Nullification Pulse suppresses resonance beams, not Evolutionary Assimilation.
//! Scopely's Sphere help page explicitly states that these officers cause an instant hull loss:
//! <https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/8763-the-borg-sphere/>

use crate::combat::hostile_tags::HOSTILE_TAG_MASK_CONQUEROR_BORG;

/// Canonical officer ids that trigger instant loss vs Conqueror Borg when on the attacking roster.
pub const EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS: &[&str] = &[
    "kathryn-janeway-bd4a19",
    "ent-e-picard-556227",
    "pike-1e7d0d",
];

#[inline]
pub fn officer_triggers_evolutionary_assimilation(officer_id: &str) -> bool {
    EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS
        .iter()
        .any(|&id| id.eq_ignore_ascii_case(officer_id))
}

/// True when the attacking roster should be destroyed before normal combat resolution.
#[inline]
pub fn evolutionary_assimilation_instant_loss(
    defender_is_npc_hostile: bool,
    defender_hostile_tag_mask: u32,
    _conqueror_borg_beam_suppression: bool,
    attacker_roster_officer_ids: &[String],
) -> bool {
    if !defender_is_npc_hostile {
        return false;
    }
    if defender_hostile_tag_mask & HOSTILE_TAG_MASK_CONQUEROR_BORG == 0 {
        return false;
    }
    attacker_roster_officer_ids
        .iter()
        .any(|id| officer_triggers_evolutionary_assimilation(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_officers_trigger_independently_of_beam_suppression() {
        for officer in EVOLUTIONARY_ASSIMILATION_FORBIDDEN_OFFICER_IDS {
            let roster = vec![officer.to_string()];
            for suppression in [false, true] {
                assert!(evolutionary_assimilation_instant_loss(
                    true,
                    HOSTILE_TAG_MASK_CONQUEROR_BORG,
                    suppression,
                    &roster
                ));
                assert!(!evolutionary_assimilation_instant_loss(
                    false,
                    HOSTILE_TAG_MASK_CONQUEROR_BORG,
                    suppression,
                    &roster
                ));
                assert!(!evolutionary_assimilation_instant_loss(
                    true,
                    0,
                    suppression,
                    &roster
                ));
            }
        }
        assert!(!evolutionary_assimilation_instant_loss(
            true,
            HOSTILE_TAG_MASK_CONQUEROR_BORG,
            true,
            &["charvanek".to_string()]
        ));
    }
}
