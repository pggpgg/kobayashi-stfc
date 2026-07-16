//! Update 92 (IDIC) Spock trio: Origin Sector [OGN] faction-gated bridge buffs
//! (`EnemyHullFaction` `faction_id=3132466015`) plus below-decks gates.

use std::collections::HashMap;
use std::sync::OnceLock;

use kobayashi::combat::abilities::CrewConfiguration;
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, OpponentFactionTag, ShipType,
    SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    LcarsOfficer, ResolveOptions,
};

/// Upstream `faction.id` shared by Quantum Adjudicator / Guardian / Tesseract hostiles.
const OGN_FACTION_ID: i64 = 3132466015;

fn lcars_officers_by_id() -> &'static HashMap<String, LcarsOfficer> {
    static OFFICERS: OnceLock<HashMap<String, LcarsOfficer>> = OnceLock::new();
    OFFICERS.get_or_init(|| {
        let file = build_officer_model_file_default().expect("build officer model");
        index_lcars_officers_by_id(file.officers)
    })
}

fn attacker_always_crit() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 500.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.5,
        crit_chance: 1.0,
        crit_multiplier: 1.5,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 500.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn passive_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 0.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 50_000_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

#[test]
fn idic_spock_bridge_abilities_map_ogn_faction_gate() {
    let officers = lcars_officers_by_id();
    for (id, stat, rank5_value) in [
        ("female-spock-c21a12", "isolytic_damage", 30.0_f64),
        ("cat-spock-61edec", "crit_damage", 250.0_f64),
        ("ambassador-spock-e7945f", "isolytic_defense", 800.0_f64),
    ] {
        let officer = officers.get(id).unwrap_or_else(|| panic!("{id} in model"));
        let bridge = officer
            .bridge_ability
            .as_ref()
            .unwrap_or_else(|| panic!("{id} bridge ability"));
        let effect = bridge
            .effects
            .iter()
            .find(|e| e.stat.as_deref() == Some(stat))
            .unwrap_or_else(|| panic!("{id} bridge {stat} effect"));
        assert_eq!(
            effect.trigger.as_deref(),
            Some("on_combat_start"),
            "{id} trigger"
        );
        let cond = effect.condition.as_ref().unwrap_or_else(|| {
            panic!("{id} bridge {stat} must be OGN-gated, found unconditional effect")
        });
        let has_faction_gate = |c: &kobayashi::lcars::LcarsCondition| {
            c.condition_type == "defender_hull_faction_id" && c.faction_id == Some(OGN_FACTION_ID)
        };
        let gated = has_faction_gate(cond)
            || cond
                .conditions
                .as_ref()
                .is_some_and(|kids| kids.iter().any(has_faction_gate));
        assert!(gated, "{id} bridge {stat} missing OGN faction_id gate");
        let last = effect
            .scaling
            .as_ref()
            .and_then(|s| s.values.as_ref())
            .and_then(|v| v.last().copied())
            .unwrap_or_else(|| panic!("{id} bridge {stat} scaling values"));
        assert!(
            (last - rank5_value).abs() < 1e-9,
            "{id} rank-5 value: expected {rank5_value}, got {last}"
        );
    }
}

#[test]
fn cat_spock_crit_damage_applies_only_vs_ogn_faction() {
    let crew = resolve_crew_to_buff_set(
        "",
        &["cat-spock-61edec".into()],
        &[],
        lcars_officers_by_id(),
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let attacker = attacker_always_crit();
    let defender = passive_defender();
    let empty = CrewConfiguration::default();

    let damage_for = |faction_id: i64, with_crew: bool| {
        let config = SimulationConfig {
            rounds: 1,
            seed: 7,
            trace_mode: TraceMode::Off,
            defender_hull_faction_id: faction_id,
            ..Default::default()
        };
        simulate_combat_from_setup(
            &build_combat_setup(
                &attacker,
                &defender,
                &config,
                if with_crew { &crew } else { &empty },
                OpponentFactionTag::Unknown,
                ShipType::Battleship,
                ShipType::Battleship,
                true,
                false,
                &empty,
            ),
            7,
        )
        .total_damage
    };

    let base_ogn = damage_for(OGN_FACTION_ID, false);
    let with_ogn = damage_for(OGN_FACTION_ID, true);
    assert!(
        with_ogn > base_ogn * 10.0,
        "vs OGN hostile the +25,000% crit damage must dominate (base={base_ogn}, with={with_ogn})"
    );

    let base_other = damage_for(0, false);
    let with_other = damage_for(0, true);
    assert!(
        (with_other - base_other).abs() < 1e-6,
        "vs non-OGN hostile Tooth and Claw must not apply (base={base_other}, with={with_other})"
    );
}

#[test]
fn cat_spock_below_decks_apex_barrier_gates_non_armada_hostile() {
    let officers = lcars_officers_by_id();
    let officer = officers.get("cat-spock-61edec").expect("cat spock");
    let bd = officer.below_decks_ability.as_ref().expect("BD ability");
    let effect = bd
        .effects
        .iter()
        .find(|e| e.stat.as_deref() == Some("apex_barrier"))
        .expect("BD apex_barrier effect");
    let cond = effect.condition.as_ref().expect("BD condition");
    assert_eq!(cond.condition_type, "and");
    let kids = cond.conditions.as_ref().expect("and children");
    assert!(kids
        .iter()
        .any(|c| c.condition_type == "defender_is_npc_hostile"));
    assert!(kids.iter().any(|c| {
        c.condition_type == "not"
            && c.conditions.as_ref().is_some_and(|inner| {
                inner
                    .iter()
                    .any(|i| i.condition_type == "defender_ship_type_is")
            })
    }));
    let last = effect
        .scaling
        .as_ref()
        .and_then(|s| s.values.as_ref())
        .and_then(|v| v.last().copied())
        .expect("scaling values");
    assert!((last - 10_000.0).abs() < 1e-9, "rank-5 apex barrier");
}

#[test]
fn ambassador_spock_below_decks_node_defense_is_inert() {
    let officers = lcars_officers_by_id();
    let officer = officers.get("ambassador-spock-e7945f").expect("ambassador");
    let bd = officer.below_decks_ability.as_ref().expect("BD ability");
    let effect = bd
        .effects
        .iter()
        .find(|e| e.stat.as_deref() == Some("apex_barrier"))
        .expect("BD apex_barrier effect");
    // SelfMining maps to literal_false: node-defense state is not simulated, so the
    // effect must carry an always-false gate rather than apply unconditionally.
    let cond = effect.condition.as_ref().expect("BD condition");
    let is_literal_false =
        |c: &kobayashi::lcars::LcarsCondition| c.condition_type == "literal_false";
    let inert = is_literal_false(cond)
        || cond
            .conditions
            .as_ref()
            .is_some_and(|kids| kids.iter().any(is_literal_false));
    assert!(
        inert,
        "My Mine to Your Mine must be gated inert (literal_false)"
    );
}
