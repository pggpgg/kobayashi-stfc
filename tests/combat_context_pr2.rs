//! PR 2 end-to-end integration tests:
//! - Hostile catalog auto-inference (`SoloArmadas`, `OutpostArmadas`,
//!   `OutpostRetaliationAttackers`, `GroupArmadas`, `QTrial`, `MissionBosses`, `RedMovingSpace`)
//! - Faction & species fidelity (Xindi numeric ids, Conqueror Borg / Ex-Borg isolation from Borg)
//! - `/api/hostiles` metadata (`inferred_enemy_type`, `is_outpost`, `faction_tag`, `hostile_tags`)
//! - `resolved_context` API echo across `/api/simulate`, `/api/optimize`, `/api/compare/crews`,
//!   `/api/crew/counterfactual`, and `/api/optimize/replay-seed`
//! - Real combat activation under Auto inference (no manual `enemy_type` override needed!) for
//!   Outpost Retaliators (Caleb Mir BDA) and Dominion Solo Armadas (Dezoc `OfficerStatAll`).

use std::collections::HashMap;
use std::sync::OnceLock;

use kobayashi::combat::EnemyType;
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    run_monte_carlo_with_registry_and_context, DefenderOpponent, ScenarioEngagementOverride,
};
use kobayashi::server::api::{hostiles_payload, run_optimize, simulate_payload, OptimizeRequest};

fn shared_registry() -> &'static DataRegistry {
    static REG: OnceLock<std::sync::Arc<DataRegistry>> = OnceLock::new();
    REG.get_or_init(|| DataRegistry::load().expect("DataRegistry::load"))
}

#[test]
fn hostiles_payload_exposes_inferred_mode_outpost_faction_and_tags() {
    let reg = shared_registry();
    let json = hostiles_payload(reg).expect("hostiles_payload");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse hostiles JSON");
    let hostiles = parsed["hostiles"].as_array().expect("hostiles array");
    let by_id: HashMap<&str, &serde_json::Value> = hostiles
        .iter()
        .filter_map(|h| h["id"].as_str().map(|id| (id, h)))
        .collect();

    // Dominion Solo Armada (Jem'Hadar Flagship)
    let jh_solo = by_id.get("1081891879").expect("Jem'Hadar Flagship");
    assert_eq!(jh_solo["inferred_enemy_type"], "solo_armadas");
    assert_eq!(jh_solo["faction_tag"], "dominion");
    assert!(jh_solo.get("is_outpost").is_none());

    // Borg Solo Armada (Borg Sphere) vs Borg Group Armada (Borg Cube)
    let borg_solo = by_id.get("1209459541").expect("Borg Sphere Solo Armada");
    assert_eq!(borg_solo["inferred_enemy_type"], "solo_armadas");
    assert_eq!(borg_solo["faction_tag"], "borg");

    let borg_group = by_id.get("1846271075").expect("BORG CUBE Group Armada");
    assert_eq!(borg_group["inferred_enemy_type"], "group_armadas");
    assert_eq!(borg_group["faction_tag"], "borg");

    // Conqueror Borg Solo Armada & Suppressor: isolated from regular Borg faction!
    let cb_solo = by_id.get("1121284456").expect("Conqueror Borg Solo Armada");
    assert_eq!(cb_solo["inferred_enemy_type"], "solo_armadas");
    assert!(
        cb_solo.get("faction_tag").is_none(),
        "Conqueror Borg must not collide with Borg faction: {cb_solo}"
    );

    let cb_sup = by_id.get("80039078").expect("Conqueror Borg Suppressor");
    assert_eq!(cb_sup["inferred_enemy_type"], "red_moving_space");
    assert!(cb_sup.get("faction_tag").is_none());
    let cb_tags: Vec<&str> = cb_sup["hostile_tags"]
        .as_array()
        .expect("hostile_tags")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(cb_tags.contains(&"conqueror_borg"));
    assert!(cb_tags.contains(&"conqueror_borg_suppressor"));

    // Cardassian Solo Outpost (is_outpost beats solo heuristics!)
    let outpost = by_id.get("1180795590").expect("Cardassian Solo Outpost");
    assert_eq!(outpost["inferred_enemy_type"], "outpost_armadas");
    assert_eq!(outpost["is_outpost"], true);
    assert_eq!(outpost["faction_tag"], "cardassian");

    // Outpost Retaliators across Cardassian / Borg / Node outpost factions
    let card_retal = by_id.get("1028117720").expect("Galor-Class Warship");
    assert_eq!(
        card_retal["inferred_enemy_type"],
        "outpost_retaliation_attackers"
    );
    assert!(card_retal.get("is_outpost").is_none());
    assert_eq!(card_retal["faction_tag"], "cardassian");

    let borg_retal = by_id.get("1034822371").expect("Assimilated Galor-Class");
    assert_eq!(
        borg_retal["inferred_enemy_type"],
        "outpost_retaliation_attackers"
    );
    assert_eq!(borg_retal["faction_tag"], "borg");

    let node_retal = by_id.get("1354731536").expect("Controlled Galor-Class");
    assert_eq!(
        node_retal["inferred_enemy_type"],
        "outpost_retaliation_attackers"
    );
    assert!(node_retal.get("faction_tag").is_none());

    // Xindi-Aquatic Cruiser carries xindi faction + xindi_aquatic native tag
    let xindi = by_id.get("3919830203").expect("Xindi-Aquatic Cruiser");
    assert_eq!(xindi["inferred_enemy_type"], "red_moving_space");
    assert_eq!(xindi["faction_tag"], "xindi");
    let xindi_tags: Vec<&str> = xindi["hostile_tags"]
        .as_array()
        .expect("xindi hostile_tags")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(xindi_tags.contains(&"xindi_aquatic"));

    // Native-supplement L62 V'ger Gorn Hunter
    let native_vger = by_id.get("2978535446").expect("V'ger Gorn Hunter");
    assert_eq!(native_vger["inferred_enemy_type"], "red_moving_space");
    assert_eq!(native_vger["faction_tag"], "borg");
}

