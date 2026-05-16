# Roadmap

Planned features and priorities for Kobayashi.

## Known issues

- **x86_64 AVX2 damage kernel produces zero damage.** Two drift fixtures in [`tests/calibration_drift_tests.rs`](../tests/calibration_drift_tests.rs) — `drift_harness_all_fixtures_within_bands` (via `drift_conqueror_borg_beam_suppressed.json`) and `drift_research_weapon_damage_pool_orders_below_layered_total_damage` — fail on x86_64 CI with the attacker dealing **zero damage**. Both pass byte-identical on aarch64 locally on `main` and on the combat-cleanup stack. `simulate_drift_fixture` ([`src/calibration/drift.rs`](../src/calibration/drift.rs)) runs with `TraceMode::Off`, which auto-enables the AVX2 batch kernel (`avx2_supported() && !trace.is_enabled()` at [`src/combat/engine.rs`](../src/combat/engine.rs) `use_experimental_simd_damage_after_apex_base`). Suspected source: [`compute_damage_after_apex_batch`](../src/combat/simd_damage_kernel.rs) and the caller-side batch population in `engine.rs`. Latent since the drift fixtures landed 2026-04-30 ([5310d1d2](https://github.com/pggpgg/kobayashi-stfc/commit/5310d1d2)); masked by 2+ months of consecutive CI fmt/clippy failures (last green CI: 2026-03-04). Affects production users running x86_64. Needs an x86_64 dev environment (or CI-push diagnostic loop) to reproduce and fix.
