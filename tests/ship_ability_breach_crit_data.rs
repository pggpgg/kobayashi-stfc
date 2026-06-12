//! Data-driven coverage: the breach-gated cumulative crit hull abilities
//! (Hegh'ta "Open the Wound" / Rotarran "Bird of Prey") resolve out of the real
//! `data/ships_extended` records — i.e. the catalog mapping + normalize step are wired,
//! not just the engine. These were previously catalogued as `combat_noop`; see
//! docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md §6.2.

use kobayashi::combat::AbilityEffect;
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

/// Resolve a ship by name at max tier/level and return the effect for one ability id.
fn effect_for(ship: &str, ability_id: &str) -> AbilityEffect {
    let record = resolve_ship_with_tier_level(ship, Some(9), Some(90))
        .unwrap_or_else(|| panic!("ship {ship} should resolve from data/ships_extended"));
    let ability = record
        .abilities
        .iter()
        .flatten()
        .find(|a| a.id == ability_id)
        .unwrap_or_else(|| panic!("ship {ship} should carry ability {ability_id}"));
    ship_ability_to_crew_seat_context(ability)
        .unwrap_or_else(|| panic!("ability {ability_id} should resolve to a seat context"))
        .ability
        .effect
}

#[test]
fn rotarran_bird_of_prey_resolves_to_cumulative_crit_damage() {
    match effect_for("rotarran", "2195955652") {
        AbilityEffect::BreachCumulativeCritDamagePerCrit(v) => {
            assert!(
                v > 0.0,
                "Rotarran per-crit crit-damage growth should be positive, got {v}"
            );
        }
        other => panic!("expected BreachCumulativeCritDamagePerCrit, got {other:?}"),
    }
}

#[test]
fn hegh_ta_open_the_wound_resolves_to_cumulative_crit_chance() {
    match effect_for("hegh_ta", "3432906971") {
        AbilityEffect::BreachCumulativeCritChancePerHit(v) => {
            assert!(
                v > 0.0,
                "Hegh'ta per-hit crit-chance growth should be positive, got {v}"
            );
        }
        other => panic!("expected BreachCumulativeCritChancePerHit, got {other:?}"),
    }
}
