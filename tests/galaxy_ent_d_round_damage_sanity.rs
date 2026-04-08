//! One-off sanity trace: U.S.S. Enterprise-D Galaxy class hull ability (level-scaled growth) + demo profile.
//! Run: `cargo test galaxy_ent_d_round_damage_sanity_prints_per_round_damage -- --nocapture`

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::loader::resolve_ship_with_tier_level;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::replay_optimize_iteration_with_registry;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

fn as_f64(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .unwrap_or(0.0)
}

/// Human-readable damage totals (e.g. `456K`, `1.2M`, `145B`) instead of scientific notation.
fn format_compact_damage(n: f64) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let n = n.abs();

    if n < 1000.0 {
        return format!("{}{:.0}", sign, n);
    }

    for (div, label) in [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")] {
        if n >= div {
            let x = n / div;
            let body = if x >= 100.0 {
                format!("{:.0}", x)
            } else if x >= 10.0 {
                trim_trailing_zeros(format!("{:.1}", x))
            } else {
                trim_trailing_zeros(format!("{:.2}", x))
            };
            return format!("{}{}{}", sign, body, label);
        }
    }
    format!("{}{:.0}", sign, n)
}

fn trim_trailing_zeros(s: String) -> String {
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_string()
}

#[test]
fn galaxy_ent_d_round_damage_sanity_prints_per_round_damage() {
    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));

    let ship_rec = resolve_ship_with_tier_level("uss_enterprise_d", Some(5), Some(7))
        .expect("resolve uss_enterprise_d tier 5 level 7");
    let hull = ship_rec
        .abilities
        .as_ref()
        .and_then(|a| a.iter().find(|x| x.id == "448699234"))
        .expect("Galaxy class hull ability");
    assert!(
        (hull.value - 0.93).abs() < 1e-9,
        "level 7 should pick values[6]=0.93 from upstream curve, got {}",
        hull.value
    );
    assert!(
        hull.level_scaled_values.is_none(),
        "ShipRecord abilities must be level-resolved (no curve on flat record)"
    );

    let candidate = CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec!["harry-kim-a79fdf (T5)".to_string()],
    };

    let replay = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "uss_enterprise_d",
        "kobayashi_theoretical_damage_sponge",
        Some(5),
        Some(7),
        &candidate,
        42,
        0,
        Some(DEMO_PROFILE_ID),
        2_000_000,
        None,
    );

    assert!(
        !replay.using_placeholder_combatants,
        "ship/hostile must resolve from data"
    );
    assert!(
        replay.rounds_simulated >= 50,
        "expected at least 50 rounds (sponge hull); got {}",
        replay.rounds_simulated
    );

    let mut per_round: BTreeMap<u32, f64> = BTreeMap::new();
    let mut morale_by_round: BTreeMap<u32, bool> = BTreeMap::new();

    for ev in &replay.trace_events {
        if ev.event_type == "damage_application" {
            let sd = ev.values.get("shield_damage").map(as_f64).unwrap_or(0.0);
            let hd = ev.values.get("hull_damage").map(as_f64).unwrap_or(0.0);
            *per_round.entry(ev.round_index).or_insert(0.0) += sd + hd;
        }
        if ev.event_type == "morale_activation" {
            let tr = ev
                .values
                .get("triggered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            morale_by_round.insert(ev.round_index, tr);
        }
    }

    println!(
        "\n=== Enterprise-D T5 L7 vs kobayashi_theoretical_damage_sponge (demo profile) ===\n\
         rounds_simulated={} attacker_won={} total_damage={}\n",
        replay.rounds_simulated,
        replay.attacker_won,
        format_compact_damage(replay.total_damage)
    );
    println!("round\tdmg_this_round\tmorale_proc");
    let mut cumulative = 0.0_f64;
    for r in 0..=replay.rounds_simulated.saturating_sub(1) {
        let dmg = per_round.get(&r).copied().unwrap_or(0.0);
        cumulative += dmg;
        let m = morale_by_round
            .get(&r)
            .map(|b| if *b { "yes" } else { "no" })
            .unwrap_or("-");
        println!(
            "{}\t{}\t{}",
            r,
            format_compact_damage(dmg),
            m
        );
    }
    println!("cumulative_damage_to_defender={}", format_compact_damage(cumulative));
}
