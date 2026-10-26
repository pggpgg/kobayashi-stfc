//! Validate physical accounting and provenance for the fresh Jadzia/Mirror captain swap.
//! These measurements do not assert a simulator formula.

use serde_json::Value;
use sha2::{Digest, Sha256};

const JADZIA: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_jadzia_tom_20261026_observed.json");
const MIRROR: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_mirror_tom_20261026_observed.json");
const OLD_TOM: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_tom_20261024_observed.json");
const JADZIA_MANIFEST: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_jadzia_tom_20261026_replay.json");
const MIRROR_MANIFEST: &str =
    include_str!("fixtures/recorded_fights/realta_jemhadar_mirror_tom_20261026_replay.json");
const JADZIA_BYTES: &[u8] =
    include_bytes!("fixtures/recorded_fights/realta_jemhadar_jadzia_tom_20261026_observed.json");
const MIRROR_BYTES: &[u8] =
    include_bytes!("fixtures/recorded_fights/realta_jemhadar_mirror_tom_20261026_observed.json");

fn value(json: &str) -> Value {
    serde_json::from_str(json).expect("public recorded-fight fixture should parse")
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("numeric fixture field")
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-12, "{a} != {b}");
}

fn close_approx(a: f64, b: f64) {
    assert!((a - b).abs() <= 5e-10, "{a} != approximately {b}");
}

fn interval(v: &Value) -> [f64; 2] {
    [num(&v[0]), num(&v[1])]
}

fn disjoint(a: [f64; 2], b: [f64; 2]) -> bool {
    a[1] < b[0] || b[1] < a[0]
}

fn verify_manifest(m: &Value, f: &Value, bytes: &[u8], id: &str, captain: u64) {
    assert_eq!(m["id"], id);
    assert_eq!(
        m["snapshot_files"]["battlelogs.imported.json"],
        f["source_json_sha256"]
    );
    let obs = &m["observation"];
    assert_eq!(obs["path"], format!("{id}_observed.json"));
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(obs["sha256"], digest);
    let journal = &m["journals"][0];
    assert_eq!(journal["battle_time"], f["battle_time"]);
    for (path, expected) in [
        (&journal["player"]["captain"], captain),
        (&journal["player"]["below_decks"][0], 3_849_424_588),
    ] {
        assert_eq!(num(&path["officer_id"]), expected as f64);
        let source = f["setup"]["player"]["crew"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| num(&row["public_officer_id"]) == expected as f64)
            .unwrap();
        assert_eq!(path["rank"], source["rank"]);
        assert_eq!(path["level"], source["level"]);
    }
}

fn verify_case(f: &Value, captain_name: &str) -> [f64; 2] {
    for flag in [
        "all_combat_setup_checks_pass",
        "player_started_full_hull_and_shield",
        "target_started_full_hull_and_shield",
        "paired_complete_ship_maps_and_target_setup_identical",
        "paired_player_deployed_buff_signatures_identical",
        "paired_source_files_identical",
    ] {
        assert_eq!(f["setup_check_summary"][flag], true, "{flag}");
    }
    assert_eq!(
        f["setup_check_summary"]["paired_expiry_boundary_crossed"],
        false
    );
    for side in ["player", "target"] {
        for stat in ["hull", "shield"] {
            close(
                num(&f["setup"][side][format!("initial_{stat}")]),
                num(&f["setup"][side][format!("maximum_{stat}")]),
            );
        }
    }

    let player = &f["setup"]["player"];
    assert_eq!(player["captain"], captain_name);
    let crew = player["crew"].as_array().unwrap();
    assert_eq!(crew.len(), 2);
    let officer_at = |slot| crew.iter().find(|o| o["slot"] == slot).unwrap();
    let captain = officer_at(0);
    let captain_id = if captain_name == "Jadzia Dax" {
        1_916_661_181
    } else {
        3_598_147_546_u64
    };
    assert_eq!(num(&captain["public_officer_id"]), captain_id as f64);
    assert_eq!(num(&captain["rank"]), 1.0);
    assert_eq!(
        num(&captain["level"]),
        if captain_name == "Jadzia Dax" {
            5.0
        } else {
            1.0
        }
    );
    let tom = officer_at(3);
    assert_eq!(num(&tom["public_officer_id"]), 3_849_424_588.0);
    assert_eq!((num(&tom["rank"]), num(&tom["level"])), (4.0, 20.0));

    let o = &f["observed"];
    assert_eq!(num(&o["published_incoming_hit_count"]), 4.0);
    close(num(&o["diagnostic_denominator_error_budget"]), 2.01);

    let strong = &o["stronger_weapon"];
    assert_eq!(num(&strong["weapon_id"]), 2_806_302_352.0);
    assert_eq!(
        (
            num(&strong["clean_noncritical_count"]),
            num(&strong["clean_critical_count"])
        ),
        (4.0, 0.0)
    );
    assert!(strong["clean_critical_interval"].is_null());
    assert_eq!(num(&strong["primary_combined_count"]), 4.0);
    let hits = o["incoming_hits"].as_array().unwrap();
    assert_eq!(hits.len(), 4);
    let budget = num(&o["diagnostic_denominator_error_budget"]);
    let mut joint = [0.0_f64, f64::INFINITY];
    for hit in hits {
        assert_eq!(
            (
                num(&hit["round"]),
                num(&hit["weapon_id"]),
                num(&hit["shot_hit"]),
                num(&hit["is_critical"])
            ),
            (1.0, num(&strong["weapon_id"]), 1.0, 0.0)
        );
        assert!(num(&hit["remaining_hull"]) > 0.0 && num(&hit["remaining_shield"]) > 0.0);
        for channel in [
            "isolytic_damage",
            "mitigated_isolytic_damage",
            "mitigated_critical_damage",
            "mitigated_isolytic_critical_damage",
        ] {
            close(num(&hit[channel]), 0.0);
        }
        let d = num(&hit["hull_damage"])
            + num(&hit["shield_damage"])
            + num(&hit["mitigated_damage"])
            + num(&hit["mitigated_apex_barrier"]);
        assert!(d > budget);
        joint[0] = joint[0].max(num(&hit["mitigated_damage"]) / (d + budget));
        joint[1] = joint[1].min(num(&hit["mitigated_damage"]) / (d - budget));
    }
    assert!(joint[0] <= joint[1]);
    let recorded = interval(&strong["primary_combined_interval"]);
    close(joint[0], recorded[0]);
    close(joint[1], recorded[1]);
    joint
}

