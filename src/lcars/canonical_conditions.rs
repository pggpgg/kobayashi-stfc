//! Map maintainer-curated canonical officer `conditions` tokens to LCARS condition trees.
//!
//! Used by `generate_lcars` and reporting tools. See [`crate::lcars::map_canonical_condition_token`].

use super::LcarsCondition;
use crate::combat::ShipType;

fn lcars_cond_base(ty: impl Into<String>) -> LcarsCondition {
    LcarsCondition {
        weapon_scope: Default::default(),
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

/// LCARS `not` with exactly one child (see [`crate::lcars::lcars_condition_to_spec`]).
fn lcars_not(inner: LcarsCondition) -> LcarsCondition {
    LcarsCondition {
        weapon_scope: Default::default(),
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
        weapon_scope: Default::default(),
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

/// STFC “opponent has any state” bundle: burning / hull breach / assimilated on the defender.
/// Does **not** include defender morale (engine exposes morale only on the attacker as `morale_active`).
fn lcars_target_state_any() -> LcarsCondition {
    lcars_or(vec![
        lcars_cond_base("defender_burning"),
        lcars_cond_base("defender_hull_breach"),
        lcars_cond_base("defender_assimilated"),
    ])
}

/// Approximation for “no debuff state on self”: neither attacker burning nor attacker hull breach.
fn lcars_self_state_none_attacker_debuffs() -> LcarsCondition {
    lcars_not(lcars_or(vec![
        lcars_cond_base("attacker_burning"),
        lcars_cond_base("attacker_hull_breach"),
    ]))
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
/// See [`crate::lcars::lcars_condition_to_spec`].
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
        // Default ship-vs-hostile / PvP attack path assumes the player is not defending.
        // Omit from LCARS `and` (do not emit literal_false — that arm never evaluates true).
        "SelfDefending" => return None,
        // Solo armada — matches [`crate::combat::EnemyType::SoloArmadas`] when hostile sets `engagement_enemy_types`.
        "SelfAtSoloArmada" => {
            let mut c = lcars_cond_base("engagement_includes");
            c.enemy_type = Some("solo_armadas".to_string());
            return Some(c);
        }
        // Station / sentinel / overworld encounter tokens: not modeled on default ship-vs-hostile path.
        "EnemySentinel"
        | "CombatGameContext"
        | "SelfAtStation"
        | "SelfAtWaveDefenseChallenge"
        | "SelfAtAssault2"
        | "TargetIsInvadingEntity" => {
            return Some(lcars_cond_base("literal_false"));
        }
        // Invading half is not a distinct signal yet; armada fights gate on defender hull class Armada.
        "TargetIsArmadaOrInvadingEntity" => {
            return Some(lcars_defender_ship_type_is("armada"));
        }
        "TargetNotInvadingEntity" => return Some(lcars_cond_base("literal_true")),
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
        // Weapon-type gates: compile-time scope extracted into `Ability.weapon_scope`
        // (condition-tree evaluation stays `true`; the engine applies the effect only to
        // weapons of the matching type — untyped weapons match leniently).
        "ModuleKinetic" => {
            let mut c = lcars_cond_base("attacker_weapon_scope");
            c.weapon_scope = Some("kinetic".to_string());
            return Some(c);
        }
        "ModuleEnergy" => {
            let mut c = lcars_cond_base("attacker_weapon_scope");
            c.weapon_scope = Some("energy".to_string());
            return Some(c);
        }
        "TargetStateAny" => return Some(lcars_target_state_any()),
        "SelfStateNone" => return Some(lcars_self_state_none_attacker_debuffs()),
        "SelfCloaked" | "SelfMining" => return Some(lcars_cond_base("literal_false")),
        "CargoEmpty" | "EnemyNotToaTrialHostile" => return Some(lcars_cond_base("literal_true")),
        "CargoFull" | "EnemyStronger" | "HitEnemyWithEnergy" | "HitEnemyWithKinetic" => {
            return Some(lcars_cond_base("literal_false"));
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
                tracing::warn!(
                    token = %tok,
                    officer = %officer_name,
                    ability = %ability_label,
                    "skipping unmapped canonical condition"
                );
            }
        }
    }
    match mapped.len() {
        0 => None,
        1 => Some(mapped.pop().expect("len checked")),
        _ => Some(LcarsCondition {
            weapon_scope: Default::default(),
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
    use crate::combat::effect_spec_compile::compile_condition;
    use crate::combat::{AbilityCondition, EnemyType, ShipType};
    use crate::lcars::lcars_condition_to_spec;

    #[test]
    fn maps_enemy_explorer_to_defender_ship_type() {
        let c = map_canonical_condition_token("EnemyExplorer").expect("maps");
        assert_eq!(c.condition_type, "defender_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("explorer"));
        lcars_condition_to_spec(&c).expect("spec adapter accepts");
    }

    #[test]
    fn maps_self_interceptor_to_attacker_ship_type() {
        let c = map_canonical_condition_token("SelfInterceptor").expect("maps");
        assert_eq!(c.condition_type, "attacker_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("interceptor"));
        lcars_condition_to_spec(&c).unwrap();
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
        lcars_condition_to_spec(&out).expect("spec adapter accepts combined and");
    }

    #[test]
    fn maps_enemy_armada_to_defender_ship_type_armada() {
        let c = map_canonical_condition_token("EnemyArmada").expect("maps");
        assert_eq!(c.condition_type, "defender_ship_type_is");
        assert_eq!(c.ship_type.as_deref(), Some("armada"));
        lcars_condition_to_spec(&c).expect("spec adapter accepts");
    }

    #[test]
    fn maps_target_is_armada_same_as_enemy_armada() {
        let a = map_canonical_condition_token("EnemyArmada").expect("maps");
        let b = map_canonical_condition_token("TargetIsArmada").expect("maps");
        assert_eq!(a.condition_type, b.condition_type);
        assert_eq!(a.ship_type, b.ship_type);
        lcars_condition_to_spec(&b).expect("spec adapter accepts");
    }

    #[test]
    fn maps_target_not_solo_armada_to_group_armadas_engagement() {
        let c = map_canonical_condition_token("TargetNotSoloArmada").expect("maps");
        assert_eq!(c.condition_type, "engagement_includes");
        assert_eq!(c.enemy_type.as_deref(), Some("group_armadas"));
        lcars_condition_to_spec(&c).expect("spec adapter accepts");
    }

    #[test]
    fn maps_enemy_group_armadas_to_engagement_includes() {
        let c = map_canonical_condition_token("EnemyGroupArmadas").expect("maps");
        assert_eq!(c.condition_type, "engagement_includes");
        assert_eq!(c.enemy_type.as_deref(), Some("group_armadas"));
        lcars_condition_to_spec(&c).expect("spec adapter accepts");
    }

    #[test]
    fn maps_target_not_armada_to_not_defender_armada() {
        let c = map_canonical_condition_token("TargetNotArmada").expect("maps");
        assert_eq!(c.condition_type, "not");
        let inner = c.conditions.as_ref().expect("inner");
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].condition_type, "defender_ship_type_is");
        assert_eq!(inner[0].ship_type.as_deref(), Some("armada"));
        let spec = lcars_condition_to_spec(&c).expect("spec adapter resolves");
        let ac = compile_condition(&spec).expect("compile");
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
        lcars_condition_to_spec(&out).unwrap();
    }

    #[test]
    fn target_burning_maps() {
        let c = map_canonical_condition_token("TargetHasBurning").unwrap();
        assert_eq!(c.condition_type, "defender_burning");
        lcars_condition_to_spec(&c).unwrap();
    }

    #[test]
    fn target_has_assimilated_maps_to_defender_assimilated() {
        let c = map_canonical_condition_token("TargetHasAssimilated").unwrap();
        assert_eq!(c.condition_type, "defender_assimilated");
        let spec = lcars_condition_to_spec(&c).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(ac, AbilityCondition::DefenderAssimilated);
    }

    #[test]
    fn self_has_morale_not_swallowed_by_self_hull_prefix() {
        let c = map_canonical_condition_token("SelfHasMorale").unwrap();
        assert_eq!(c.condition_type, "morale_active");
        lcars_condition_to_spec(&c).unwrap();
    }

    #[test]
    fn self_has_hull_breach_maps_to_attacker_hull_breach() {
        let c = map_canonical_condition_token("SelfHasHullBreach").unwrap();
        assert_eq!(c.condition_type, "attacker_hull_breach");
        let spec = lcars_condition_to_spec(&c).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerHullBreach);
    }

    #[test]
    fn self_has_burning_maps_to_attacker_burning() {
        let c = map_canonical_condition_token("SelfHasBurning").unwrap();
        assert_eq!(c.condition_type, "attacker_burning");
        let spec = lcars_condition_to_spec(&c).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerBurning);
    }

    #[test]
    fn maps_self_officer_tal_not_on_bridge() {
        let c = map_canonical_condition_token("SelfOfficerTalNotOnBridge").unwrap();
        assert_eq!(c.condition_type, "attacker_officer_tal_not_on_bridge");
        let spec = lcars_condition_to_spec(&c).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(ac, AbilityCondition::AttackerOfficerTalNotOnBridge);
    }

    #[test]
    fn self_hull_voyager_maps_to_attacker_ship_id() {
        let c = map_canonical_condition_token("SelfHullVoyager").expect("maps");
        assert_eq!(c.condition_type, "attacker_ship_id_is");
        assert_eq!(c.ship_id.as_deref(), Some("uss_voyager"));
        let spec = lcars_condition_to_spec(&c).expect("spec adapter accepts");
        let ac = compile_condition(&spec).expect("compile");
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
        let spec = lcars_condition_to_spec(&c).expect("spec adapter accepts combined or");
        let ac = compile_condition(&spec).expect("compile");
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
        assert!(map_canonical_condition_token("SelfDefending").is_none());
        for (tok, expected) in [
            ("TargetNotASB", AbilityCondition::LiteralBool(true)),
            ("SelfAttacking", AbilityCondition::LiteralBool(true)),
            (
                "TargetNotPlayerStation",
                AbilityCondition::LiteralBool(true),
            ),
            ("EnemySentinel", AbilityCondition::LiteralBool(false)),
            ("CombatGameContext", AbilityCondition::LiteralBool(false)),
            (
                "TargetNotInvadingEntity",
                AbilityCondition::LiteralBool(true),
            ),
        ] {
            let lc = map_canonical_condition_token(tok).expect(tok);
            let spec = lcars_condition_to_spec(&lc).expect(tok);
            let ac = compile_condition(&spec).expect(tok);
            assert_eq!(ac, expected, "{tok}");
        }
        let raw = vec!["TargetNotASB".to_string(), "EnemyHostile".to_string()];
        let out = canonical_conditions_to_lcars(&raw, "x", "y").expect("and");
        assert_eq!(out.condition_type, "and");
    }

    #[test]
    fn self_at_solo_armada_maps_to_engagement_includes() {
        let lc = map_canonical_condition_token("SelfAtSoloArmada").expect("maps");
        assert_eq!(lc.condition_type, "engagement_includes");
        assert_eq!(lc.enemy_type.as_deref(), Some("solo_armadas"));
        let spec = lcars_condition_to_spec(&lc).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(
            ac,
            AbilityCondition::EngagementIncludes(EnemyType::SoloArmadas)
        );
    }

    #[test]
    fn target_is_armada_or_invading_entity_maps_to_defender_armada_class() {
        let lc = map_canonical_condition_token("TargetIsArmadaOrInvadingEntity").expect("maps");
        assert_eq!(lc.condition_type, "defender_ship_type_is");
        assert_eq!(lc.ship_type.as_deref(), Some("armada"));
        let spec = lcars_condition_to_spec(&lc).unwrap();
        let ac = compile_condition(&spec).unwrap();
        assert_eq!(ac, AbilityCondition::DefenderShipTypeIs(ShipType::Armada));
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

    // Task 2 audit: tokens merged from canonical `attributes` only — must stay absent from
    // `map_canonical_condition_token` (see docs/CANONICAL_CONDITIONS.md).
    #[test]
    fn task2_attribute_merged_hull_health_tokens_remain_token_only_unmapped() {
        const ATTR_ONLY: &[&str] = &[
            "HullHealthBelowStartOfCombat",
            "HullHealthBelow",
            "HullHealthAbove",
        ];

        for tok in ATTR_ONLY {
            assert!(
                map_canonical_condition_token(tok).is_none(),
                "attribute-merged token {tok:?} must not gain a duplicate token-only LCARS map"
            );
        }
    }

    #[test]
    fn target_state_any_maps_to_or_of_defender_states() {
        let lc = map_canonical_condition_token("TargetStateAny").expect("maps");
        assert_eq!(lc.condition_type, "or");
        let ch = lc.conditions.as_ref().expect("children");
        assert_eq!(ch.len(), 3);
        let spec = lcars_condition_to_spec(&lc).expect("spec adapter resolve");
        let ac = compile_condition(&spec).expect("compile");
        assert_eq!(
            ac,
            AbilityCondition::Or(vec![
                AbilityCondition::DefenderBurning,
                AbilityCondition::DefenderHullBreach,
                AbilityCondition::DefenderAssimilated,
            ])
        );
    }

    #[test]
    fn self_state_none_maps_to_not_or_attacker_debuffs() {
        let lc = map_canonical_condition_token("SelfStateNone").expect("maps");
        assert_eq!(lc.condition_type, "not");
        let inner = lc.conditions.as_ref().expect("not child")[0].clone();
        assert_eq!(inner.condition_type, "or");
        let spec = lcars_condition_to_spec(&lc).expect("spec adapter resolve");
        let ac = compile_condition(&spec).expect("compile");
        assert_eq!(
            ac,
            AbilityCondition::Not(Box::new(AbilityCondition::Or(vec![
                AbilityCondition::AttackerBurning,
                AbilityCondition::AttackerHullBreach,
            ])))
        );
    }
}
