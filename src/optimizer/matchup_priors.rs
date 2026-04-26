//! Matchup-aware tie-breakers for analytical prefilter ranking (see docs/DESIGN.md §6.2).
//!
//! Closed-form [`crate::optimizer::analytical::expected_damage`] ignores conditional abilities; we add a
//! small **prior** so crews with gates that match this fight (hull class, faction, PvE/PvP, Tal) and
//! crews overlapping UI warm-start or persisted optimize-history reference crews sort ahead when
//! truncating before Monte Carlo. Catalog-backed synergy bumps are deferred (see `src/data/synergy.rs`).

use std::collections::{HashMap, HashSet};

use crate::combat::{
    attacker_crew_tal_assigned_captain_or_bridge, AbilityCondition, CrewConfiguration, EnemyType,
    OpponentFactionTag,
};
use crate::data::upstream_hostile_ship_type::upstream_hostile_ship_type_profile;
use crate::optimizer::analytical::expected_damage;
use crate::optimizer::constraints::normalize_officer_name;
use crate::optimizer::crew_generator::{CrewCandidate, BRIDGE_SLOTS};
use crate::optimizer::monte_carlo::scenario::{
    CombatSimulationInput, DefenderOpponent, SharedScenarioData,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticGate {
    Pass,
    Fail,
    Unknown,
}

/// Hull-class / faction / PvE / ship-id / Tal gates only; morale and stateful arms stay [`StaticGate::Unknown`].
fn eval_static_gate(
    cond: &AbilityCondition,
    shared: &SharedScenarioData,
    crew: &CrewConfiguration,
) -> StaticGate {
    match cond {
        AbilityCondition::DefenderShipTypeIs(st) => {
            if *st == shared.defender_ship_type_for_combat() {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::DefenderFactionIs(expected) => {
            let actual = shared
                .hostile_rec
                .as_ref()
                .map(|h| h.opponent_faction_tag())
                .unwrap_or(OpponentFactionTag::Unknown);
            if actual == OpponentFactionTag::Unknown {
                StaticGate::Unknown
            } else if *expected == actual {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::DefenderHullFactionIdIs(id) => {
            let actual = shared
                .hostile_rec
                .as_ref()
                .and_then(|h| h.faction.as_ref().map(|f| f.id))
                .unwrap_or(0);
            if actual == 0 {
                StaticGate::Unknown
            } else if *id == actual {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::AttackerShipTypeIs(st) => {
            if *st == shared.attacker_ship_type_for_combat() {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::AttackerShipIdIs(id) => {
            if shared.ship == *id {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::DefenderIsNpcHostile => match shared.defender_opponent {
            DefenderOpponent::Hostile => StaticGate::Pass,
            DefenderOpponent::Player => StaticGate::Fail,
        },
        AbilityCondition::DefenderIsPlayerShip => match shared.defender_opponent {
            DefenderOpponent::Player => StaticGate::Pass,
            DefenderOpponent::Hostile => StaticGate::Fail,
        },
        AbilityCondition::AttackerOfficerTalNotOnBridge => {
            if attacker_crew_tal_assigned_captain_or_bridge(crew) {
                StaticGate::Fail
            } else {
                StaticGate::Pass
            }
        }
        AbilityCondition::LiteralBool(v) => {
            if *v {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::EngagementIncludes(et) => {
            if shared.engagement_enemy_types.contains(*et) {
                StaticGate::Pass
            } else {
                StaticGate::Fail
            }
        }
        AbilityCondition::Not(inner) => match eval_static_gate(inner, shared, crew) {
            StaticGate::Pass => StaticGate::Fail,
            StaticGate::Fail => StaticGate::Pass,
            StaticGate::Unknown => StaticGate::Unknown,
        },
        AbilityCondition::And(parts) => {
            let mut any_unknown = false;
            for p in parts {
                match eval_static_gate(p, shared, crew) {
                    StaticGate::Fail => return StaticGate::Fail,
                    StaticGate::Unknown => any_unknown = true,
                    StaticGate::Pass => {}
                }
            }
            if any_unknown {
                StaticGate::Unknown
            } else {
                StaticGate::Pass
            }
        }
        AbilityCondition::Or(parts) => {
            let mut any_unknown = false;
            for p in parts {
                match eval_static_gate(p, shared, crew) {
                    StaticGate::Pass => return StaticGate::Pass,
                    StaticGate::Unknown => any_unknown = true,
                    StaticGate::Fail => {}
                }
            }
            if any_unknown {
                StaticGate::Unknown
            } else {
                StaticGate::Fail
            }
        }
        _ => StaticGate::Unknown,
    }
}

fn static_matchup_gate_score(shared: &SharedScenarioData, crew: &CrewConfiguration) -> f32 {
    let mut score = 0.0f32;
    for seat in &crew.seats {
        if let Some(ref cond) = seat.ability.condition {
            match eval_static_gate(cond, shared, crew) {
                StaticGate::Pass => score += 1.0,
                StaticGate::Fail => score -= 0.65,
                StaticGate::Unknown => {}
            }
        }
    }
    score
}

/// Encounter hints: scout/outpost flags, armada-related upstream or engagement tags, Conqueror Borg hostile tags.
/// Ability name substrings are LCARS slug text — kept conservative; capped so priors stay subordinate to [`expected_damage`].
fn encounter_tag_score(shared: &SharedScenarioData, crew: &CrewConfiguration) -> f32 {
    let Some(h) = shared.hostile_rec.as_ref() else {
        return 0.0;
    };
    let engagement = h.engagement_enemy_types_for_combat();
    let armada_ctx = upstream_hostile_ship_type_profile(h.upstream_ship_type).is_armada_target
        || engagement.contains(EnemyType::SoloArmadas)
        || engagement.contains(EnemyType::GroupArmadas)
        || engagement.contains(EnemyType::OutpostArmadas);
    let borg_ctx = h.hostile_tags.iter().any(|t| {
        let x = t.to_lowercase().replace('-', "_");
        x == "conqueror_borg"
            || x == "conqueror_borg_suppressor"
            || x == "conqueror_borg_obliterator"
    });
    let scout_outpost_ctx = h.is_scout || h.is_outpost;
    if !scout_outpost_ctx && !armada_ctx && !borg_ctx {
        return 0.0;
    }
    let mut s = 0.0f32;
    for seat in &crew.seats {
        let name = seat.ability.name.to_lowercase();
        if h.is_outpost && name.contains("outpost") {
            s += 1.0;
        }
        if h.is_scout && name.contains("scout") {
            s += 1.0;
        }
        if armada_ctx && name.contains("armada") {
            s += 1.0;
        }
        if borg_ctx && name.contains("conqueror") && name.contains("borg") {
            s += 1.0;
        }
    }
    s.min(3.0)
}

fn officer_material_set(c: &CrewCandidate) -> HashSet<String> {
    let mut s = HashSet::with_capacity(1 + c.bridge.len() + c.below_decks.len());
    s.insert(normalize_officer_name(&c.captain));
    for o in &c.bridge {
        s.insert(normalize_officer_name(o));
    }
    for o in &c.below_decks {
        s.insert(normalize_officer_name(o));
    }
    s.retain(|k| !k.is_empty());
    s
}

/// Best Jaccard overlap between `candidate` and any warm-start crew (material officers).
fn warm_start_family_score(candidate: &CrewCandidate, warm_start: &[CrewCandidate]) -> f32 {
    if warm_start.is_empty() {
        return 0.0;
    }
    let cand = officer_material_set(candidate);
    if cand.is_empty() {
        return 0.0;
    }
    let mut best = 0.0f32;
    for w in warm_start {
        let wset = officer_material_set(w);
        if wset.is_empty() {
            continue;
        }
        let inter = cand.intersection(&wset).count() as f32;
        let union = cand.union(&wset).count() as f32;
        if union > 0.0 {
            best = best.max(inter / union);
        }
    }
    best
}

/// Same captain as a warm-start crew: fraction of bridge officers shared with that crew (best over warm starts).
fn captain_bridge_warm_score(candidate: &CrewCandidate, warm_start: &[CrewCandidate]) -> f32 {
    if warm_start.is_empty() {
        return 0.0;
    }
    let cap = normalize_officer_name(&candidate.captain);
    if cap.is_empty() {
        return 0.0;
    }
    let mut best = 0.0f32;
    let denom = BRIDGE_SLOTS.max(1) as f32;
    for w in warm_start {
        if normalize_officer_name(&w.captain) != cap {
            continue;
        }
        let mut hit = 0_u32;
        for b in &candidate.bridge {
            let bn = normalize_officer_name(b);
            if bn.is_empty() {
                continue;
            }
            if w.bridge.iter().any(|x| normalize_officer_name(x) == bn) {
                hit += 1;
            }
        }
        best = best.max(hit as f32 / denom);
    }
    best
}

fn officer_material_vec_sorted(c: &CrewCandidate) -> Vec<String> {
    let mut v = Vec::with_capacity(1 + c.bridge.len() + c.below_decks.len());
    v.push(normalize_officer_name(&c.captain));
    for o in &c.bridge {
        v.push(normalize_officer_name(o));
    }
    for o in &c.below_decks {
        v.push(normalize_officer_name(o));
    }
    v.retain(|k| !k.is_empty());
    v.sort();
    v.dedup();
    v
}

fn material_pair_key(a: &str, b: &str) -> Option<(String, String)> {
    if a.is_empty() || b.is_empty() || a == b {
        return None;
    }
    if a < b {
        Some((a.to_string(), b.to_string()))
    } else {
        Some((b.to_string(), a.to_string()))
    }
}

/// Learned pair prior from reference crews (warm-start + optimize-history).
///
/// The score is the mean co-occurrence frequency of candidate officer pairs across references,
/// capped and support-gated so sparse history does not dominate analytical ranking.
fn learned_pair_prior_score(candidate: &CrewCandidate, reference_crews: &[CrewCandidate]) -> f32 {
    const MIN_REF_CREWS_FOR_PRIOR: usize = 3;
    const MIN_PAIR_SUPPORT: u32 = 2;
    const MAX_SCORE: f32 = 1.5;
    if reference_crews.len() < MIN_REF_CREWS_FOR_PRIOR {
        return 0.0;
    }
    let cand = officer_material_vec_sorted(candidate);
    if cand.len() < 2 {
        return 0.0;
    }

    let mut pair_counts: HashMap<(String, String), u32> = HashMap::new();
    for r in reference_crews {
        let names = officer_material_vec_sorted(r);
        if names.len() < 2 {
            continue;
        }
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                if let Some(k) = material_pair_key(&names[i], &names[j]) {
                    *pair_counts.entry(k).or_insert(0) += 1;
                }
            }
        }
    }
    if pair_counts.is_empty() {
        return 0.0;
    }

    let mut sum = 0.0f32;
    let mut used = 0_u32;
    let denom = reference_crews.len() as f32;
    for i in 0..cand.len() {
        for j in (i + 1)..cand.len() {
            let Some(k) = material_pair_key(&cand[i], &cand[j]) else {
                continue;
            };
            let c = pair_counts.get(&k).copied().unwrap_or(0);
            if c < MIN_PAIR_SUPPORT {
                continue;
            }
            sum += c as f32 / denom;
            used += 1;
        }
    }
    if used == 0 {
        0.0
    } else {
        (sum / used as f32).min(MAX_SCORE)
    }
}

// Weights: keep priors subordinate to [`expected_damage`] scale (typically 1e3–1e5 hull proxy).
const W_GATE: f64 = 8.0;
const W_ENCOUNTER: f64 = 6.0;
const W_WARM_JACCARD: f64 = 18.0;
const W_WARM_CAP_BRIDGE: f64 = 14.0;
const W_LEARNED_PAIR_PRIOR: f64 = 12.0;

/// Scalar for sorting candidates before analytical truncation (higher explores first).
pub(crate) fn analytical_prefilter_rank_score(
    shared: &SharedScenarioData,
    input: &CombatSimulationInput,
    candidate: &CrewCandidate,
    warm_start: &[CrewCandidate],
    enable_learned_pair_prior: bool,
) -> f64 {
    let base = f64::from(expected_damage(input));
    let gate = f64::from(static_matchup_gate_score(shared, &input.crew));
    let enc = f64::from(encounter_tag_score(shared, &input.crew));
    let warm = f64::from(warm_start_family_score(candidate, warm_start));
    let cap_br = f64::from(captain_bridge_warm_score(candidate, warm_start));
    let pair = if enable_learned_pair_prior {
        f64::from(learned_pair_prior_score(candidate, warm_start))
    } else {
        0.0
    };
    base + W_GATE * gate
        + W_ENCOUNTER * enc
        + W_WARM_JACCARD * warm
        + W_WARM_CAP_BRIDGE * cap_br
        + W_LEARNED_PAIR_PRIOR * pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{
        Ability, AbilityClass, CrewSeat, CrewSeatContext, EnemyType, EnemyTypes, ShipType,
        TimingWindow,
    };
    use crate::data::hostile::HostileRecord;

    fn minimal_shared_with_hostile(h: HostileRecord) -> SharedScenarioData {
        let engagement_enemy_types = h.engagement_enemy_types_for_combat();
        SharedScenarioData {
            ship: "test_ship".to_string(),
            hostile: "test_hostile".to_string(),
            officer_index: Default::default(),
            profile: Default::default(),
            lcars_data: None,
            resolve_options: Default::default(),
            ship_rec: None,
            hostile_rec: Some(h),
            cached_defender: None,
            cached_rounds: None,
            cached_defender_hull: None,
            cached_pierce: None,
            cached_defender_mitigation: None,
            using_placeholder_combatants: false,
            resolved_support_buffs: vec![],
            applied_support_buffs: vec![],
            support_static_buffs: Default::default(),
            unknown_support_buff_ids: vec![],
            research_derived_seats: vec![],
            forbidden_tech_derived_seats: vec![],
            borg_alcove_hull_hp_bonus: None,
            class_gated_torpedo_family_hull_hp_bonus: None,
            class_gated_torpedo_family_hostile_shield_mitigation_sum: None,
            defender_opponent: DefenderOpponent::Hostile,
            engagement_enemy_types,
            defender_level: None,
        }
    }

    fn seat_with_condition(cond: AbilityCondition) -> CrewSeatContext {
        CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: "test_ability".to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: false,
                effect: crate::combat::AbilityEffect::AttackMultiplier(1.05),
                condition: Some(cond),
            },
            boosted: false,
            officer_id: None,
            contribution_batch: crate::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
        }
    }

    fn seat_named(name: &str) -> CrewSeatContext {
        CrewSeatContext {
            seat: CrewSeat::Bridge,
            ability: Ability {
                name: name.to_string(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: false,
                effect: crate::combat::AbilityEffect::AttackMultiplier(1.05),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: crate::combat::NO_EXPLICIT_CONTRIBUTION_BATCH,
        }
    }

    #[test]
    fn static_gate_passes_for_matching_defender_ship_type() {
        let h: HostileRecord = serde_json::from_value(serde_json::json!({
            "id": "h1",
            "hostile_name": "H",
            "level": 1,
            "ship_class": "Battleship",
            "armor": 0.0,
            "shield_deflection": 0.0,
            "dodge": 0.0,
            "hull_health": 100.0,
            "shield_health": 0.0,
            "faction": { "id": 42 }
        }))
        .unwrap();
        let shared = minimal_shared_with_hostile(h);
        let crew = CrewConfiguration {
            seats: vec![seat_with_condition(AbilityCondition::DefenderShipTypeIs(
                ShipType::Battleship,
            ))],
        };
        assert!(static_matchup_gate_score(&shared, &crew) > 0.0);
    }

    #[test]
    fn static_gate_engagement_includes_matches_hostile_engagement_tags() {
        let mut h: HostileRecord = serde_json::from_value(serde_json::json!({
            "id": "h1",
            "hostile_name": "H",
            "level": 1,
            "ship_class": "Battleship",
            "armor": 0.0,
            "shield_deflection": 0.0,
            "dodge": 0.0,
            "hull_health": 100.0,
            "shield_health": 0.0,
            "faction": { "id": 42 },
            "engagement_enemy_types": ["solo_armadas"]
        }))
        .unwrap();
        let shared = minimal_shared_with_hostile(h.clone());
        let crew_pass = CrewConfiguration {
            seats: vec![seat_with_condition(AbilityCondition::EngagementIncludes(
                EnemyType::SoloArmadas,
            ))],
        };
        assert!(static_matchup_gate_score(&shared, &crew_pass) > 0.0);

        h.engagement_enemy_types = Some(EnemyTypes::default());
        let shared_default = minimal_shared_with_hostile(h);
        let crew_fail = CrewConfiguration {
            seats: vec![seat_with_condition(AbilityCondition::EngagementIncludes(
                EnemyType::SoloArmadas,
            ))],
        };
        assert!(static_matchup_gate_score(&shared_default, &crew_fail) < 0.0);
    }

    #[test]
    fn warm_start_jaccard_boosts_identical_crew() {
        let c = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "C".into()],
            below_decks: vec!["D".into()],
        };
        let warm = vec![c.clone()];
        assert!((warm_start_family_score(&c, &warm) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn captain_bridge_warm_counts_shared_bridge() {
        let c = CrewCandidate {
            captain: "Picard".into(),
            bridge: vec!["Riker".into(), "Data".into()],
            below_decks: vec![],
        };
        let warm = vec![CrewCandidate {
            captain: "Picard".into(),
            bridge: vec!["Riker".into(), "Worf".into()],
            below_decks: vec![],
        }];
        let s = captain_bridge_warm_score(&c, &warm);
        assert!((s - 0.5).abs() < 1e-5, "got {s}");
    }

    #[test]
    fn learned_pair_prior_prefers_supported_pairings() {
        let candidate_with_pair = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "X".into()],
            below_decks: vec!["Y".into()],
        };
        let candidate_without_pair = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["Q".into(), "X".into()],
            below_decks: vec!["Y".into()],
        };
        let refs = vec![
            CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "C".into()],
                below_decks: vec!["D".into()],
            },
            CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "E".into()],
                below_decks: vec!["F".into()],
            },
            CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "G".into()],
                below_decks: vec!["H".into()],
            },
        ];
        let s_pair = learned_pair_prior_score(&candidate_with_pair, &refs);
        let s_none = learned_pair_prior_score(&candidate_without_pair, &refs);
        assert!(s_pair > s_none, "s_pair={s_pair} s_none={s_none}");
        assert!(s_pair > 0.0);
    }

    #[test]
    fn learned_pair_prior_requires_enough_references() {
        let candidate = CrewCandidate {
            captain: "A".into(),
            bridge: vec!["B".into(), "C".into()],
            below_decks: vec![],
        };
        let sparse_refs = vec![
            CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "D".into()],
                below_decks: vec![],
            },
            CrewCandidate {
                captain: "A".into(),
                bridge: vec!["B".into(), "E".into()],
                below_decks: vec![],
            },
        ];
        assert_eq!(learned_pair_prior_score(&candidate, &sparse_refs), 0.0);
    }

    #[test]
    fn encounter_tag_armada_upstream_matches_ability_substring() {
        let h: HostileRecord = serde_json::from_value(serde_json::json!({
            "id": "h1",
            "hostile_name": "Armada Target",
            "level": 1,
            "ship_class": "Battleship",
            "armor": 0.0,
            "shield_deflection": 0.0,
            "dodge": 0.0,
            "hull_health": 100.0,
            "shield_health": 0.0,
            "faction": { "id": 42 },
            "upstream_ship_type": 1
        }))
        .unwrap();
        let shared = minimal_shared_with_hostile(h);
        let crew = CrewConfiguration {
            seats: vec![seat_named("strike_vs_armada")],
        };
        assert!(encounter_tag_score(&shared, &crew) > 0.0);
    }

    #[test]
    fn encounter_tag_conqueror_borg_hostile_matches_ability_substring() {
        let h: HostileRecord = serde_json::from_value(serde_json::json!({
            "id": "h1",
            "hostile_name": "Borg",
            "level": 1,
            "ship_class": "Battleship",
            "armor": 0.0,
            "shield_deflection": 0.0,
            "dodge": 0.0,
            "hull_health": 100.0,
            "shield_health": 0.0,
            "faction": { "id": 42 },
            "hostile_tags": ["conqueror_borg"]
        }))
        .unwrap();
        let shared = minimal_shared_with_hostile(h);
        let crew = CrewConfiguration {
            seats: vec![seat_named("conqueror_borg_suppression")],
        };
        assert!(encounter_tag_score(&shared, &crew) > 0.0);
    }
}
