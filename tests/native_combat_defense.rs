//! Native ranked sources through the combat loop, independently of account files.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass,
    AbilityCondition, AbilityEffect, AttackerStats, CombatEvent, Combatant, CrewConfiguration,
    CrewSeat, CrewSeatContext, DefenderStats, HostileMitigationParams, OpponentFactionTag,
    ShipType, SimulationConfig, SimulationResult, TimingWindow, TraceMode,
};
use kobayashi::data::{import::GlobalBuffEntry, native_consumable_buffs, native_research_buffs};

fn native_crew() -> CrewConfiguration {
    let rows: Vec<_> = [
        (2817985671, 15),
        (4289628197, 15),
        (1265887920, 10),
        (2708670536, 10),
        (266932754, 10),
        (455315064, 10),
        (4285570420, 61),
        (3568908575, 61),
        (3241601998, 61),
        (2646870581, 61),
        (1531532954, 61),
        (1232730640, 61),
        (446001877, 61),
        (361450259, 61),
        (35633979, 61),
    ]
    .into_iter()
    .map(|(bid, level)| GlobalBuffEntry {
        bid,
        level,
        expiry_time: None,
    })
    .collect();
    let mut seats = native_research_buffs::combat_start_defense_seats_for_imported_buffs(
        &rows,
        Some("explorer"),
        0,
    );
    seats.extend(
        native_consumable_buffs::combat_start_defense_seats_for_imported_buffs(
            &rows,
            Some("explorer"),
            0,
        ),
    );
    CrewConfiguration { seats }
}

fn fight(crew: &CrewConfiguration, opponent: ShipType, pvp: bool) -> SimulationResult {
    fight_with_rounding(crew, opponent, pvp, false)
}

fn fight_with_rounding(
    crew: &CrewConfiguration,
    opponent: ShipType,
    pvp: bool,
    round_native_defense_stats: bool,
) -> SimulationResult {
    let attacker = Combatant {
        attack: 1.0,
        hull_health: 1e8,
        // Source-reconstructed launch pools, including the Explorer's officer
        // Defense breakpoint contribution of one to shield deflection only.
        armor: 27.79,
        shield_deflection: 29.79,
        dodge: 28.19,
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats,
            defender_stats: DefenderStats {
                armor: 3.0,
                shield_deflection: 22.0,
                dodge: 1.0,
            },
            base_attacker_stats: AttackerStats {
                armor_piercing: 750.0,
                shield_piercing: 750.0,
                accuracy: 4500.0,
            },
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let defender = Combatant {
        attack: 2000.0,
        hull_health: 1e8,
        ..Default::default()
    };
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        crew,
        OpponentFactionTag::Unknown,
        opponent,
        ShipType::Explorer,
        !pvp,
        pvp,
        &CrewConfiguration::default(),
    )
}

fn counters(result: &SimulationResult) -> Vec<&CombatEvent> {
    result
        .events
        .iter()
        .filter(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn ranked_combat_start_sources_retain_opponent_scope_and_do_not_accumulate() {
    for (opponent, pvp, bonus) in [
        (ShipType::Explorer, false, 13.0),
        (ShipType::Battleship, false, 13.0),
        (ShipType::Armada, false, 4.75),
        (ShipType::Explorer, true, 1.75),
    ] {
        let result = fight(&native_crew(), opponent, pvp);
        let rows = counters(&result);
        assert_eq!(rows.len(), 2);
        for row in rows {
            close(
                row.values["base_mitigation_stats_bonus"].as_f64().unwrap(),
                bonus,
            );
            for (key, expected) in [
                ("effective_armor", 86.37 + 3.0 * bonus),
                ("effective_shield_deflection", 677.38 + 22.0 * bonus),
                ("effective_dodge", 29.19 + bonus),
            ] {
                close(row.values[key].as_f64().unwrap(), expected);
            }
        }
    }
}

#[test]
fn expiring_round_start_bonus_keeps_the_persistent_combat_start_pool() {
    let mut crew = native_crew();
    crew.seats.push(CrewSeatContext::legacy(
        CrewSeat::Ship,
        Ability {
            name: "one_round_defense".into(),
            class: AbilityClass::ShipAbility,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::BaseMitigationStatsBonus(0.5),
            condition: Some(AbilityCondition::RoundRange { min: 1, max: 1 }),
            weapon_scope: Default::default(),
        },
        false,
    ));
    let result = fight(&crew, ShipType::Explorer, false);
    let rows = counters(&result);
    assert_eq!(rows.len(), 2);
    close(
        rows[0].values["base_mitigation_stats_bonus"]
            .as_f64()
            .unwrap(),
        13.5,
    );
    close(
        rows[1].values["base_mitigation_stats_bonus"]
            .as_f64()
            .unwrap(),
        13.0,
    );
}

#[test]
fn native_integer_defense_matches_the_pike_only_control_curve() {
    let observed: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/recorded_fights/realta_defense_controls_20261023_observed.json"
    ))
    .unwrap();
    let hit = &observed["controls"][0]["incoming"][0];
    let raw = reconstructed_regular_damage(hit).round();
    assert_eq!(raw, 2411.0);
    let result = fight_with_rounding(&native_crew(), ShipType::Explorer, false, true);
    for row in counters(&result) {
        for (key, expected) in [
            ("effective_armor", 125.0),
            ("effective_shield_deflection", 963.0),
            ("effective_dodge", 42.0),
        ] {
            close(row.values[key].as_f64().unwrap(), expected);
        }
        // Game journal mitigation 876.269782228722 / nearest integer raw 2411.
        // The independent native getter rounds points before this curve.
        close(
            row.values["base_stat_mitigation"].as_f64().unwrap(),
            hit["mitigated_damage"].as_f64().unwrap() / raw,
        );
    }
}

