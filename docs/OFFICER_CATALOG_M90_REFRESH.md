# Scoping: refresh the canonical officer catalog m86 → m90

**Status:** scoped, not started. **Recommendation:** run as a dedicated, calibration-gated task (see §Risk) — not bundled with feature work.

## Why

`data/officers/officers.canonical.json` is at `m86-04763f1697f5` (imported 2026-02-19). The community cheat-sheet / eligibility matrix is now `m90`. Refreshing the canonical catalog would:

- Add officers that exist in m90 but not our catalog — notably **V'Ger Ilia** (`source_officer_id 3662990708`), the last remaining eligibility *orphan* (in the sheet, not in our catalog), plus any other post-m86 additions.
- Bring officer **stats and abilities** current with m90.

It is **not** required for the eligibility feature: that already works against the m90 cheat-sheet, and the only gap (Chancellor Ake, Deidamia, V'Ger Ilia) degrades gracefully to the heuristic fallback. So this is valuable maintenance, not urgent.

## Key fact that makes this non-trivial

`officers.canonical.json` is **maintainer-curated**, not auto-generated from upstream (CLAUDE.md: "the maintainer-curated catalog"). It carries hand-authored condition tokens, slot assignments, scaling, and metadata. A refresh is therefore **curation + pipeline regen**, not a one-command job. `data_version` is set by hand (`generate-lcars` does not touch it).

## Pipeline (verified commands)

```bash
# 1. Fetch upstream officer detail from data.stfc.space (missing-only; --full re-downloads all)
node scripts/fetch_stfcspace_officers.mjs --full
#    -> data/upstream/data-stfc-space/officers/<id>.json, summary-officer.json
#    (last fetched 2026-06-21 — may itself lag m90; verify new officer ids resolve, not 404)

# 2. Curate data/officers/officers.canonical.json (MANUAL — the real work):
#    - add new m90 officers (V'Ger Ilia, …) with abilities/conditions/scaling
#    - update changed stats / ability values / conditions for existing officers
#    - bump "data_version" -> "m90-<hash>" and "imported_at"

# 3. Normalize ids + sync below-decks slots from upstream detail
python3 scripts/normalize_officer_id_strings.py

# 4. Regenerate the LCARS combat monolith from canonical
cargo run --bin generate_lcars            # (or: cargo xtask regen-lcars)

# 5. Validate — STRICT fails on unmapped canonical condition tokens
cargo run --bin validate_data -- --strict

# 6. Calibration (see Risk) + full suite
cargo test
```

Downstream of the catalog: `officers.lcars.yaml` (combat source, built in-process at startup), `id_registry.json`, `name_aliases.json`, and `officer_modeling_fidelity.yaml` (hand-maintained notes — review per changed officer; regenerate the scorecard with `cargo run --bin generate_officer_scorecard`).

## The delta to curate (do this first)

Before any edits, enumerate exactly what changed so the work is bounded:

1. **New officers** = officers in the m90 cheat-sheet (`source_officer_id`) not present in `officers.canonical.json`. The importer's coverage report already surfaces these as *orphans* (currently just V'Ger Ilia, `3662990708`). Cross-check against `data/upstream/data-stfc-space/summary-officer.json` to confirm upstream has them.
2. **Changed officers** = abilities/conditions/values that differ m86→m90. The cheat-sheet `MasterOfficers`/`RawOfficers` CSVs are the human-readable reference for intended wording.
3. **New condition tokens** = any canonical condition strings on new officers not yet mapped in `src/lcars/canonical_conditions.rs` (caught by `validate_data --strict`).

## Risk: calibration is the gate

Officer stats and ability effects feed the combat engine, which is covered by a sigma-band calibration suite:
`tests/calibration_drift_tests.rs`, `calibration_scoreboard_tests.rs`, `officer_stat_calibration_anchors.rs`, `recorded_fight_calibration_tests.rs`, `mitigation_feedback_calibration_tests.rs`, plus per-fight log calibration tests.

Any stat/ability change to an officer that appears in a recorded/anchored fixture can push that fixture out of band and **fail the suite**. Remediation is either re-anchoring fixtures (needs fresh recorded fights) or justified band widening — i.e. exactly the work that the **calibration snapshot-freeze + fight-recording sitting** enables. **Sequence this refresh with that sitting**, and treat "full calibration suite green" as the merge gate. Adding *new* officers (no existing fixtures) is lower-risk than editing *existing* officers' stats.

## Suggested phasing

1. **Baseline** — confirm the calibration suite is green on `main` first (so any post-refresh failure is attributable to the refresh).
2. **Enumerate the delta** (§above) — produces a bounded checklist.
3. **Additive-first** — add the new officers (V'Ger Ilia, …) without touching existing officers' stats; regen LCARS; `validate_data --strict`; run calibration. This closes the orphan with minimal calibration risk.
4. **Stat updates** (higher risk) — apply m90 stat/ability changes to existing officers; rerun calibration; re-anchor or justify any drift during the recording sitting.
5. **Finalize** — bump `data_version`/`imported_at`, refresh `officer_modeling_fidelity.yaml` + scorecard, update docs.

## Not in scope here

- No CI auto-refresh for officers today (`.github/workflows/data-refresh.yml` covers ships/hostiles/research, not officers). Adding officers to it is a separate decision.
- The eligibility matrix and the canonical catalog version independently; refreshing one does not require regenerating the other (they join by `ability_id`).
