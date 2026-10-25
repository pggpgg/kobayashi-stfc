//! Independent observations from tracked game exports.
//!
//! These are not snapshot-bound research calibration fights. The ignored diagnostic
//! compares recorded final fleet stats with recorded per-hit damage columns; it
//! deliberately reports discrepancies instead of accepting a fitted tolerance.

use std::collections::HashMap;
use std::path::Path;

use kobayashi::combat::{
    compute_damage_through_factor, mitigation, parse_fight_export, pierce_damage_through_bonus,
    AttackerStats, DefenderStats, ShipType,
};
use serde_json::{json, Value};

const REALTA: &str = include_str!("../fight samples/realta vs takret militia 10.csv");
const FRESH_REALTA_OBSERVATION: &str =
    include_str!("fixtures/recorded_fights/realta_scouting_bandit_26_observed.tsv");
const TWO_ROUND_REALTA_OBSERVATION: &str =
    include_str!("fixtures/recorded_fights/realta_scouting_bandit_26_20261023_observed.tsv");

fn section<'a>(csv: &'a str, first_column: &str) -> Vec<HashMap<&'a str, &'a str>> {
    let mut lines = csv.lines();
    let header = lines
        .find(|line| line.split('\t').next() == Some(first_column))
        .expect("required export section");
    let columns: Vec<_> = header.split('\t').map(str::trim).collect();
    lines
        .take_while(|line| !line.trim().is_empty())
        .map(|line| {
            columns
                .iter()
                .copied()
                .zip(line.split('\t').map(str::trim))
                .collect()
        })
        .collect()
}

fn number(row: &HashMap<&str, &str>, column: &str) -> f64 {
    match row.get(column).copied().unwrap_or("--") {
        "--" | "" => 0.0,
        value => value.parse().expect("numeric export cell"),
    }
}

fn fleet_number(row: &HashMap<String, String>, column: &str) -> f64 {
    row.get(column)
        .expect("required fleet column")
        .parse()
        .expect("numeric fleet cell")
}

#[test]
fn realta_log_preserves_raw_damage_and_actual_health_loss_as_distinct_observations() {
    let export = parse_fight_export(REALTA).expect("tracked real game export");
    let summary = section(REALTA, "Player Name");
    let player = summary[0]["Player Name"];
    let attacks: Vec<_> = section(REALTA, "Round")
        .into_iter()
        .filter(|row| row["Type"] == "Attack")
        .collect();
    assert_eq!(attacks.len(), 4);

    // This identity is observed in this simple export, not assumed for all log
    // versions or mechanics. Incoming hits have neither isolytic nor apex damage.
    for row in &attacks {
        let recorded_pool = number(row, "Total Damage") + number(row, "Total Isolytic Damage");
        let accounted_pool = number(row, "Hull Damage")
            + number(row, "Shield Damage")
            + number(row, "Mitigated Damage");
        assert_eq!(recorded_pool, accounted_pool);
        assert_eq!(number(row, "Mitigated Isolytic Damage"), 0.0);
        assert_eq!(number(row, "Mitigated Apex Barrier"), 0.0);
    }
    let incoming: Vec<_> = attacks
        .iter()
        .filter(|row| row["Target Name"] == player)
        .collect();
    assert_eq!(incoming.len(), 2);
    assert_eq!(number(incoming[0], "Total Damage"), 133.0);
    assert_eq!(number(incoming[0], "Mitigated Damage"), 89.0);
    assert_eq!(number(incoming[1], "Total Damage"), 148.0);
    assert_eq!(number(incoming[1], "Mitigated Damage"), 99.0);
    let incoming_hull: f64 = incoming.iter().map(|row| number(row, "Hull Damage")).sum();
    let incoming_shield: f64 = incoming
        .iter()
        .map(|row| number(row, "Shield Damage"))
        .sum();
    assert_eq!(incoming_hull, 17.0);
    assert_eq!(incoming_shield, 76.0);
    assert_eq!(
        incoming_hull,
        number(&summary[0], "Hull Health") - number(&summary[0], "Hull Health Remaining")
    );
    assert_eq!(
        incoming_shield,
        number(&summary[0], "Shield Health") - number(&summary[0], "Shield Health Remaining")
    );

    let outgoing_raw: f64 = attacks
        .iter()
        .filter(|row| row["Attacker Name"] == player)
        .map(|row| number(row, "Total Damage"))
        .sum();
    assert_eq!(outgoing_raw, 12_958.0);
    assert_eq!(export.total_damage, 830.0);
    // The parser's total is the enemy's lost HP, including clamping/overkill.
    // Comparing it with the raw attack column would give a false accuracy score.
    assert_eq!(
        export.total_damage,
        number(&summary[1], "Hull Health") + number(&summary[1], "Shield Health")
    );
}