fn reconstructed_regular_damage(hit: &serde_json::Value) -> f64 {
    [
        "hull_damage",
        "shield_damage",
        "mitigated_damage",
        "mitigated_apex_barrier",
    ]
    .into_iter()
    .map(|key| hit[key].as_f64().unwrap())
    .sum::<f64>()
        - hit["isolytic_damage"].as_f64().unwrap()
}

#[test]
fn patrol_control_preserves_observed_cap_and_previous_round_repair() {
    // This is an accounting check on observations, not a Tom simulation pass.
    let observed: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/recorded_fights/realta_defense_controls_20261023_observed.json"
    ))
    .unwrap();
    let control = &observed["controls"][1];
    let hits = control["incoming"].as_array().unwrap();
    assert_eq!(hits.len(), 5);
    for hit in hits {
        let raw = reconstructed_regular_damage(hit).round();
        close(hit["mitigated_damage"].as_f64().unwrap() / raw, 0.712);
    }
    let previous_round_hull: f64 = hits
        .iter()
        .filter(|hit| hit["round"] == 1)
        .map(|hit| hit["hull_damage"].as_f64().unwrap())
        .sum();
    assert_eq!(control["hull_repairs"][0]["round"], 2);
    close(
        control["hull_repairs"][0]["amount"].as_f64().unwrap(),
        previous_round_hull * 0.08,
    );
}

#[test]
fn minimal_crew_observation_preserves_seven_round_defense_and_damage_accounting() {
    // This validates decoded observations, not Tom's source/placement in the
    // simulator. In particular, the shared mitigation cap cannot establish his
    // runtime Defense source by itself.
    let observed: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/recorded_fights/realta_defense_controls_20261023_observed.json"
    ))
    .unwrap();
    let control = observed["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["label"] == "jadzia_captain_tom_below_romulan_patrol_31")
        .unwrap();
    assert_eq!(control["rounds"], 7);
    let damage_events = control["damage_events"].as_array().unwrap();
    let hits = control["incoming"].as_array().unwrap();
    let repairs = control["hull_repairs"].as_array().unwrap();
    let charges = control["charge_events"].as_array().unwrap();
    assert_eq!(damage_events.len(), 31);
    assert_eq!(hits.len(), 17);
    assert_eq!(
        damage_events
            .iter()
            .filter(|event| event["attacker"] == "player")
            .count(),
        14
    );
    assert_eq!(
        damage_events
            .iter()
            .filter(|event| event["attacker"] == "hostile")
            .count(),
        hits.len()
    );
    assert_eq!(charges.len(), 3);
    assert_eq!(repairs.len(), 6);

    for (index, expected_hits) in [3, 2, 3, 2, 3, 2, 2].into_iter().enumerate() {
        let round = index as u64 + 1;
        assert_eq!(
            hits.iter()
                .filter(|hit| hit["round"].as_u64() == Some(round))
                .count(),
            expected_hits,
            "round {round} must retain the actual damage-hit cadence"
        );
    }
    for hit in hits {
        assert_eq!(hit["shot_hit"], 1);
        assert_eq!(hit["is_critical"], 0);
        assert!(damage_events.iter().any(|event| {
            event["attacker"] == "hostile"
                && event["source_index"] == hit["source_index"]
                && event["round"] == hit["round"]
                && event["weapon_id"] == hit["weapon_id"]
        }));
        let raw = reconstructed_regular_damage(hit).round();
        // Raw weapon damage is not a direct journal field. These rounded
        // reconstructions consistently preserve 71.2% across all seven rounds.
        close(hit["mitigated_damage"].as_f64().unwrap() / raw, 0.712);

        let hull = hit["hull_damage"].as_f64().unwrap();
        let shield = hit["shield_damage"].as_f64().unwrap();
        let shield_share = if hit["round"].as_u64().unwrap() <= 2 {
            0.825
        } else {
            0.8
        };
        // The native rank-five first-two-round research adds 0.025. Integer
        // health components can differ by less than one point after splitting.
        assert!(
            (shield - (hull + shield) * shield_share).abs() <= 1.0,
            "round {} shield share must be {shield_share}",
            hit["round"]
        );
    }

    for (index, repair) in repairs.iter().enumerate() {
        let round = index as u64 + 2;
        assert_eq!(repair["round"].as_u64(), Some(round));
        let previous_round_hull: f64 = hits
            .iter()
            .filter(|hit| hit["round"].as_u64() == Some(round - 1))
            .map(|hit| hit["hull_damage"].as_f64().unwrap())
            .sum();
        close(
            repair["amount"].as_f64().unwrap(),
            previous_round_hull * 0.08,
        );
    }
    for (index, charge) in charges.iter().enumerate() {
        assert_eq!(charge["round"].as_u64(), Some(index as u64 * 2 + 2));
        assert_eq!(charge["weapon_id"], 3784560438u64);
        close(charge["charge_percentage"].as_f64().unwrap(), 1.0);
        assert!(charge.get("hull_damage").is_none());
        assert!(charge.get("shield_damage").is_none());
        assert!(damage_events
            .iter()
            .all(|event| event["source_index"] != charge["source_index"]));
    }
}
