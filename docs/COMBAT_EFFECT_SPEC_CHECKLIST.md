# CombatEffectSpec Implementation Checklist

This checklist turns the draft in [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) into concrete implementation steps with file-level targets.

> **Status:** Phases 1–6 are **implemented**; research attack-phase seats use **only** the CombatEffectSpec adapter (legacy inline builder removed). Optional future work: further LCARS resolver cutover behind parity.

## Phase 0 - Guardrails and scope lock

- [x] Add a short "Phase 1 scope" section to [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) (Phase 1 delivery scope + pointer to roadmap non-goals).
- [x] Diagnostics / tooling: `combat_effect_spec_enabled()` is true unless `KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE=0` / `false` / `no` (research seats always use the adapter in code).
- [x] Explicit Phase 1 non-goals in [docs/ROADMAP.md](ROADMAP.md) (`### Phase 1 non-goals`).

## Phase 1 - Canonical IR types and serde

### New files

- [x] [src/data/combat_effect_spec.rs](../src/data/combat_effect_spec.rs) — IR enums/structs + serde + `combat_effect_spec_enabled` / `combat_effect_spec_debug_http_enabled`
- [x] [src/data/combat_effect_spec_validate.rs](../src/data/combat_effect_spec_validate.rs) — structural + semantic validation

### Existing files to update

- [x] Exported from [src/data/mod.rs](../src/data/mod.rs); [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) references schema

### Acceptance criteria

- [x] Serde / validation tests in-module (`combat_effect_spec.rs`, `combat_effect_spec_validate.rs`)

## Phase 2 - Compiler (spec -> engine runtime)

### New files

- [x] [src/combat/effect_spec_compile.rs](../src/combat/effect_spec_compile.rs) — `compile_trigger`, `compile_condition`, `compile_research_attack_phase_spec`, …

### Existing files to update

- [x] Wired in [src/combat/mod.rs](../src/combat/mod.rs); uses [src/combat/condition.rs](../src/combat/condition.rs) helpers

### Acceptance criteria

- [x] Unit tests in `effect_spec_compile.rs` + integration parity suites

## Phase 3 - Research adapter (catalog -> spec)

### New files

- [x] [src/data/research_effect_spec_adapter.rs](../src/data/research_effect_spec_adapter.rs)

### Existing files to update

- [x] [src/data/profile.rs](../src/data/profile.rs) — `research_derived_attack_phase_seats` delegates to `research_derived_attack_phase_seats_from_spec`
- [x] [src/data/research.rs](../src/data/research.rs) documents adapter path

### Tests

- [x] [tests/research_profile_merge_tests.rs](../tests/research_profile_merge_tests.rs), [tests/combat_effect_spec_research_parity_tests.rs](../tests/combat_effect_spec_research_parity_tests.rs), adapter unit tests in `research_effect_spec_adapter.rs`

### Acceptance criteria

- [x] Parity proven for golden fixtures ([tests/combat_effect_spec_research_parity_tests.rs](../tests/combat_effect_spec_research_parity_tests.rs))

## Phase 4 - LCARS adapter (LCARS -> spec)

### New files

- [x] [src/lcars/effect_spec_adapter.rs](../src/lcars/effect_spec_adapter.rs)

### Existing files to update

- [x] Runtime combat still uses [src/lcars/resolver.rs](../src/lcars/resolver.rs); spec adapter used for IR export, parity tests, optional HTTP debug

### Tests

- [x] Unit tests in `effect_spec_adapter.rs`; [tests/lcars_combat_effect_spec_parity_tests.rs](../tests/lcars_combat_effect_spec_parity_tests.rs)

### Acceptance criteria

- [x] Parity harness for representative rows; full resolver cutover remains optional

## Phase 5 - Optional stfc.cc ingestion adapter

### New files

- [x] Create [src/data/stfc_cc_effect_spec_adapter.rs](../src/data/stfc_cc_effect_spec_adapter.rs)
  - Parse stfc.cc columns (`AbilityModifier`, `AbilityConditions`, `AbilityTrigger`, etc.)
  - Map to canonical tokens
  - Emit explicit "unmapped token" diagnostics

### Existing files to update

- [x] Add a non-default import utility (bin or script) entrypoint:
  - [src/bin/stfc_cc_cheat_sheet_report.rs](../src/bin/stfc_cc_cheat_sheet_report.rs)
- [x] Keep this ingestion path optional and not required by core simulation

### Acceptance criteria

