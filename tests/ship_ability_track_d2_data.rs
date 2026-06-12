//! Data-driven guard for the Track D2 hostile-debuff hull abilities (Quv'Sompek, Sanctus, B'Rel,
//! U.S.S. Intrepid). These resolve out of the real `data/ships_extended` records, so this catches
//! two failure modes that previously left them dormant (see SHIP_ABILITY_COMBAT_NOOP_AUDIT.md §6.1):
//!   1. `ships_extended` not regenerated after a catalog change (effect stays `combat_noop`).
//!   2. `value_is_percentage` mis-set so an already-fractional value is scaled ×0.01 (100× too small).
//!
//! The lower bound on the resolved value guards specifically against (2).

use kobayashi::combat::AbilityEffect;
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::ship_ability_resolve::ship_ability_to_crew_seat_context;

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

/// A resolved debuff/bonus fraction should be a meaningful percentage (>= 5%), not the ~0.1%
/// that a double-applied `value_is_percentage` would produce.
fn assert_meaningful(label: &str, v: f64) {
    assert!(
        v >= 0.05,
        "{label} resolved to {v}; expected a meaningful fraction (>= 0.05). \
         A value near 0.001 means value_is_percentage double-scaled an already-fractional row."
    );
}

#[test]
fn quv_sompek_counter_stat_debuff_is_meaningful() {
    match effect_for("quv_sompek", "701705952") {
        AbilityEffect::HostileCounterStatDebuff {
            reduction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 5);
            assert_meaningful("Quv'Sompek counter debuff", reduction);
        }
        other => panic!("expected HostileCounterStatDebuff, got {other:?}"),
    }
}

#[test]
fn sanctus_shield_drain_is_meaningful() {
    match effect_for("sanctus", "1379978713") {
        AbilityEffect::DefenderShieldDrainPerRound {
            fraction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 5);
            assert_meaningful("Sanctus shield drain", fraction);
        }
        other => panic!("expected DefenderShieldDrainPerRound, got {other:?}"),
    }
}

#[test]
fn b_rel_counter_stat_debuff_is_meaningful() {
    match effect_for("b_rel", "2441576367") {
        AbilityEffect::HostileCounterStatDebuff {
            reduction,
            duration_rounds,
        } => {
            assert_eq!(duration_rounds, 1);
            assert_meaningful("B'Rel counter debuff", reduction);
        }
        other => panic!("expected HostileCounterStatDebuff, got {other:?}"),
    }
}

#[test]
fn intrepid_engagement_defensive_is_meaningful() {
    match effect_for("uss_intrepid", "1463338054") {
        AbilityEffect::HostileEngagementDefensiveBonus(v) => {
            assert_meaningful("Intrepid engagement defensive", v);
        }
        other => panic!("expected HostileEngagementDefensiveBonus, got {other:?}"),
    }
}