/// This checks observed export semantics, not whether the engine reproduces the battle.
#[test]
fn fresh_realta_observation_preserves_apex_accounting_and_overkill() {
    let export = parse_fight_export(FRESH_REALTA_OBSERVATION).expect("sanitized real game export");
    let summary = section(FRESH_REALTA_OBSERVATION, "Player Name");
    let player = summary[0]["Player Name"];
    let attacks: Vec<_> = section(FRESH_REALTA_OBSERVATION, "Round")
        .into_iter()
        .filter(|row| row["Type"] == "Attack")
        .collect();
    let incoming: Vec<_> = attacks
        .iter()
        .filter(|row| row["Target Name"] == player)
        .collect();
    let outgoing: Vec<_> = attacks
        .iter()
        .filter(|row| row["Attacker Name"] == player)
        .collect();
    assert!(export.attacker_won);
    assert_eq!(export.rounds, 1);
    assert_eq!(export.events.len(), 11);
    assert_eq!(incoming.len(), 2);
    assert_eq!(outgoing.len(), 2);
    assert_eq!(export.enemy_player_name.as_deref(), Some("Scouting Bandit"));
    assert_eq!(export.enemy_ship_level, Some(26));

    // The fresh export adds a Mitigated Critical Damage column. Preserve all
    // observed damage fields by header name, including the nonzero apex channel.
    let parsed_attacks: Vec<_> = export
        .events
        .iter()
        .filter(|event| event.event_type == "Attack")
        .collect();
    assert_eq!(parsed_attacks.len(), attacks.len());
    for (parsed, row) in parsed_attacks.iter().zip(&attacks) {
        assert_eq!(parsed.total_damage, number(row, "Total Damage"));
        assert_eq!(parsed.hull_damage, number(row, "Hull Damage"));
        assert_eq!(parsed.shield_damage, number(row, "Shield Damage"));
        assert_eq!(parsed.critical_hit, row["Critical Hit?"] == "YES");
        let raw_pool = number(row, "Total Damage") + number(row, "Total Isolytic Damage");
        let accounted_pool = number(row, "Hull Damage")
            + number(row, "Shield Damage")
            + number(row, "Mitigated Damage")
            + number(row, "Mitigated Isolytic Damage")
            + number(row, "Mitigated Apex Barrier")
            + number(row, "Mitigated Critical Damage");
        assert_eq!(raw_pool, accounted_pool);
    }
    assert_eq!(number(incoming[0], "Mitigated Apex Barrier"), 48.0);
    assert_eq!(number(incoming[1], "Mitigated Apex Barrier"), 55.0);
    let incoming_hull: f64 = incoming.iter().map(|row| number(row, "Hull Damage")).sum();
    let incoming_shield: f64 = incoming
        .iter()
        .map(|row| number(row, "Shield Damage"))
        .sum();
    assert_eq!(incoming_hull, 228.0);
    assert_eq!(incoming_shield, 1073.0);
    assert_eq!(
        incoming_hull,
        number(&summary[0], "Hull Health") - number(&summary[0], "Hull Health Remaining")
    );
    assert_eq!(
        incoming_shield,
        number(&summary[0], "Shield Health") - number(&summary[0], "Shield Health Remaining")
    );
    let outgoing_regular: f64 = outgoing.iter().map(|row| number(row, "Total Damage")).sum();
    let outgoing_isolytic: f64 = outgoing
        .iter()
        .map(|row| number(row, "Total Isolytic Damage"))
        .sum();
    let outgoing_reported_health_damage: f64 = outgoing
        .iter()
        .map(|row| number(row, "Hull Damage") + number(row, "Shield Damage"))
        .sum();
    assert_eq!(outgoing_regular, 31_494.0);
    assert_eq!(outgoing_isolytic, 99_942.0);
    assert_eq!(outgoing_reported_health_damage, 110_516.0);
    // The parser's total is actual enemy HP lost, not raw damage or the
    // reported pre-clamp hull/shield allocations, which include overkill.
    assert_eq!(export.total_damage, 22_200.0);
    assert_eq!(
        outgoing_reported_health_damage - export.total_damage,
        88_316.0
    );
    assert_eq!(export.defender_hull_remaining, 0.0);
    assert_eq!(export.defender_shield_remaining, 0.0);
}