- [x] Can convert sample rows from [data/upstream/cheat-sheet/raw-officers-m88-17rc.csv](../data/upstream/cheat-sheet/raw-officers-m88-17rc.csv)
- [x] Reports unmapped tokens with stable diagnostics (`unmapped_*:` prefixes)
- [x] Bundled cheat sheet: **561/561** rows full-convert (`scan_stfc_cc_cheat_sheet_csv`); unit test locks `rows_full_convert == rows_total`
- [x] `EnemyHullFaction` + `AbilityAttributes` (`faction_id=…`) → `DefenderHullFactionIdIs`; attributes merged under `CombatEffectSpec.attributes["stfc_cc_ability_attributes"]`
- [x] Deferred upstream condition tokens → `AbilityConditionSpec::StfcCcToken` (not compilable to `AbilityCondition` until modeled in engine)
- [x] Composite `OfficerStatAll` → `AbilityModifierSpec::TagOnly` (aligned with `generate_lcars` multi-stat bucket)
- [x] CLI [`src/bin/stfc_cc_cheat_sheet_report.rs`](../src/bin/stfc_cc_cheat_sheet_report.rs): `--json` output; human mode prints `coverage: full` when complete

## Phase 6 - Parity harness and cutover

### Existing files to update

- [x] Add golden parity tests under [tests/](../tests/)
  - [x] Research: [tests/combat_effect_spec_research_parity_tests.rs](../tests/combat_effect_spec_research_parity_tests.rs) — `research_derived_attack_phase_seats` vs `research_derived_attack_phase_seats_from_spec` (order-independent seat signatures)
  - [x] LCARS: [tests/lcars_combat_effect_spec_parity_tests.rs](../tests/lcars_combat_effect_spec_parity_tests.rs) — `resolve_lcars_condition` vs `compile_condition(lcars_condition_to_spec)`; `compile_trigger` + `resolve_officer_ability` timing; scalar/effect alignment for representative `stat_modify` rows
  - [x] Mixed: [tests/mixed_crew_research_combat_effect_spec_parity_tests.rs](../tests/mixed_crew_research_combat_effect_spec_parity_tests.rs) — LCARS bridge + conditional research merged with adapter-aligned research seats
- [x] Optional HTTP debug: `GET /api/debug/combat-effect-spec/officers/:id` when `KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1` (and LCARS data loaded with `KOBAYASHI_OFFICER_SOURCE=lcars`); returns JSON with per-effect optional [`CombatEffectSpec`](../src/data/combat_effect_spec.rs) rows; **404** when debug is off (see [`src/server/routes.rs`](../src/server/routes.rs)); documented in [docs/openapi/kobayashi-heavy-payloads.yaml](../docs/openapi/kobayashi-heavy-payloads.yaml)

### CI requirements

- [x] Parity harness covered by `cargo test` in CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml))

### Default flip

- [x] Research attack-phase seats use CombatEffectSpec adapter only (`research_derived_attack_phase_seats` → `research_derived_attack_phase_seats_from_spec`)
- [x] Legacy inline builder and `KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE` removed

## Deliverables by milestone

### Milestone A (research parity only)

- [x] Phase 1 + 2 + 3 complete (in-tree)
- [x] NS Burning Damage and conditional research parity — [tests/combat_effect_spec_research_parity_tests.rs](../tests/combat_effect_spec_research_parity_tests.rs), [tests/research_profile_merge_tests.rs](../tests/research_profile_merge_tests.rs)

### Milestone B (officer parity)

- [x] Phase 4 complete (LCARS → spec adapter)
- [x] LCARS fixture parity — [tests/lcars_combat_effect_spec_parity_tests.rs](../tests/lcars_combat_effect_spec_parity_tests.rs), [tests/mixed_crew_research_combat_effect_spec_parity_tests.rs](../tests/mixed_crew_research_combat_effect_spec_parity_tests.rs)

### Milestone C (ecosystem hardening)

- [x] Phase 5 complete (stfc.cc adapter; bundled CSV 561/561)
- [x] Phase 6 complete (golden parity harness, optional debug HTTP, OpenAPI path for debug)
- [x] Default flip — spec-only research seats; legacy builder removed

## Tracking notes template

Use this snippet in PRs/issues:

```md
## CombatEffectSpec checklist
- Phase: <1|2|3|4|5|6>
- Flag default: on (spec path)
- Legacy research seat builder: removed
- Parity tests added:
  - [x] Research
  - [x] LCARS
  - [x] Mixed scenarios
- Known gaps:
  - ...
```
