# CombatEffectSpec Implementation Checklist

This checklist turns the draft in [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md) into concrete implementation steps with file-level targets.

## Phase 0 - Guardrails and scope lock

- [ ] Add a short "Phase 1 scope" section to [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md): officers + research only, no behavior changes intended.
- [x] Define migration flag strategy:
  - Default **on** (spec + compiler for research attack-phase seats).
  - `KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE=1` (or `true`/`yes`) forces the legacy path.
  - `KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE=0` / `false` / `no` also disables the spec path.
- [ ] Add explicit non-goals for Phase 1 in [docs/ROADMAP.md](ROADMAP.md) (no engine timing changes, no new mechanics).

## Phase 1 - Canonical IR types and serde

### New files

- [ ] Create [src/data/combat_effect_spec.rs](../src/data/combat_effect_spec.rs)
  - `CombatEffectSpec`, `EffectSource`, `SourceRef`, `AbilityTriggerSpec`, `AbilityTargetSpec`
  - `AbilityModifierSpec`, `AbilityOperationSpec`
  - `ValueSpec`, `ChanceSpec`, `DurationSpec`
  - `AbilityConditionSpec` tree (`and`/`or`/`not` + primitives)
  - `StackingPolicySpec`
  - `confidence` and `category`
- [ ] Create [src/data/combat_effect_spec_validate.rs](../src/data/combat_effect_spec_validate.rs)
  - Structural validation (required fields, ranges)
  - Semantic validation (operation/modifier compatibility)
  - Clear diagnostics enum + display messages

### Existing files to update

- [ ] Export module from [src/data/mod.rs](../src/data/mod.rs)
- [ ] Add docs-level references in [docs/COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md)

### Acceptance criteria

- [ ] Round-trip serde tests for all enum variants
- [ ] Validation tests for invalid shapes (empty ids, invalid chance ranges, bad round bounds)

## Phase 2 - Compiler (spec -> engine runtime)

### New files

- [ ] Create [src/combat/effect_spec_compile.rs](../src/combat/effect_spec_compile.rs)
  - `compile_trigger(...) -> TimingWindow`
  - `compile_condition(...) -> AbilityCondition`
  - `compile_effect(...) -> AbilityEffect`
  - `compile_spec_to_seat(...) -> CrewSeatContext`

### Existing files to update

- [ ] Wire module export in [src/combat/mod.rs](../src/combat/mod.rs)
- [ ] Reuse condition helpers from [src/combat/condition.rs](../src/combat/condition.rs) where possible

### Acceptance criteria

- [ ] Unit tests for each mapping family:
  - trigger mappings
  - modifier/operation combinations
  - condition tree correctness (`and`/`or`/`not`)
- [ ] Compiler returns typed errors (not silent drops) for unsupported combinations

## Phase 3 - Research adapter (catalog -> spec)

### New files

- [ ] Create [src/data/research_effect_spec_adapter.rs](../src/data/research_effect_spec_adapter.rs)
  - Convert `ResearchRecord`/`ResearchBonusEntry` to `CombatEffectSpec`
  - Map `ResearchBonusConditionKey` to condition nodes
  - Preserve provenance (`rid`, optional `loca_id`/`buff_id` when available)

### Existing files to update

- [ ] Refactor [src/data/profile.rs](../src/data/profile.rs):
  - Replace direct special routing in `research_derived_attack_phase_seats` with adapter + compiler path when feature flag is enabled
- [ ] Keep existing path as fallback until parity is proven
- [ ] Update docs in [src/data/research.rs](../src/data/research.rs) comments to mention adapter path

### Tests

- [ ] Extend [tests/research_profile_merge_tests.rs](../tests/research_profile_merge_tests.rs):
  - conditional `weapon_damage`/burning seat parity
  - conditional `crit_*` parity
  - fallback behavior when flag off
- [ ] Add focused adapter unit tests in [src/data/research_effect_spec_adapter.rs](../src/data/research_effect_spec_adapter.rs)

### Acceptance criteria

- [ ] With flag on, research-derived seats exactly match current behavior for covered fixtures
- [ ] With flag off, no behavior change

## Phase 4 - LCARS adapter (LCARS -> spec)

### New files

- [ ] Create [src/lcars/effect_spec_adapter.rs](../src/lcars/effect_spec_adapter.rs)
  - Convert `LcarsEffect` and `LcarsCondition` to `CombatEffectSpec`
  - Preserve `officerId`, ability context, and LCARS provenance

### Existing files to update

- [ ] Update [src/lcars/resolver.rs](../src/lcars/resolver.rs):
  - Add path to resolve LCARS via spec compiler behind feature flag
  - Keep legacy resolver path for parity checks
- [ ] Update [src/lcars/parser.rs](../src/lcars/parser.rs) only if additional normalized fields are needed

### Tests

- [ ] Add adapter tests for representative LCARS primitives:
  - stat_modify, state effects, chance effects, decay/accumulate, condition trees
- [ ] Add parity tests: legacy resolver output == spec resolver output for canonical fixtures

### Acceptance criteria

- [ ] No regression in existing LCARS integration tests
- [ ] Unsupported LCARS tokens produce equivalent warnings/errors to current behavior

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
  - [x] Research: [tests/combat_effect_spec_research_parity_tests.rs](../tests/combat_effect_spec_research_parity_tests.rs) — legacy (`KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE=1`) vs `research_derived_attack_phase_seats_from_spec` (order-independent seat signatures)
  - [ ] LCARS fixtures (resolver vs spec compiler)
  - [ ] Mixed crew + research scenarios
- [ ] Add optional debug endpoint/flag in server layer (if useful) to dump compiled spec effects for investigation

### CI requirements

- [x] Add a CI job variant that runs tests with `KOBAYASHI_COMBAT_EFFECT_SPEC_DISABLE=1` (legacy path)
- [ ] Require parity suite green before default flip

### Default flip

- [ ] Enable spec path by default
- [ ] Keep legacy path for one release behind kill-switch
- [ ] Remove legacy path after stabilization window

## Deliverables by milestone

### Milestone A (research parity only)

- [ ] Phase 1 + 2 + 3 complete
- [ ] NS Burning Damage and existing conditional research behavior parity proven

### Milestone B (officer parity)

- [ ] Phase 4 complete
- [ ] LCARS fixture parity proven

### Milestone C (ecosystem hardening)

- [x] Phase 5 complete (stfc.cc adapter; bundled CSV 561/561)
- [ ] Phase 6 complete (golden parity harness, optional debug)
- [ ] Default flip completed and legacy retired

## Tracking notes template

Use this snippet in PRs/issues:

```md
## CombatEffectSpec checklist
- Phase: <1|2|3|4|5|6>
- Flag default: <on|off>
- Legacy fallback kept: <yes|no>
- Parity tests added:
  - [ ] Research
  - [ ] LCARS
  - [ ] Mixed scenarios
- Known gaps:
  - ...
```
