//! Gorn Eviscerator vs hostile 447012258 (isolytic-vulnerability): is the fight trivial without officers?
//! Log anchor: fight samples/gorn-evisc_vs_447012258_60_easywin_trivial.csv (Pike/Moreau/T'Laan, round-1 win).
//! Full log-vs-sim calibration: `tests/gorn_evisc_vs_447012258_log_calibration.rs` (`gorn_evisc_log_vs_sim_calibration`).

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::profile_index::DEMO_PROFILE_ID;
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{run_monte_carlo_with_registry, DefenderOpponent};

fn run_solo(label: &str, crew: &CrewCandidate) -> (f64, f64, f64) {
    std::env::set_var("KOBAYASHI_OFFICER_SOURCE", "lcars");
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let (results, placeholder) = run_monte_carlo_with_registry(
        registry.as_ref(),
        "gorn_eviscerator",
        "447012258",
        Some(10),
        Some(50),
        std::slice::from_ref(crew),
        5_000,
        42,
        Some(DEMO_PROFILE_ID),
        SupportBuffScenarioRequest::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert!(
        !placeholder,
        "{label}: ship/hostile must resolve from registry"
    );
    let r = &results[0];
    eprintln!(
        "{label}: win_rate={:.4} r1_kill={:.4} avg_hull={:.4}",
        r.win_rate, r.r1_kill_rate, r.avg_hull_remaining,
    );
    (r.win_rate, r.r1_kill_rate, r.avg_hull_remaining)
}

#[test]
fn gorn_evisc_non_combat_captain_alone_is_trivial_win_vs_447012258() {
    // Non-combat captain (Quark), no bridge, no below-decks — ship hull ability + demo profile + Enaran chaos only.
    let solo = CrewCandidate {
        captain: "quark-2fd57b".to_string(),
        bridge: vec![],
        below_decks: vec![],
    };
    let (win_rate, _, hull) = run_solo("quark_solo", &solo);
    assert!(
        win_rate > 0.99,
        "non-combat captain alone should trivially win (got {win_rate})"
    );
    assert!(
        hull > 0.5,
        "expected comfortable hull remaining on wins (got {hull})"
    );
}

#[test]
fn gorn_evisc_empty_crew_is_not_reliable_vs_447012258() {
    let empty = CrewCandidate {
        captain: String::new(),
        bridge: vec![],
        below_decks: vec![],
    };
    let (win_rate, _, _) = run_solo("empty_crew", &empty);
    assert!(
        win_rate < 0.9,
        "no captain at all should not be a trivial guaranteed win (got {win_rate})"
    );
}
