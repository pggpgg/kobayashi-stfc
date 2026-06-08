//! Characterization harness for the officer ability source (LCARS vs the legacy hash-stub).
//!
//! Background: officer abilities resolve via two front-ends selected by `KOBAYASHI_OFFICER_SOURCE`.
//! The LCARS resolver is full-fidelity; the legacy `build_crew_seats` path fabricates hash-derived
//! placeholder buffs for non-state officers. As of the officer-resolution-unify work the **default
//! is LCARS**, and `KOBAYASHI_OFFICER_SOURCE=stub` is a temporary escape hatch into the old path.
//!
//! These tests pin that contract and provide the per-scenario damage baseline that later migration
//! stages (delete the stub; retire the yaml monolith) compare against. Serialized because they
//! mutate the process-global env var around `DataRegistry::load`.

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::crew_generator::CrewCandidate;
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry, DefenderOpponent,
};
use serial_test::serial;
use std::sync::Arc;

/// Save/restore `KOBAYASHI_OFFICER_SOURCE`; `Some(v)` sets it, `None` removes it (the default).
struct OfficerSourceGuard {
    previous: Option<String>,
}

impl OfficerSourceGuard {
    const KEY: &'static str = "KOBAYASHI_OFFICER_SOURCE";

    fn apply(value: Option<&str>) -> Self {
        let previous = std::env::var(Self::KEY).ok();
        match value {
            Some(v) => std::env::set_var(Self::KEY, v),
            None => std::env::remove_var(Self::KEY),
        }
        Self { previous }
    }
}

impl Drop for OfficerSourceGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(p) => std::env::set_var(Self::KEY, p),
            None => std::env::remove_var(Self::KEY),
        }
    }
}

/// A fixed, real reference roster (the Enterprise-D crew also used by the calibration trace).
fn reference_candidate() -> CrewCandidate {
    CrewCandidate {
        captain: "ent-e-picard-556227".to_string(),
        bridge: vec![
            "ent-e-data-871245".to_string(),
            "five-of-eleven-d9aa11".to_string(),
        ],
        below_decks: vec!["harry-kim-a79fdf (T5)".to_string()],
    }
}

/// Load a registry under the given officer source and replay the reference scenario once.
/// `value`: `None` = default (unset env), `Some("lcars")`, `Some("stub")`.
fn replay_under_source(value: Option<&str>) -> (bool, f64, u32) {
    let _guard = OfficerSourceGuard::apply(value);
    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    let lcars_loaded = registry.lcars_officers.is_some();
    let replay = replay_optimize_iteration_with_registry(
        registry.as_ref(),
        "uss_enterprise_d",
        "kobayashi_theoretical_damage_sponge",
        Some(5),
        Some(7),
        &reference_candidate(),
        42,
        0,
        Some(kobayashi::data::profile_index::DEMO_PROFILE_ID),
        0, // no trace events needed; we only compare totals
        None,
        DefenderOpponent::Hostile,
    );
    assert!(
        !replay.using_placeholder_combatants,
        "ship/hostile must resolve from data (source={value:?})"
    );
    (lcars_loaded, replay.total_damage, replay.rounds_simulated)
}

#[test]
#[serial]
fn default_loads_lcars_officers() {
    let registry = {
        let _guard = OfficerSourceGuard::apply(None);
        Arc::new(DataRegistry::load().expect("DataRegistry::load"))
    };
    let officers = registry
        .lcars_officers
        .as_ref()
        .expect("default officer source must load LCARS officers (the full-fidelity path)");
    assert!(
        officers.len() > 100,
        "expected the full LCARS officer set, got {}",
        officers.len()
    );
}

#[test]
#[serial]
fn stub_escape_hatch_disables_lcars() {
    let _guard = OfficerSourceGuard::apply(Some("stub"));
    let registry = Arc::new(DataRegistry::load().expect("DataRegistry::load"));
    assert!(
        registry.lcars_officers.is_none(),
        "KOBAYASHI_OFFICER_SOURCE=stub must disable LCARS (legacy placeholder path)"
    );
}

/// The core characterization: the default path now resolves through LCARS (identical to an explicit
/// `=lcars`), and that is genuinely different from the legacy stub — proving the default flip moved
/// real officers off the hash-placeholder path. Deterministic (fixed seed) so equality is exact.
#[test]
#[serial]
fn default_matches_lcars_and_differs_from_stub() {
    let (default_lcars_loaded, default_dmg, default_rounds) = replay_under_source(None);
    let (explicit_lcars_loaded, lcars_dmg, _) = replay_under_source(Some("lcars"));
    let (stub_lcars_loaded, stub_dmg, _) = replay_under_source(Some("stub"));

    assert!(default_lcars_loaded && explicit_lcars_loaded);
    assert!(!stub_lcars_loaded);

    assert_eq!(
        default_dmg, lcars_dmg,
        "default (env unset) must resolve through LCARS identically to KOBAYASHI_OFFICER_SOURCE=lcars"
    );
    assert!(
        (default_dmg - stub_dmg).abs() > 1.0,
        "LCARS default ({default_dmg}) should differ from the legacy stub ({stub_dmg}); \
         identical totals would mean the flip changed nothing. rounds={default_rounds}"
    );
}