#[test]
fn two_round_realta_export_preserves_damage_variation_and_repaired_health() {
    let export = parse_fight_export(TWO_ROUND_REALTA_OBSERVATION).unwrap();
    let summary = section(TWO_ROUND_REALTA_OBSERVATION, "Player Name");
    let player = summary[0]["Player Name"];
    let attacks: Vec<_> = section(TWO_ROUND_REALTA_OBSERVATION, "Round")
        .into_iter()
        .filter(|r| r["Type"] == "Attack")
        .collect();
    let outgoing: Vec<_> = attacks
        .iter()
        .filter(|r| r["Attacker Name"] == player)
        .collect();
    let incoming: Vec<_> = attacks
        .iter()
        .filter(|r| r["Target Name"] == player)
        .collect();
    assert_eq!(export.rounds, 2);
    assert!(export.attacker_won);
    assert_eq!(attacks.len(), 7);
    assert_eq!(incoming.len(), 4);
    assert_eq!(
        outgoing
            .iter()
            .map(|r| number(r, "Total Damage"))
            .collect::<Vec<_>>(),
        vec![3937.0, 3452.0, 32008.0]
    );
    assert_eq!(
        outgoing
            .iter()
            .map(|r| r["Critical Hit?"])
            .collect::<Vec<_>>(),
        vec!["NO", "NO", "YES"]
    );
    for row in &attacks {
        assert_eq!(
            number(row, "Total Damage") + number(row, "Total Isolytic Damage"),
            number(row, "Hull Damage")
                + number(row, "Shield Damage")
                + number(row, "Mitigated Damage")
                + number(row, "Mitigated Isolytic Damage")
                + number(row, "Mitigated Apex Barrier")
                + number(row, "Mitigated Critical Damage")
        );
    }
    let round_one_hull: f64 = incoming
        .iter()
        .filter(|r| r["Round"] == "1")
        .map(|r| number(r, "Hull Damage"))
        .sum();
    assert_eq!(round_one_hull, 358.0);
    let gross_hull: f64 = incoming.iter().map(|r| number(r, "Hull Damage")).sum();
    assert_eq!(gross_hull, 465.0);
    let displayed_loss =
        number(&summary[0], "Hull Health") - number(&summary[0], "Hull Health Remaining");
    assert_eq!(displayed_loss, 437.0);
    // The CSV truncates health and omits the numeric repair event. The automatic
    // journal separately records 28.64 repair; do not equate gross hits to net loss.
    assert_eq!(gross_hull - displayed_loss, 28.0);
    let shield_loss: f64 = incoming.iter().map(|r| number(r, "Shield Damage")).sum();
    assert_eq!(shield_loss, 2195.0);
    assert_eq!(
        shield_loss,
        number(&summary[0], "Shield Health") - number(&summary[0], "Shield Health Remaining")
    );
    assert_eq!(export.total_damage, 22200.0);
}

fn direction_report(
    label: &str,
    attacking_fleet: &HashMap<String, String>,
    defending_fleet: &HashMap<String, String>,
    defender_class: ShipType,
    hits: &[&HashMap<&str, &str>],
) -> Value {
    let attacker = AttackerStats {
        armor_piercing: fleet_number(attacking_fleet, "Armour Pierce"),
        shield_piercing: fleet_number(attacking_fleet, "Shield Pierce"),
        accuracy: fleet_number(attacking_fleet, "Accuracy"),
    };
    let defender = DefenderStats {
        armor: fleet_number(defending_fleet, "Armour"),
        shield_deflection: fleet_number(defending_fleet, "Shield Deflection"),
        dodge: fleet_number(defending_fleet, "Dodge"),
    };
    let model = mitigation(defender, attacker, defender_class);
    let pierce_bonus = pierce_damage_through_bonus(defender, attacker, defender_class);
    let effective_model = 1.0 - compute_damage_through_factor(1.0 - model, pierce_bonus, 0.0);
    let raw_sum: f64 = hits.iter().map(|row| number(row, "Total Damage")).sum();
    let mitigated_sum: f64 = hits.iter().map(|row| number(row, "Mitigated Damage")).sum();
    let observed = (raw_sum > 0.0).then(|| mitigated_sum / raw_sum);
    let rows: Vec<_> = hits
        .iter()
        .filter(|row| number(row, "Total Damage") > 0.0)
        .map(|row| {
            let ratio = number(row, "Mitigated Damage") / number(row, "Total Damage");
            json!({
                "round": number(row, "Round"),
                "battle_event": number(row, "Battle Event"),
                "critical": row.get("Critical Hit?") == Some(&"YES"),
                "raw_damage": number(row, "Total Damage"),
                "mitigated_damage": number(row, "Mitigated Damage"),
                "observed_fraction": ratio,
                "component_model_minus_observed_pp": (model-ratio)*100.0,
                "with_additional_pierce_minus_observed_pp": (effective_model-ratio)*100.0,
                "isolytic_present": number(row, "Total Isolytic Damage") != 0.0,
                "apex_present": number(row, "Mitigated Apex Barrier") != 0.0,
            })
        })
        .collect();
    json!({
        "direction": label,
        "defender_class": format!("{defender_class:?}"),
        "attack_events": hits.len(),
        "raw_damage_sum": raw_sum,
        "mitigated_damage_sum": mitigated_sum,
        "observed_raw_weighted_fraction": observed,
        "component_model": model,
        "component_model_with_additional_pierce": effective_model,
        "component_model_minus_observed_pp": observed.map(|v| (model-v)*100.0),
        "with_additional_pierce_minus_observed_pp": observed.map(|v| (effective_model-v)*100.0),
        "hits": rows,
    })
}