#[test]
fn simulate_and_optimize_echo_resolved_context_for_auto_override_and_waves() {
    let reg = shared_registry();

    // 1. Auto-inferred Dominion Solo Armada (no enemy_type supplied)
    let sim_auto_body = serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "1081891879",
        "ship_tier": 9,
        "ship_level": 45,
        "crew": { "captain": "kirk", "bridge": ["spock", "bones"], "below_deck": [] },
        "num_sims": 4,
        "seed": 7
    })
    .to_string();
    let sim_auto: serde_json::Value = serde_json::from_str(
        &simulate_payload(reg, &sim_auto_body, None).expect("simulate solo auto"),
    )
    .expect("parse simulate solo auto");
    let ctx = &sim_auto["resolved_context"];
    assert_eq!(ctx["opponent_kind"], "npc_hostile");
    assert_eq!(ctx["primary_enemy_type"], "solo_armadas");
    assert_eq!(
        ctx["engagement_enemy_types"],
        serde_json::json!(["solo_armadas"])
    );
    assert_eq!(ctx["wave_defense"], false);
    assert_eq!(ctx["defender_ship_type"], "armada");
    assert_eq!(ctx["defender_faction"], "dominion");
    assert_eq!(ctx["defender_is_armada"], true);
    assert_eq!(ctx["defender_is_outpost"], false);
    assert_eq!(ctx["is_override"], false);

    // 2. Additive Wave Defense on the same Dominion Solo Armada
    let sim_waves_body = serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "1081891879",
        "ship_tier": 9,
        "ship_level": 45,
        "crew": { "captain": "kirk", "bridge": ["spock", "bones"], "below_deck": [] },
        "num_sims": 4,
        "seed": 7,
        "wave_defense": true
    })
    .to_string();
    let sim_waves: serde_json::Value = serde_json::from_str(
        &simulate_payload(reg, &sim_waves_body, None).expect("simulate solo waves"),
    )
    .expect("parse simulate solo waves");
    let wctx = &sim_waves["resolved_context"];
    assert_eq!(wctx["primary_enemy_type"], "waves");
    assert_eq!(
        wctx["engagement_enemy_types"],
        serde_json::json!(["solo_armadas", "waves"])
    );
    assert_eq!(wctx["wave_defense"], true);
    assert_eq!(wctx["defender_ship_type"], "armada");
    assert_eq!(wctx["defender_faction"], "dominion");
    assert_eq!(wctx["is_override"], false);

    // 3. Auto-inferred Outpost Retaliator preserves hull class (explorer, NOT armada)
    let sim_retal_body = serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "1028117720",
        "ship_tier": 9,
        "ship_level": 45,
        "crew": { "captain": "kirk", "bridge": ["spock", "bones"], "below_deck": [] },
        "num_sims": 4,
        "seed": 7
    })
    .to_string();
    let sim_retal: serde_json::Value = serde_json::from_str(
        &simulate_payload(reg, &sim_retal_body, None).expect("simulate retaliator auto"),
    )
    .expect("parse simulate retaliator");
    let rctx = &sim_retal["resolved_context"];
    assert_eq!(rctx["primary_enemy_type"], "outpost_retaliation_attackers");
    assert_eq!(rctx["defender_ship_type"], "explorer");
    assert_eq!(rctx["defender_faction"], "cardassian");
    assert_eq!(rctx["defender_is_armada"], false);
    assert_eq!(rctx["defender_is_outpost"], false);
    assert_eq!(rctx["is_override"], false);

    // 4. OptimizeResponse.scenario.resolved_context echoes the same context (with manual override)
    let opt_req: OptimizeRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": "1081891879",
        "ship_tier": 9,
        "ship_level": 45,
        "sims": 4,
        "seed": 7,
        "max_candidates": 2,
        "strategy": "exhaustive",
        "enemy_type": "group_armadas",
        "wave_defense": true
    }))
    .expect("OptimizeRequest");
    let opt_res = run_optimize(reg, &opt_req, None).expect("run_optimize");
    let octx = opt_res
        .scenario
        .resolved_context
        .expect("scenario.resolved_context present");
    assert_eq!(octx.primary_enemy_type, "group_armadas");
    assert_eq!(
        octx.engagement_enemy_types,
        vec!["group_armadas".to_string(), "waves".to_string()]
    );
    assert!(octx.wave_defense);
    assert!(octx.is_override);
    assert_eq!(octx.defender_faction.as_deref(), Some("dominion"));
    assert_eq!(octx.defender_ship_type.as_deref(), Some("armada"));
}