#[test]
fn fresh_captain_swap_fixtures_reconstruct_clean_intervals_and_setup() {
    let jadzia = value(JADZIA);
    let mirror = value(MIRROR);
    let jadzia_manifest = value(JADZIA_MANIFEST);
    let mirror_manifest = value(MIRROR_MANIFEST);
    for (m, f, bytes, id, officer) in [
        (
            &jadzia_manifest,
            &jadzia,
            JADZIA_BYTES,
            "realta_jemhadar_jadzia_tom_20261026",
            1_916_661_181,
        ),
        (
            &mirror_manifest,
            &mirror,
            MIRROR_BYTES,
            "realta_jemhadar_mirror_tom_20261026",
            3_598_147_546,
        ),
    ] {
        verify_manifest(m, f, bytes, id, officer);
    }
    let ji = verify_case(&jadzia, "Jadzia Dax");
    let mi = verify_case(&mirror, "Mirror Troi");
    assert_eq!(jadzia["battle_time"], "2026-10-26T03:31:48");
    assert_eq!(mirror["battle_time"], "2026-10-26T05:35:45");
    assert_eq!(jadzia["paired_battle_time"], mirror["battle_time"]);
    assert_eq!(mirror["paired_battle_time"], jadzia["battle_time"]);
    let comparison = &jadzia["paired_comparison"];
    let delta = [mi[0] - ji[1], mi[1] - ji[0]];
    let registered = interval(&comparison["fraction_interval"]);
    close(delta[0], registered[0]);
    close(delta[1], registered[1]);
    let percentage_points = interval(&comparison["percentage_point_interval"]);
    close(percentage_points[0], delta[0] * 100.0);
    close(percentage_points[1], delta[1] * 100.0);
    assert_eq!(comparison["direction"], "Mirror minus Jadzia");
    assert_eq!(comparison["zero_delta_compatible"], false);
    for (index, expected_id) in [
        "sum_individual_effective_before_percentage_pools",
        "raw_crew_times_Tom_context_before_percentage_pools",
    ]
    .iter()
    .enumerate()
    {
        let h = &comparison["frozen_hypotheses"][index];
        assert_eq!(h["id"], *expected_id);
        let expected_delta = num(&h["Mirror_prediction"]) - num(&h["old_Jadzia_prediction"]);
        close_approx(num(&h["expected_delta_approximately"]), expected_delta);
        assert_eq!(
            h["registered_delta_compatible"],
            expected_delta >= registered[0] && expected_delta <= registered[1]
        );
        let jprediction = num(&h["old_Jadzia_prediction"]);
        let mprediction = num(&h["Mirror_prediction"]);
        let ji_recorded =
            interval(&jadzia["observed"]["stronger_weapon"]["primary_combined_interval"]);
        let mi_recorded =
            interval(&mirror["observed"]["stronger_weapon"]["primary_combined_interval"]);
        assert_eq!(
            h["jadzia_absolute_compatible"],
            jprediction >= ji_recorded[0] && jprediction <= ji_recorded[1]
        );
        assert_eq!(
            h["mirror_absolute_compatible"],
            mprediction >= mi_recorded[0] && mprediction <= mi_recorded[1]
        );
    }
    let old = value(OLD_TOM);
    let old_interval = interval(&old["observed"]["stronger_weapon"]["primary_combined_interval"]);
    assert!(disjoint(ji, old_interval));
    assert!(disjoint(mi, old_interval));
}
