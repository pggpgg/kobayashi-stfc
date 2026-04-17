//! Map maintainer-curated canonical officer `conditions` tokens to LCARS condition trees.
//!
//! Used by `generate_lcars` and reporting tools. See [`crate::lcars::resolve_lcars_condition`].

use crate::combat::ShipType;
use super::LcarsCondition;

fn lcars_cond_base(ty: impl Into<String>) -> LcarsCondition {
    LcarsCondition {
        condition_type: ty.into(),
        stat: None,
        threshold_pct: None,
        min: None,
        max: None,
        faction: None,
        group: None,
        min_members: None,
        tag: None,
        ship_type: None,
        faction_id: None,
        conditions: None,
    }
}

fn lcars_defender_ship_type_is(slug: &str) -> LcarsCondition {
    let mut c = lcars_cond_base("defender_ship_type_is");
    c.ship_type = Some(slug.to_string());
    c
}

/// LCARS `not` with exactly one child (see [`crate::lcars::resolve_lcars_condition`]).
fn lcars_not(inner: LcarsCondition) -> LcarsCondition {
    LcarsCondition {
        condition_type: "not".to_string(),
        stat: None,
        threshold_pct: None,
        min: None,
        max: None,
        faction: None,
        group: None,
        min_members: None,
        tag: None,
        ship_type: None,
        faction_id: None,
        conditions: Some(vec![inner]),
    }
}

/// Maps one RawOfficers / canonical condition token to LCARS when the resolver already supports it.
/// See [`crate::lcars::resolve_lcars_condition`].
pub fn map_canonical_condition_token(token: &str) -> Option<LcarsCondition> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }

    match t {
        "TargetHasBurning" => return Some(lcars_cond_base("defender_burning")),
        "TargetHasAssimilated" => return Some(lcars_cond_base("defender_assimilated")),
        "TargetHasHullBreach" => return Some(lcars_cond_base("defender_hull_breach")),
        "SelfHasMorale" => return Some(lcars_cond_base("morale_active")),
        // Player ship debuffs (from hostile procs / counter); must be before `Self` hull-class prefix.
        "SelfHasHullBreach" => return Some(lcars_cond_base("attacker_hull_breach")),
        "SelfHasBurning" => return Some(lcars_cond_base("attacker_burning")),
        "SelfOfficerTalNotOnBridge" => {
            return Some(lcars_cond_base("attacker_officer_tal_not_on_bridge"));
        }
        // Canonical opponent category: NPC hostile (ship-vs-hostile optimizer default).
        "EnemyHostile" => return Some(lcars_cond_base("defender_is_npc_hostile")),
        // Canonical opponent category: player ship (PvP-shaped API toggle).
        "EnemyPlayer" => return Some(lcars_cond_base("defender_is_player_ship")),
        // Canonical armada target: modeled as defender combat ship-type Armada (same signal as mitigation / upstream ship_type).
        "EnemyArmada" | "TargetIsArmada" => {
            return Some(lcars_defender_ship_type_is("armada"));
        }
        // Defender is not the Armada ship class (canonical alias used alongside other `Target*` tokens).
        "TargetNotArmada" => {
            return Some(lcars_not(lcars_defender_ship_type_is("armada")));
        }
        _ => {}
    }

    if let Some(rest) = t.strip_prefix("Enemy") {
        let slug = match rest {
            "Explorer" => "explorer",
            "Battleship" => "battleship",
            "Interceptor" => "interceptor",
            "Survey" | "Surveyor" => "survey",
            "Armada" => "armada",
            _ => return None,
        };
        ShipType::from_data_slug(slug)?;
        let mut c = lcars_cond_base("defender_ship_type_is");
        c.ship_type = Some(slug.to_string());
        return Some(c);
    }

    if let Some(rest) = t.strip_prefix("Self") {
        let slug = match rest {
            "Explorer" => "explorer",
            "Battleship" => "battleship",
            "Interceptor" => "interceptor",
            "Surveyor" | "Survey" => "survey",
            "Armada" => "armada",
            _ => return None,
        };
        ShipType::from_data_slug(slug)?;
        let mut c = lcars_cond_base("attacker_ship_type_is");
        c.ship_type = Some(slug.to_string());
        return Some(c);
    }

    None
}

/// True when [`map_canonical_condition_token`] returns an LCARS mapping for this token (after trim).
pub fn is_canonical_condition_mapped(token: &str) -> bool {
    map_canonical_condition_token(token).is_some()
}

