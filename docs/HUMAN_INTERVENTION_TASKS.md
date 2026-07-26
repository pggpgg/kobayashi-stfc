# Tasks requiring human intervention

Work the simulator or data pipeline cannot complete automatically: judgment, upstream correlation, or in-game verification.

## Factions and hostiles

1. **Mapping of hostiles to factions using `translations-factions.json`** — **ongoing in code; see `src/data/hostile.rs`**
  `HostileRecord::opponent_faction_tag()` maps high-volume `faction.id` values from `summary-hostile` and `faction.loca_id` rows that match `translations-factions.json` `faction_name` (e.g. Texas-class → Federation, “Card” → Cardassian, Borg alt loca, V’Ger Clone → Borg). **Intentionally `Unknown`:** Q-Continuum, Exiles, Node, Maverick, Orion, Eclipse, Krenim, Apex Raiders, Transogen, Aggregation (no `OpponentFactionTag` / not modeled for hull faction gates). When the game adds a new `faction.id`, extend the match arms in `hostile.rs` and add a unit test.

## Ship ability catalog (heuristic)

1. **Review `scripts/generate_full_ship_ability_catalog.py` classifications**
  Ability text that is ambiguous, conditional on mechanics we do not model, or uses non-standard wording may need manual catalog rows or script rule updates after regeneration.

## Recorded fights and CLI

1. **Defender faction for imports** — **shipped for TSV fight exports (2026-06-14).**
  - **CLI `simulate`:** supply faction explicitly with `--defender-faction <slug>`, or let `--hostile <id|name level>` derive it. Resolved by `defender_faction_for_cli_simulate` (`src/data/loader.rs`) and passed to `simulate_combat_with_defender_faction`.
  - **Calibration drift fixtures:** set `simulation.defender_faction` (slug) — and optionally `simulation.defender_hull_faction_id` — in a `drift_*.json` fixture. Validated at load and threaded through `src/calibration/drift.rs` so `AbilityCondition::DefenderFactionIs` gating fires (see `drift_faction_gated_attack_multiplier.json`).
  - **Recorded-fight import (TSV):** `parse_fight_export` captures enemy summary `Player Name` + `Ship Level`. `defender_faction_for_fight_export(export, optional_slug_override)` resolves display name → hostile id → `opponent_faction_tag()` (override slug wins). See `docs/combat_log_format.md`. **Note:** some hostiles (e.g. Takret Militia) resolve to a bundled id but upstream `faction.id = -1`, so the tag stays `Unknown` until `hostile.rs` faction mapping is extended — that remains a data maintenance item under “Mapping of hostiles to factions” above.

## Calibration

1. **Snapshot freeze + curated fight suite** — needs the maintainer, one uninterrupted sitting.
  Snapshot the full game state into a Kobayashi profile, freeze all progression (no tiering, leveling, research, or building), record a varied curated set of in-game fights, and export them as the test suite for that snapshot. **Not on the roadmap** — see [NOT_ROADMAP.md](NOT_ROADMAP.md) § Snapshot-bound calibration; protocol and rationale (avoiding overfit to mixed-vintage fight records) are documented there. Scheduling constraint: the freeze blocks STFC event participation, so the window must be chosen deliberately — until then, calibration work proceeds only on synthetic drift fixtures.

## Research / CI environment

1. **Weekly data refresh cannot open its PR** — needs a GitHub settings toggle only the account owner can flip.
  `.github/workflows/data-refresh.yml` (Mondays 06:00 UTC) has failed at its final step on every run since 2026-06-15. Everything before it succeeds — fetch, normalize, `cargo test`, strict validation — and the branch **is** pushed; only PR creation is rejected:

  ```text
  ##[error]GitHub Actions is not permitted to create or approve pull requests.
  ```

  Because `peter-evans/create-pull-request` never reaches its `delete-branch: true` cleanup, each failed run strands an `automated/data-refresh-<run_id>` branch on the remote.

  **Fix:** enable *Allow GitHub Actions to create and approve pull requests* under **Settings → Actions → General → Workflow permissions**. The repo-level API already reports `can_approve_pull_request_reviews: true`, so the remaining block is the **account-level** setting at <https://github.com/settings/actions>. Confirm with the next Monday run, or trigger `workflow_dispatch` manually.

  Until it is fixed, refresh data by hand — `cargo xtask data-refresh -- --stfcspace` — as on `claude/upstream-data-refresh-2026-07-24`.

2. **Broad research catalog for integration tests**
  `tests/scenario_research_integration_tests.rs` expects a populated `data/research_catalog.json` (see test message / `scripts/import_stfcspace_research.mjs`). Filling or refreshing that data is an operational/data task, not a code change.

---

*Add new bullets here when you discover work that needs a person, not just a patch.*