# Documentation

Reference and design docs for KOBAYASHI. The main [README](../README.md) is the front page; this directory is the deep end.

## Getting started

- [SYNC.md](SYNC.md) — pull your roster from the game in near-real time via the STFC Community Mod.
- [DEPLOYMENT_SECURITY.md](DEPLOYMENT_SECURITY.md) — exposing the server beyond loopback: optional API key, sync tokens, loopback trust, hardening.
- [workflows.html](workflows.html) — interactive workflow diagrams (rendered companion to `workflows.json`).

## Design & architecture

- [DESIGN.md](DESIGN.md) — the canonical design doc: combat model, LCARS grammar, optimizer, profiles, support buffs.
- [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) — how officer attack/defense/health stats are derived and routed into combat.
- [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) — effect resolution order, triggers, conditions, and composability.
- [COMBAT_TRACE.md](COMBAT_TRACE.md) — debug trace format for fight logs.
- [PERFORMANCE.md](PERFORMANCE.md) — benchmark data, optimization techniques, hot-path notes.
- [combinatorics-and-optimization-theory.md](combinatorics-and-optimization-theory.md) — search-space math behind the optimizer strategies.

## API reference

- [openapi/kobayashi-openapi.yaml](openapi/kobayashi-openapi.yaml) — OpenAPI 3.1 spec for the HTTP API, also served live at `/api/openapi.yaml` and `/api/openapi.json`.

## Contributing officer data

- [LCARS_CONTRIBUTING.md](LCARS_CONTRIBUTING.md) — the officer definition contribution guide: schema, examples, the `generate_lcars` workflow.
- [OFFICER_TRANSLATIONS_MAPPING.md](OFFICER_TRANSLATIONS_MAPPING.md) — buff-id ↔ stat mappings used during LCARS generation.
- [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md) — per-officer fidelity audit; regenerated via `cargo run --bin generate_officer_scorecard`.
- [CANONICAL_CONDITIONS.md](CANONICAL_CONDITIONS.md) — canonical condition strings used in LCARS generation.
- **LCARS coverage report** — LCARS effects the YAML→IR adapter currently drops at load time. Generated locally via `cargo run --bin validate_data -- --coverage` (writes `docs/lcars_coverage_report.{md,json}`; gitignored).

## Data pipeline

- [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md) — upstream data sourcing from data.stfc.space and refresh strategy.
- [UPSTREAM_HOSTILE_SHIP_TYPES.md](UPSTREAM_HOSTILE_SHIP_TYPES.md) — hostile type taxonomy and faction mapping.
- [client_combat_log_mapping.md](client_combat_log_mapping.md) — in-game combat log ↔ simulator format mapping.
- [combat_log_format.md](combat_log_format.md) — detailed combat log parsing notes.
- [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) — ships whose modelled abilities currently have no combat impact (intentional or open work).
- [building_gaps.md](building_gaps.md) — actionable opaque `buff_*` building stats (allowlisted economy/alliance rows excluded); regenerated via `cargo run --bin report_building_mapping_gaps` → see also `data/buildings/opaque_buff_allowlist.json`.

## Process

- [ROADMAP.md](ROADMAP.md) — shipped vs planned features.
- [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) — how to curate 30–50 snapshot-bound fights for calibration (reference; not on roadmap — see [NOT_ROADMAP.md](NOT_ROADMAP.md)).
- [NOT_ROADMAP.md](NOT_ROADMAP.md) — explicit non-goals.
- [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md) — maintainer tasks that require human judgment (data curation, calibration decisions).

---

See also: [CLAUDE.md](../CLAUDE.md) (commands, env vars, ops tuning), [CONTRIBUTING.md](../CONTRIBUTING.md) (CI / pre-commit / branch protection), [data/README.md](../data/README.md) (ship/hostile/building/research data layout), [scripts/README.md](../scripts/README.md) (maintenance script order), [profiles/README.md](../profiles/README.md) (profile sharing policy).
