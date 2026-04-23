//! Map maintainer-curated canonical officer `conditions` tokens to LCARS condition trees.
//!
//! Used by `generate_lcars` and reporting tools. See [`crate::lcars::resolve_lcars_condition`].

use super::LcarsCondition;
use crate::combat::ShipType;

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
        ship_id: None,
        enemy_type: None,
        battle_types: None,
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
        ship_id: None,
        enemy_type: None,
        battle_types: None,
        conditions: Some(vec![inner]),
    }
}

fn lcars_attacker_ship_id_is(ship_id: &str) -> LcarsCondition {
    let mut c = lcars_cond_base("attacker_ship_id_is");
    c.ship_id = Some(ship_id.to_string());
    c
}

fn lcars_or(children: Vec<LcarsCondition>) -> LcarsCondition {
    LcarsCondition {
        condition_type: "or".to_string(),
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
        ship_id: None,
        enemy_type: None,
        battle_types: None,
        conditions: Some(children),
    }
}

/// Canonical `SelfHull*` → Kobayashi `data/ships_extended` ids (see `data/ships_extended/index.json`).
fn map_self_hull_suffix_to_lcars(rest: &str) -> Option<LcarsCondition> {
    match rest {
        "Voyager" => Some(lcars_attacker_ship_id_is("uss_voyager")),
        "Discovery" => Some(lcars_attacker_ship_id_is("uss_discovery")),
        "BorgCube" => Some(lcars_attacker_ship_id_is("borg_cube")),
        "NseaProtector" => Some(lcars_attacker_ship_id_is("nsea_protector")),
        "Amalgam" => Some(lcars_attacker_ship_id_is("amalgam")),
        "Junker" => Some(lcars_attacker_ship_id_is("gs_31")),
        "Franklins" => Some(lcars_or(vec![
            lcars_attacker_ship_id_is("uss_franklin"),
            lcars_attacker_ship_id_is("uss_franklin_a"),
        ])),
        _ => None,
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
        // Kobayashi ship-vs-hostile scenario literals (see `docs/CANONICAL_CONDITIONS.md`).
        "TargetNotASB" | "SelfAttacking" | "TargetNotPlayerStation" => {
            return Some(lcars_cond_base("literal_true"));
        }
        "SelfDefending" => return Some(lcars_cond_base("literal_false")),
        // Canonical opponent category: NPC hostile (ship-vs-hostile optimizer default).
        "EnemyHostile" => return Some(lcars_cond_base("defender_is_npc_hostile")),
        // Canonical opponent category: player ship (PvP-shaped API toggle).
        "EnemyPlayer" => return Some(lcars_cond_base("defender_is_player_ship")),
        // Canonical armada target: modeled as defender combat ship-type Armada (same signal as mitigation / upstream ship_type).
        "EnemyArmada" | "TargetIsArmada" => {
            return Some(lcars_defender_ship_type_is("armada"));
        }
        // “Not solo armada” — group armada engagements only (same tag as [`EnemyGroupArmadas`]).
        "TargetNotSoloArmada" => {
            let mut c = lcars_cond_base("engagement_includes");
            c.enemy_type = Some("group_armadas".to_string());
            return Some(c);
        }
        // Defender is not the Armada ship class (canonical alias used alongside other `Target*` tokens).
        "TargetNotArmada" => {
            return Some(lcars_not(lcars_defender_ship_type_is("armada")));
        }
        // STFC “group armada” / engagement tag; see [`crate::combat::EnemyType::GroupArmadas`].
        "EnemyGroupArmadas" => {
            let mut c = lcars_cond_base("engagement_includes");
            c.enemy_type = Some("group_armadas".to_string());
            return Some(c);
        }
        _ => {}
    }

    if let Some(rest) = t.strip_prefix("SelfHull") {
        if let Some(c) = map_self_hull_suffix_to_lcars(rest) {
            return Some(c);
        }
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

/// Condition tokens that are not handled by [`map_canonical_condition_token`] but are still merged
/// into officer LCARS by `generate_lcars` (typically from the ability `attributes` string).
const OFFICER_LCARS_ATTRIBUTE_MERGED_CONDITION_TOKENS: &[&str] = &[
    "EnemyHullFaction",
    "CombatBattleType",
    "TargetMaxLevel",
    "HullHealthBelowStartOfCombat",
    "HullHealthBelow",
    "HullHealthAbove",
];

/// True when canonical `conditions` need no triage for the officer LCARS pipeline: either
/// [`map_canonical_condition_token`] returns LCARS, or `generate_lcars` merges the token from
/// attributes (e.g. `EnemyHullFaction` + `faction_id=`).
///
/// Maintainer reports should use this (not [`is_canonical_condition_mapped`] alone) so
/// attribute-merged tokens are not listed as unknown.
pub fn is_canonical_officer_condition_resolved(token: &str) -> bool {
    if is_canonical_condition_mapped(token) {
        return true;
    }
    let t = token.trim();
    OFFICER_LCARS_ATTRIBUTE_MERGED_CONDITION_TOKENS
        .iter()
        .any(|&known| t.eq_ignore_ascii_case(known))
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
            ship_id: None,
            enemy_type: None,
            battle_types: None,
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
    fn maps_target_not_solo_armada_to_group_armadas_engagement() {
        let c = map_canonical_condition_token("TargetNotSoloArmada").expect("maps");
        assert_eq!(c.condition_type, "engagement_includes");
        assert_eq!(c.enemy_type.as_deref(), Some("group_armadas"));
        resolve_lcars_condition(&c).expect("resolver accepts");
    }

    #[test]
    fn maps_enemy_group_armadas_to_engagement_includes() {
        let c = map_canonical_condition_token("EnemyGroupArmadas").expect("maps");
        assert_eq!(c.condition_type, "engagement_includes");
        assert_eq!(c.enemy_type.as_deref(), Some("group_armadas"));
        resolve_lcars_condition(&c).expect("resolver accepts");
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
    fn self_hull_voyager_maps_to_attacker_ship_id() {
        let c = map_canonical_condition_token("SelfHullVoyager").expect("maps");
        assert_eq!(c.condition_type, "attacker_ship_id_is");
        assert_eq!(c.ship_id.as_deref(), Some("uss_voyager"));
        let ac = resolve_lcars_condition(&c).expect("resolver accepts");
        assert_eq!(ac, AbilityCondition::AttackerShipIdIs("uss_voyager".into()));
    }

    #[test]
    fn self_hull_franklins_maps_to_or_of_two_ship_ids() {
        let c = map_canonical_condition_token("SelfHullFranklins").expect("maps");
        assert_eq!(c.condition_type, "or");
        let kids = c.conditions.as_ref().expect("or children");
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].condition_type, "attacker_ship_id_is");
        assert_eq!(kids[0].ship_id.as_deref(), Some("uss_franklin"));
        assert_eq!(kids[1].ship_id.as_deref(), Some("uss_franklin_a"));
        let ac = resolve_lcars_condition(&c).expect("resolver accepts combined or");
        match ac {
            AbilityCondition::Or(parts) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(
                    parts[0],
                    AbilityCondition::AttackerShipIdIs("uss_franklin".into())
                );
                assert_eq!(
                    parts[1],
                    AbilityCondition::AttackerShipIdIs("uss_franklin_a".into())
                );
            }
            _ => panic!("expected Or, got {ac:?}"),
        }
    }

    #[test]
    fn enemy_hull_faction_token_not_mapped_without_attributes() {
        assert!(
            map_canonical_condition_token("EnemyHullFaction").is_none(),
            "EnemyHullFaction is merged in generate_lcars from ability attributes, not token-only map"
        );
        assert!(
            is_canonical_officer_condition_resolved("EnemyHullFaction"),
            "officer LCARS pipeline still resolves EnemyHullFaction via attributes merge"
        );
        assert!(is_canonical_officer_condition_resolved(
            " enemyHullFaction "
        ));
    }

    #[test]
    fn scenario_literal_tokens_map_and_resolve() {
        for (tok, expected) in [
            ("TargetNotASB", AbilityCondition::LiteralBool(true)),
            ("SelfAttacking", AbilityCondition::LiteralBool(true)),
            ("TargetNotPlayerStation", AbilityCondition::LiteralBool(true)),
            ("SelfDefending", AbilityCondition::LiteralBool(false)),
        ] {
            let lc = map_canonical_condition_token(tok).expect(tok);
            let ac = resolve_lcars_condition(&lc).expect(tok);
            assert_eq!(ac, expected, "{tok}");
        }
        let raw = vec![
            "TargetNotASB".to_string(),
            "EnemyHostile".to_string(),
        ];
        let out = canonical_conditions_to_lcars(&raw, "x", "y").expect("and");
        assert_eq!(out.condition_type, "and");
    }

    #[test]
    fn hull_health_tokens_are_marked_as_attribute_merged() {
        for tok in [
            "CombatBattleType",
            "TargetMaxLevel",
            "HullHealthBelowStartOfCombat",
            "HullHealthBelow",
            "HullHealthAbove",
        ] {
            assert!(
                map_canonical_condition_token(tok).is_none(),
                "{tok} should be merged from canonical attributes, not token-only map"
            );
            assert!(
                is_canonical_officer_condition_resolved(tok),
                "{tok} should be considered resolved for officer LCARS reports"
            );
        }
    }

    // Task 2 audit: tokens below still lack a 1:1 AbilityCondition / CombatContext story (see
    // docs/CANONICAL_CONDITIONS.md). When engine support exists, map in map_canonical_condition_token
    // and remove the token from DEFERRED.
    #[test]
    fn task2_deferred_tokens_remain_unmapped() {
        const DEFERRED: &[&str] = &[
            "EnemySentinel",
            "ModuleKinetic",
            "CombatGameContext",
            "ModuleEnergy",
            "SelfAtSoloArmada",
            "SelfAtStation",
            "TargetStateAny",
            "HullHealthBelowStartOfCombat",
            "HullHealthBelow",
            "SelfAtWaveDefenseChallenge",
            "TargetIsArmadaOrInvadingEntity",
            "SelfCloaked",
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