/// Converts canonical `conditions` to a single LCARS condition (`and` when multiple tokens map).
/// Logs unmapped tokens to stderr. Returns `None` if nothing maps (or the list is empty).
pub fn canonical_conditions_to_lcars(
    conditions: &[String],
    officer_name: &str,
    ability_label: &str,
) -> Option<LcarsCondition> {
    if conditions.is_empty() {
        return None;
    }
    let mut mapped = Vec::new();
    for raw in conditions {
        if let Some(c) = map_canonical_condition_token(raw) {
            mapped.push(c);
        } else {
            let tok = raw.trim();
            if !tok.is_empty() {
                eprintln!(
                    "generate_lcars: skipping unmapped canonical condition {tok:?} \
                     (officer {officer_name:?}, ability {ability_label:?})"
                );
            }
        }
    }
    match mapped.len() {
        0 => None,
        1 => Some(mapped.pop().expect("len checked")),
        _ => Some(LcarsCondition {
            condition_type: "and".to_string(),
            stat: None,
            threshold_pct: None,
            min: None,
            max: None,
            faction: None,
            group: None,
            min_members: None,
            tag: None,
            ship_type: None,
            faction_id: None,
            conditions: Some(mapped),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{AbilityCondition, ShipType};
    use crate::lcars::resolve_lcars_condition;

    #[test]
    fn maps_enemy_explorer_to_defender_ship_type() {
        let c = map_canonical_condition_token("EnemyExplorer").expect("maps");
        assert_eq!(c.condition_type, "defender_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("explorer"));
        resolve_lcars_condition(&c).expect("resolver accepts");
    }

    #[test]
    fn maps_self_interceptor_to_attacker_ship_type() {
        let c = map_canonical_condition_token("SelfInterceptor").expect("maps");
        assert_eq!(c.condition_type, "attacker_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("interceptor"));
        resolve_lcars_condition(&c).unwrap();
    }

    #[test]
    fn mixed_tokens_map_enemy_hostile_not_armada_and_explorer_to_and() {
        let raw = vec![
            "EnemyHostile".to_string(),
            " TargetNotArmada".to_string(),
            "EnemyExplorer".to_string(),
            "SelfOfficerTalNotOnBridge".to_string(),
        ];
        let out = canonical_conditions_to_lcars(&raw, "Alok", "test").expect("maps");
        assert_eq!(out.condition_type, "and");
        let kids = out.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 4);
        assert_eq!(kids[0].condition_type, "defender_is_npc_hostile");
        assert_eq!(kids[1].condition_type, "not");
        let not_inner = kids[1].conditions.as_ref().expect("not inner");
        assert_eq!(not_inner.len(), 1);
        assert_eq!(not_inner[0].condition_type, "defender_ship_type_is");
        assert_eq!(not_inner[0].ship_type.as_deref(), Some("armada"));
        assert_eq!(kids[2].condition_type, "defender_ship_type_is");
        assert_eq!(kids[2].ship_type.as_deref(), Some("explorer"));
        assert_eq!(kids[3].condition_type, "attacker_officer_tal_not_on_bridge");
        resolve_lcars_condition(&out).expect("resolver accepts combined and");
    }

    #[test]
    fn maps_enemy_armada_to_defender_ship_type_armada() {
        let c = map_canonical_condition_token("EnemyArmada").expect("maps");
        assert_eq!(c.condition_type, "defender_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("armada"));
        resolve_lcars_condition(&c).expect("resolver accepts");
    }

    #[test]
    fn maps_target_is_armada_same_as_enemy_armada() {
        let a = map_canonical_condition_token("EnemyArmada").expect("maps");
        let b = map_canonical_condition_token("TargetIsArmada").expect("maps");
        assert_eq!(a.condition_type, b.condition_type);
        assert_eq!(a.ship_type, b.ship_type);
        resolve_lcars_condition(&b).expect("resolver accepts");
    }

    #[test]
    fn maps_target_not_armada_to_not_defender_armada() {
        let c = map_canonical_condition_token("TargetNotArmada").expect("maps");
        assert_eq!(c.condition_type, "not");
        let inner = c.conditions.as_ref().expect("inner");
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].condition_type, "defender_ship_type_is");
        assert_eq!(inner[0].ship_type.as_deref(), Some("armada"));
        let ac = resolve_lcars_condition(&c).expect("resolves");
        assert_eq!(
            ac,
            AbilityCondition::Not(Box::new(AbilityCondition::DefenderShipTypeIs(
                ShipType::Armada
            )))
        );
    }

    #[test]
    fn two_mapped_tokens_become_and() {
        let raw = vec!["EnemyExplorer".to_string(), "SelfBattleship".to_string()];
        let out = canonical_conditions_to_lcars(&raw, "x", "y").expect("and");
        assert_eq!(out.condition_type, "and");
        let kids = out.conditions.as_ref().expect("children");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "defender_ship_type_is");
        assert_eq!(kids[1].condition_type, "attacker_ship_type_is");
        resolve_lcars_condition(&out).unwrap();
    }

    #[test]
    fn target_burning_maps() {
        let c = map_canonical_condition_token("TargetHasBurning").unwrap();
        assert_eq!(c.condition_type, "defender_burning");
        resolve_lcars_condition(&c).unwrap();
    }

    #[test]
    fn target_has_assimilated_maps_to_defender_assimilated() {
        let c = map_canonical_condition_token("TargetHasAssimilated").unwrap();
        assert_eq!(c.condition_type, "defender_assimilated");
        let ac = resolve_lcars_condition(&c).unwrap();
        assert_eq!(ac, AbilityCondition::DefenderAssimilated);
    }

    #[test]
    fn self_has_morale_not_swallowed_by_self_hull_prefix() {
        let c = map_canonical_condition_token("SelfHasMorale").unwrap();
        assert_eq!(c.condition_type, "morale_active");
        resolve_lcars_condition(&c).unwrap();
    }

    #[test]
    fn self_has_hull_breach_maps_to_attacker_hull_breach() {
        let c = map_canonical_condition_token("SelfHasHullBreach").unwrap();
        assert_eq!(c.condition_type, "attacker_hull_breach");
        let ac = resolve_lcars_condition(&c).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerHullBreach);
    }

    #[test]
    fn self_has_burning_maps_to_attacker_burning() {
        let c = map_canonical_condition_token("SelfHasBurning").unwrap();
        assert_eq!(c.condition_type, "attacker_burning");
        let ac = resolve_lcars_condition(&c).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerBurning);
    }

    #[test]
    fn maps_self_officer_tal_not_on_bridge() {
        let c = map_canonical_condition_token("SelfOfficerTalNotOnBridge").unwrap();
        assert_eq!(c.condition_type, "attacker_officer_tal_not_on_bridge");
        let ac = resolve_lcars_condition(&c).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerOfficerTalNotOnBridge);
    }

    #[test]
    fn enemy_hull_faction_token_not_mapped_without_attributes() {
        assert!(
            map_canonical_condition_token("EnemyHullFaction").is_none(),
            "EnemyHullFaction is merged in generate_lcars from ability attributes, not token-only map"
        );
    }

    // Task 2 audit: tokens below still lack a 1:1 AbilityCondition / CombatContext story (see
    // docs/CANONICAL_CONDITIONS.md). When engine support exists, map in map_canonical_condition_token
    // and remove the token from DEFERRED.
    #[test]
    fn task2_deferred_tokens_remain_unmapped() {
        const DEFERRED: &[&str] = &[
            "TargetNotASB",
            "TargetMaxLevel",
            "SelfDefending",
            "CombatBattleType",
            "EnemySentinel",
            "ModuleKinetic",
            "CombatGameContext",
            "ModuleEnergy",
            "SelfAtSoloArmada",
            "SelfAtStation",
            "TargetStateAny",
            "SelfAttacking",
            "HullHealthBelowStartOfCombat",
            "HullHealthBelow",
            "SelfAtWaveDefenseChallenge",
            "TargetIsArmadaOrInvadingEntity",
            "TargetNotPlayerStation",
            "SelfCloaked",
            "SelfHullBorgCube",
            "SelfHullDiscovery",
            "SelfHullFranklins",
            "SelfHullNseaProtector",
            "SelfHullVoyager",
            "SelfStateNone",
            "TargetIsInvadingEntity",
            "CargoEmpty",
            "CargoFull",
            "EnemyNotToaTrialHostile",
            "EnemyStronger",
            "HitEnemyWithEnergy",
            "HitEnemyWithKinetic",
            "HullHealthAbove",
            "SelfAtAssault2",
            "SelfHullAmalgam",
            "SelfHullJunker",
            "SelfMining",
            "TargetNotInvadingEntity",
        ];

        for tok in DEFERRED {
            assert!(
                map_canonical_condition_token(tok).is_none(),
                "deferred token {tok:?} unexpectedly maps; remove from DEFERRED or document new engine semantics"
            );
        }
    }
}