#[test]
#[ignore = "diagnostic, not an accuracy gate; writes only when an explicit report path is supplied"]
fn recorded_fleet_stats_mitigation_residuals() {
    let cases = [
        (
            "realta vs takret militia 10.csv",
            "realta",
            ShipType::Explorer,
            ShipType::Battleship,
        ),
        (
            "uss enterprise d vs vger hurak 59.csv",
            "uss_enterprise_d",
            ShipType::Explorer,
            ShipType::Interceptor,
        ),
        (
            "enterprise-d_vs_424930668_51_loss.csv",
            "uss_enterprise_d",
            ShipType::Explorer,
            ShipType::Interceptor,
        ),
        (
            "enterprise-d_vs_728388414_51_loss.csv",
            "uss_enterprise_d",
            ShipType::Explorer,
            ShipType::Battleship,
        ),
        (
            "gorn-evisc_vs_447012258_60_easywin_trivial.csv",
            "gorn_eviscerator",
            ShipType::Interceptor,
            ShipType::Battleship,
        ),
        (
            "borg sphere with forbidden officer vs conqueror borg supressor.csv",
            "borg_sphere",
            ShipType::Battleship,
            ShipType::Explorer,
        ),
        (
            "ss revenant t6 lvl30 vs lvl 52 borg type 031 solo armada.csv",
            "ss_revenant",
            ShipType::Interceptor,
            ShipType::Survey,
        ),
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut reports = Vec::new();
    for (file, ship_id, player_class, enemy_class) in cases {
        let csv =
            std::fs::read_to_string(root.join("fight samples").join(file)).expect("tracked log");
        let export = parse_fight_export(&csv).expect("parse tracked log");
        let summary = section(&csv, "Player Name");
        let player = summary[0]["Player Name"];
        let events = section(&csv, "Round");
        let attacks: Vec<_> = events
            .iter()
            .filter(|row| row["Type"] == "Attack")
            .collect();
        let outgoing: Vec<_> = attacks
            .iter()
            .copied()
            .filter(|row| row["Attacker Name"] == player)
            .collect();
        let incoming: Vec<_> = attacks
            .iter()
            .copied()
            .filter(|row| row["Target Name"] == player)
            .collect();
        let ship: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("data/ships_extended/{ship_id}.json")))
                .expect("ship catalog"),
        )
        .expect("ship JSON");
        let report = json!({
            "source": format!("fight samples/{file}"),
            "timestamp_as_exported_without_timezone": summary[0]["Timestamp"],
            "ship_id": ship_id,
            "ship_level": summary[0]["Ship Level"],
            "grade_from_current_catalog_not_log": ship["grade"],
            "rounds": export.rounds,
            "attacker_won": export.attacker_won,
            "has_matched_research_snapshot": false,
            "ability_event_count": events.iter().filter(|row| row["Type"].contains("Ability")).count(),
            "outgoing": direction_report("player_to_enemy", &export.player_fleet, &export.enemy_fleet, enemy_class, &outgoing),
            "incoming": direction_report("enemy_to_player", &export.enemy_fleet, &export.player_fleet, player_class, &incoming),
        });
        eprintln!(
            "{}: outgoing residual={}pp incoming residual={}pp",
            file,
            report["outgoing"]["component_model_minus_observed_pp"],
            report["incoming"]["component_model_minus_observed_pp"]
        );
        reports.push(report);
    }
    let result = json!({
        "kind": "recorded-final-fleet-stat diagnostic, not research or combat accuracy validation",
        "limitations": [
            "No immutable historical profile is bound to these exports.",
            "Recorded fleet stats already include player bonuses; the research compiler is deliberately bypassed.",
            "Per-hit buffs, morale, debuffs, hostile abilities and column semantics can differ from fleet-header stats.",
            "The extra-pierce column is an alternative arithmetic diagnostic, not an assertion that every engine path uses it.",
            "Current catalog classes/grades are used; grade and base weapon stats are absent from the log.",
            "The Revenant fight is an armada outside ordinary hostile scope; the Borg Sphere export has no attack rows."
        ],
        "fights": reports,
    });
    if let Some(path) = std::env::var_os("KOBAYASHI_REAL_LOG_DIAGNOSTIC_PATH") {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&result).expect("report JSON"),
        )
        .expect("write explicitly requested diagnostic report");
    }
}
