//! Reject scenarios whose ship or target does not resolve against the data registry.
//!
//! An unresolved id is **not** inert. When `build_shared_scenario_data_*` cannot find a
//! `ShipRecord` or `HostileRecord` it falls back to a synthetic fight derived from hashing the id
//! strings (`src/optimizer/monte_carlo/scenario.rs`, the branch after the `cached_defender`
//! `if let`): a defender with 260–540 hull and an attacker around 100 attack. Any real profile
//! obliterates that in round one, so a typo does not fail — it returns a confident
//! `win_rate: 1.0`, `r1_kill_rate: 1.0` answer for a fight that never existed.
//!
//! The synthetic path stays (unit tests and the standalone builder rely on it for scenarios with
//! no data files); ingress is where a caller-supplied id has to be real.
//!
//! Out-of-range **tiers** resolve to `None` the same way an unknown id does — `to_ship_record`
//! looks the tier up in the ship's tier table — so they are rejected here too, with the tiers the
//! ship actually has. Out-of-range levels are not an error: they contribute a zero stat bonus
//! rather than failing resolution.

use super::pvp::ScenarioTarget;
use super::requests::ValidationIssue;
use crate::data::data_registry::DataRegistry;

/// Verify the attacker ship and the scenario target resolve. `field` names the request field the
/// ship came from, so PvP defender ships report against their own field.
pub fn validate_scenario_resolves(
    registry: &DataRegistry,
    ship: &str,
    ship_tier: Option<u32>,
    ship_level: Option<u32>,
    target: &ScenarioTarget,
) -> Result<(), Vec<ValidationIssue>> {
    let mut errors = Vec::new();
    if let Some(issue) = ship_issue(registry, "ship", ship, ship_tier, ship_level) {
        errors.push(issue);
    }
    match target {
        ScenarioTarget::Pve { hostile } => {
            if registry.resolve_hostile(hostile.trim()).is_none() {
                errors.push(ValidationIssue {
                    field: "hostile",
                    messages: vec![format!(
                        "unknown hostile {:?}: no hostile with this id, or name and level, exists in the loaded data",
                        hostile.trim()
                    )],
                });
            }
        }
        ScenarioTarget::Pvp {
            defender_ship,
            defender_ship_tier,
            defender_ship_level,
            ..
        } => {
            if let Some(issue) = ship_issue(
                registry,
                "defender_ship",
                defender_ship,
                *defender_ship_tier,
                *defender_ship_level,
            ) {
                errors.push(issue);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn ship_issue(
    registry: &DataRegistry,
    field: &'static str,
    ship: &str,
    tier: Option<u32>,
    level: Option<u32>,
) -> Option<ValidationIssue> {
    let ship = ship.trim();
    if registry
        .resolve_ship_with_tier_level(ship, tier, level)
        .is_some()
    {
        return None;
    }
    // The id may be fine and only the tier out of range; say which, and list the real tiers.
    if let Some((tiers, _, _)) = registry.ship_tiers_levels_and_crew_slots(ship) {
        let requested = tier.unwrap_or(1);
        return Some(ValidationIssue {
            field,
            messages: vec![format!(
                "ship {ship:?} has no tier {requested}: available tiers are {}",
                tiers
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )],
        });
    }
    Some(ValidationIssue {
        field,
        messages: vec![format!(
            "unknown ship {ship:?}: no ship with this id or name exists in the loaded data"
        )],
    })
}

/// Flatten issues into the single-string form the simulate endpoint reports.
pub fn issues_to_message(errors: Vec<ValidationIssue>) -> String {
    errors
        .into_iter()
        .flat_map(|e| e.messages)
        .collect::<Vec<_>>()
        .join("; ")
}