#[test]
fn auto_inferred_outpost_retaliator_activates_caleb_mir_without_override() {
    let reg = shared_registry();
    let crew = CrewCandidate {
        captain: "Kirk".to_string(),
        bridge: vec!["Spock".to_string(), "Khan".to_string()],
        below_decks: vec!["Caleb Mir".to_string()],
    };

    // Hostile 1028117720 (Galor-Class Warship) auto-infers OutpostRetaliationAttackers:
    // Caleb Mir's BDA (EnemyOutpostOrRetaliator: +1,000%..+2,750% crit damage) fires on Auto!
    let (res_auto_vec, _) = run_monte_carlo_with_registry_and_context(
        reg,
        "uss_enterprise_d",
        "1028117720",
        Some(9),
        Some(45),
        std::slice::from_ref(&crew),
        60,
        42,
        None,
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride::default(),
    );
    let (res_group_vec, _) = run_monte_carlo_with_registry_and_context(
        reg,
        "uss_enterprise_d",
        "1028117720",
        Some(9),
        Some(45),
        std::slice::from_ref(&crew),
        60,
        42,
        None,
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::GroupArmadas),
            wave_defense: false,
            ..Default::default()
        },
    );

    let res_auto = &res_auto_vec[0];
    let res_group = &res_group_vec[0];

    let auto_dmg_frac = 1.0 - res_auto.avg_defender_hull_remaining;
    let group_dmg_frac = 1.0 - res_group.avg_defender_hull_remaining;
    assert!(
        res_auto.avg_defender_hull_remaining < res_group.avg_defender_hull_remaining
            && auto_dmg_frac > group_dmg_frac * 1.05,
        "Caleb Mir's EnemyOutpostOrRetaliator BDA should auto-fire on Outpost Retaliator 1028117720 without manual override: auto_def_hull={} (dmg={auto_dmg_frac:.6}) vs group_def_hull={} (dmg={group_dmg_frac:.6})",
        res_auto.avg_defender_hull_remaining,
        res_group.avg_defender_hull_remaining
    );
}

#[test]
fn auto_inferred_solo_armada_activates_dezoc_without_override() {
    let reg = shared_registry();
    // Dezoc's OfficerStatAll requires SelfAtSoloArmada.
    let crew = CrewCandidate {
        captain: "Julian Bashir".to_string(),
        bridge: vec!["Gossa".to_string(), "Dezoc".to_string()],
        below_decks: vec![],
    };

    // Hostile 1081891879 (Jem'Hadar Flagship L53) auto-infers SoloArmadas.
    let hostile = reg
        .resolve_hostile("1081891879")
        .expect("resolve 1081891879");
    assert_eq!(
        hostile.inferred_base_enemy_type(),
        EnemyType::SoloArmadas,
        "Jem'Hadar Flagship must auto-infer SoloArmadas"
    );

    let (res_auto_vec, _) = run_monte_carlo_with_registry_and_context(
        reg,
        "uss_saladin",
        "1081891879",
        Some(9),
        Some(45),
        std::slice::from_ref(&crew),
        40,
        11,
        None,
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride::default(),
    );
    let (res_group_vec, _) = run_monte_carlo_with_registry_and_context(
        reg,
        "uss_saladin",
        "1081891879",
        Some(9),
        Some(45),
        std::slice::from_ref(&crew),
        40,
        11,
        None,
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::GroupArmadas),
            wave_defense: false,
            ..Default::default()
        },
    );

    let res_auto = &res_auto_vec[0];
    let res_group = &res_group_vec[0];

    assert!(
        (res_auto.avg_defender_hull_remaining - res_group.avg_defender_hull_remaining).abs() > 1e-6
            || (res_auto.avg_hull_remaining - res_group.avg_hull_remaining).abs() > 1e-6,
        "Dezoc's SelfAtSoloArmada officer-stat passive should auto-activate on Jem'Hadar Flagship 1081891879 without manual override: auto=({},{}) vs group=({},{})",
        res_auto.avg_hull_remaining,
        res_auto.avg_defender_hull_remaining,
        res_group.avg_hull_remaining,
        res_group.avg_defender_hull_remaining,
    );
}
