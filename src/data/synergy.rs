//! Native officer synergy for captain maneuvers, distinct from officer rank scaling.
//!
//! Build 274 FleetPlayerData.get_LeftSynergyLevel (181a144f0) and
//! get_RightSynergyLevel (181a14570) select indices 0/1/2. SynergyBuffData's
//! CalculateValueAndChance (181986c90) adds the indexed entries to entry zero.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficerSynergyIdentity {
    pub group_id: u64,
    pub officer_class: u32,
}

/// Seat order is significant when two bridge officers have the same class.
/// Missing seats and a duplicate occupied officer are filtered by the caller.
pub fn captain_synergy_indices(
    captain: OfficerSynergyIdentity,
    bridge: [Option<OfficerSynergyIdentity>; 2],
) -> [usize; 2] {
    let index = |officer: Option<OfficerSynergyIdentity>| match officer {
        Some(o) if o.group_id == captain.group_id => {
            if o.officer_class == captain.officer_class {
                1
            } else {
                2
            }
        }
        _ => 0,
    };
    let left = index(bridge[0]);
    let mut right = index(bridge[1]);
    if right == 2
        && bridge[0].is_some_and(|first| {
            first.group_id == captain.group_id
                && bridge[1].is_some_and(|second| first.officer_class == second.officer_class)
        })
    {
        right = 1;
    }
    [left, right]
}

#[derive(Debug, Clone)]
pub struct SynergyTag {
    pub mechanism: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_captain_synergy_preserves_seat_group_and_class_rules() {
        let o = |group_id, officer_class| OfficerSynergyIdentity {
            group_id,
            officer_class,
        };
        let captain = o(7, 1);
        assert_eq!(captain_synergy_indices(captain, [None, None]), [0, 0]);
        assert_eq!(
            captain_synergy_indices(captain, [Some(o(8, 2)), Some(o(8, 3))]),
            [0, 0]
        );
        assert_eq!(
            captain_synergy_indices(captain, [Some(o(7, 1)), Some(o(7, 1))]),
            [1, 1]
        );
        assert_eq!(
            captain_synergy_indices(captain, [Some(o(7, 2)), Some(o(7, 3))]),
            [2, 2]
        );
        assert_eq!(
            captain_synergy_indices(captain, [Some(o(7, 2)), Some(o(7, 2))]),
            [2, 1]
        );
        // A same-class first officer in another group does not suppress right.
        assert_eq!(
            captain_synergy_indices(captain, [Some(o(8, 2)), Some(o(7, 2))]),
            [0, 2]
        );
    }
}
