# Documentation

This directory holds the reference documents and the design documents of KOBAYASHI. The
main [README](../README.md) is the front page. The documents here give more detail.

## Getting started

- [SYNC.md](SYNC.md) — How to get your roster from the game in near-real time with the STFC
  Community Mod.
- [DEPLOYMENT_SECURITY.md](DEPLOYMENT_SECURITY.md) — How to make the server available
  outside the loopback address. It describes the optional API key, the sync tokens, the
  loopback trust, and other protections.
- [workflows.html](workflows.html) — Interactive workflow diagrams. This page shows the
  data in `workflows.json`.
- [STYLE_STE100.md](STYLE_STE100.md) — The documentation style guide (ASD-STE100
  Simplified Technical English).

## Design and architecture

- [DESIGN.md](DESIGN.md) — The primary design document: the combat model, the LCARS
  grammar, the optimizer, the profiles, and the support buffs.
- [CREW_OPTIMIZATION_METHODS.md](CREW_OPTIMIZATION_METHODS.md) — Practical search methods
  for the large crew spaces of STFC.
- [OPTIMIZER_AMBITIOUS_ROADMAP.md](OPTIMIZER_AMBITIOUS_ROADMAP.md) — A roadmap in stages
  for the next optimizer portfolio of Kobayashi.
- [KOBAYASHI_MOONSHOT_ROADMAP.md](KOBAYASHI_MOONSHOT_ROADMAP.md) — A long-term roadmap for
  Kobayashi as an autonomous laboratory for STFC combat.
- [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) — How the tool calculates the attack,
  defense, and health statistics of an officer, and how they go into combat.
- [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) — The order of effect resolution, the
  triggers, the conditions, and the rules for composition.
- [COMBAT_TRACE.md](COMBAT_TRACE.md) — The debug trace format for a fight log.
- [PERFORMANCE.md](PERFORMANCE.md) — Benchmark data, optimization methods, and notes on the
  hot path.
- [combinatorics-and-optimization-theory.md](combinatorics-and-optimization-theory.md) —
  The mathematics of the search space behind the optimizer strategies.

## API reference

- [openapi/kobayashi-openapi.yaml](openapi/kobayashi-openapi.yaml) — The OpenAPI 3.1
  specification of the HTTP API. The server also sends it at `/api/openapi.yaml` and
  `/api/openapi.json`.

## Contributing officer data

- [LCARS_CONTRIBUTING.md](LCARS_CONTRIBUTING.md) — The guide to contribute an officer
  definition: the schema, the examples, and the `generate_lcars` operation.
- [OFFICER_TRANSLATIONS_MAPPING.md](OFFICER_TRANSLATIONS_MAPPING.md) — The mapping between
  a buff id and a statistic. The generator uses it to make the LCARS data.
- [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md) — The fidelity audit for
  each officer. To make it again, run `cargo run --bin generate_officer_scorecard`.
- [CANONICAL_CONDITIONS.md](CANONICAL_CONDITIONS.md) — The canonical condition strings that
  the generator writes into the LCARS data.
- **The LCARS coverage report** — The LCARS effects that the YAML-to-IR adapter drops at
  load time. To make it locally, run `cargo run --bin validate_data -- --coverage`. The
  command writes `docs/lcars_coverage_report.md` and `docs/lcars_coverage_report.json`.
  Git ignores both files.

## Data pipeline

- [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md) — Where the upstream data
  comes from on data.stfc.space, and how the project refreshes it.
- [UPSTREAM_HOSTILE_SHIP_TYPES.md](UPSTREAM_HOSTILE_SHIP_TYPES.md) — The taxonomy of the
  hostile types and the mapping to the factions.
- [client_combat_log_mapping.md](client_combat_log_mapping.md) — The mapping between the
  combat log of the game and the format of the simulator.
- [combat_log_format.md](combat_log_format.md) — Detailed notes on how to parse a combat
  log.
- [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) — The ships whose
  modeled abilities have no effect in combat now. Some rows are intentional, and some rows
  are open work.
- [building_gaps.md](building_gaps.md) — The opaque `buff_*` building statistics that need
  work. The rows for the economy and the alliance are on an allowlist, and the report
  excludes them. To make the report again, run
  `cargo run --bin report_building_mapping_gaps`. Refer also to
  `data/buildings/opaque_buff_allowlist.json`.

## Process

- [ROADMAP.md](ROADMAP.md) — The future work and the planning priorities.
- [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) — How to select 30 to 50
  fights that are bound to one snapshot, for calibration. This document is a reference
  only. The work is not on the roadmap. Refer to [NOT_ROADMAP.md](NOT_ROADMAP.md).
- [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md) — How to add one recorded fight to
  the calibration suite.
- [NOT_ROADMAP.md](NOT_ROADMAP.md) — The work that the project will not do.
- [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md) — The maintainer tasks that
  need human judgment, for example data curation and calibration decisions.

---

Refer also to these documents:

- [CLAUDE.md](../CLAUDE.md) — The commands, the environment variables, and the operations
  controls.
- [CONTRIBUTING.md](../CONTRIBUTING.md) — CI, the pre-commit hooks, and the branch
  protection.
- [data/README.md](../data/README.md) — The layout of the ship data, the hostile data, the
  building data, and the research data.
- [scripts/README.md](../scripts/README.md) — The order of the maintenance scripts.
- [profiles/README.md](../profiles/README.md) — The policy on profile sharing.
