# CombatEffectSpec (Draft)

This document proposes a single canonical combat-effect schema for Kobayashi.

Goal: normalize **officers (LCARS)**, **research**, **ship abilities**, and optional **stfc.cc cheat-sheet rows** into one typed IR before compiling into engine runtime objects (`AbilityEffect`, `AbilityCondition`, `TimingWindow`).

## Why

- Today, officers use a rich LCARS model while research uses stat-bonus rows plus special Rust routing.
- A shared schema reduces drift and special-casing (for example, conditional `weapon_damage` research needing attack-phase seats).
- A single validator + compiler improves explainability and parity checks across data sources.

## Scope

`CombatEffectSpec` is a **canonical IR**, not a replacement for every source format:

- LCARS remains an authoring DSL for officers.
- `research_catalog.json` remains the normalized research catalog.
- stfc.cc-style columns remain an ingestion vocabulary.

All of these adapt into `CombatEffectSpec`.

### Phase 1 delivery scope (implemented)

The first shipped slice focused on **parity and migration safety**, not a full rewrite of combat:

- **In scope:** typed IR + serde + validation; compiler to existing engine types; research → spec → attack-phase seats (sole path in `research_derived_attack_phase_seats`); optional `KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE=0` for diagnostics; LCARS → spec adapter for tooling and parity; optional HTTP debug for LCARS rows (`GET /api/debug/combat-effect-spec/officers/:id` when `KOBAYASHI_COMBAT_EFFECT_SPEC_DEBUG=1`).
- **Out of scope for that slice:** replacing every LCARS code path in the live resolver in one step; changing engine timing windows or damage formulas for migration convenience (see [ROADMAP.md](ROADMAP.md) non-goals).

## Canonical schema

```ts
type CombatEffectSpec = {
  // Identity / provenance
  id: string;                    // stable effect id (source-scoped)
  source: EffectSource;          // where this came from
  sourceRef?: SourceRef;         // rid/fid/officer id/ability id/etc
  text?: EffectText;             // optional display name/description

  // Core semantics
  trigger: AbilityTrigger;       // when effect evaluates
  target: AbilityTarget;         // who receives the effect
  modifier: AbilityModifier;     // what is modified / state action
  operation: AbilityOperation;   // add/multiply/set/min/max/...

  // Magnitude / chance / scaling
  value?: ValueSpec;             // scalar or rank table
  chance?: ChanceSpec;           // scalar or rank table
  duration?: DurationSpec;       // rounds/stacks/permanent

  // Optional gates and attributes
  conditions?: AbilityConditionSpec[]; // AND semantics by default
  attributes?: Record<string, string | number | boolean>;

  // Composition / stack behavior (default engine behavior if omitted)
  stacking?: StackingPolicy;

  // Combat / non-combat tagging
  category?: "combat" | "non_combat";
  confidence?: "authoritative" | "inferred" | "heuristic";
};
```

### Source + provenance

```ts
type EffectSource =
  | "lcars_officer"
  | "research_catalog"
  | "ship_ability_catalog"
  | "hostile_ability_catalog"
  | "stfc_cc_cheat_sheet"
  | "manual";

type SourceRef = {
  officerId?: string;
  abilityId?: string;
  rid?: number;
  fid?: number;
  shipId?: string;
  hostileId?: string;
  buffId?: number;
  locaId?: number;
  rowId?: string;
};

type EffectText = {
  name?: string;
  description?: string;
};
```

### Trigger / target

```ts
type AbilityTrigger =
  | "combat_begin"
  | "round_start"
  | "attack_phase"
  | "defense_phase"
  | "receive_damage"
  | "shield_break"
  | "self_shield_break"
  | "hull_breach"
  | "kill"
  | "round_end"
  | "combat_end";

type AbilityTarget =
  | "attacker_self"
  | "defender_opponent"
  | "attacker_team"
  | "defender_team";
```

### Modifier / operation

`modifier` is a normalized mechanic token; `operation` describes the math/write behavior.

```ts
type AbilityModifier =
  // direct stat modifiers
  | "weapon_damage"
  | "hull_hp"
  | "shield_hp"
  | "crit_chance"
  | "crit_damage"
  | "pierce"
  | "shield_mitigation"
  | "armor" // Rust IR: `MitigationAdditive` (serde wire name)
  | "shield_deflection" // Rust IR: `ShieldDeflection`; same compile path as `armor`
  | "dodge"
  | "damage_reduction"
  | "accuracy"
  | "isolytic_damage"
  | "isolytic_defense"
  | "isolytic_cascade_damage"
  | "apex_shred"
  | "apex_barrier"
  // state mechanics
  | "state_morale"
  | "state_burning"
  | "state_hull_breach"
  | "state_assimilated"
  // special mechanics already in engine
  | "shots_bonus"
  | "proc_attack_multiplier"
  | "proc_pierce_bonus"
  | "hostile_crit_damage_reduction"
  | "cumulative_opponent_shield_mitigation_debuff"
  // non-combat passthrough
  | "tag_only";

type AbilityOperation =
  | "add"
  | "multiply"
  | "set"
  | "min"
  | "max"
  | "chance_apply"
  | "state_apply"
  | "state_extend";
```

### Value / chance / duration

```ts
type ValueSpec = {
  scalar?: number;       // fixed value
  by_rank?: number[];    // rank table (index 0 = rank 1)
  unit?: "fraction" | "flat" | "rounds" | "stacks";
};

type ChanceSpec = {
  scalar?: number;       // 0..1
  by_rank?: number[];    // 0..1 table
};

type DurationSpec =
  | { type: "permanent" }
  | { type: "rounds"; rounds: number }
  | { type: "stacks"; stacks: number };
```

### Conditions

```ts
type AbilityConditionSpec =
  | { type: "morale_active" }
  | { type: "defender_burning" }
  | { type: "defender_hull_breach" }
  | { type: "attacker_burning" }
  | { type: "attacker_hull_breach" }
  | { type: "defender_assimilated" }
  | { type: "defender_is_npc_hostile" }
  | { type: "defender_is_player_ship" }
  | { type: "defender_ship_type_is"; shipType: "battleship" | "explorer" | "interceptor" | "survey" | "armada" }
  | { type: "attacker_ship_type_is"; shipType: "battleship" | "explorer" | "interceptor" | "survey" | "armada" }
  | { type: "defender_faction_is"; faction: string }
  | { type: "defender_hull_faction_id_is"; factionId: number }
  | { type: "round_range"; min: number; max: number }
  | { type: "stat_below"; stat: string; thresholdPct: number }
  | { type: "stat_above"; stat: string; thresholdPct: number }
  | { type: "not"; inner: AbilityConditionSpec }
  | { type: "and"; all: AbilityConditionSpec[] }
  | { type: "or"; any: AbilityConditionSpec[] };
```

### Stacking policy

```ts
type StackingPolicy = {
  additiveGroup?: string;      // same group sums
  multiplicativeGroup?: string; // same group multiplies
  maxStacks?: number;
  exclusiveGroup?: string;     // highest priority wins
  priority?: number;           // higher wins inside exclusive group
};
```

## Mapping adapters

### LCARS -> CombatEffectSpec

Map directly from existing LCARS fields (`type`, `stat`, `target`, `operator`, `value`, `trigger`, `duration`, `scaling`, `condition`, `chance`, etc.) to canonical fields with minimal loss.

### Research -> CombatEffectSpec

For each `ResearchBonusEntry`:

- `trigger = attack_phase` for conditional attack-scoped stats (`weapon_damage`, `crit_*`) that currently emit derived seats.
- Otherwise `trigger = combat_begin` equivalent static bonus semantics.
- `conditions` from `ResearchBonusConditionKey` (same conjunctive order as [`ability_condition_from_research_bonus_key`](../src/combat/condition.rs); every set field **AND**s together):
  - `requires_morale` -> `morale_active`
  - `requires_defender_burning` -> `defender_burning`
  - `requires_defender_hull_breach` -> `defender_hull_breach`
  - `defender_ship_class` / `defender_faction` -> typed defender condition nodes
  - `attacker_faction` (single slug) / `attacker_factions` (OR of slugs when several majors apply) -> `attacker_owner_faction_is` node(s); together with defender gates this yields an overall **AND** (owner disjunction only inside `attacker_factions`)
- Morale-gated catalog `isolytic_damage` (`requires_morale: true`) uses `round_start` timing; `conditions` merge `morale_active` with other [`ResearchBonusConditionKey`](../src/data/research.rs) gates (AND).

### stfc.cc row -> CombatEffectSpec

Treat as ingestion aliases:

- `AbilityModifier` -> `modifier`
- `AbilityConditions` -> `conditions[]` (token map)
- `AbilityTrigger` -> `trigger`
- `AbilityTarget` -> `target`
- `AbilityOperation` -> `operation`
- `AbilityAttributes` -> `attributes`
- `AbilityChances` -> `chance.by_rank` or `chance.scalar`
- `AbilityValues` -> `value.by_rank` or `value.scalar`

Important: stfc.cc naming is input vocabulary, not runtime contract.

**Implementation:** `[src/data/stfc_cc_effect_spec_adapter.rs](../src/data/stfc_cc_effect_spec_adapter.rs)` maps cheat-sheet columns to [`CombatEffectSpec`] where tokens are known; unknown modifiers/triggers/targets/operations/conditions emit stable `unmapped_*:` diagnostics. CLI: `cargo run --bin stfc_cc_cheat_sheet_report` (optional report on `data/upstream/cheat-sheet/raw-officers-m88-17rc.csv`).

## Compile target

Compiler emits existing engine structures:

- `AbilityEffect`
- `AbilityCondition`
- `TimingWindow`

No immediate engine rewrite is required.

## Worked examples

### A) NS Burning Damage research (rid 365419690)

```json
{
  "id": "research:365419690:l1",
  "source": "research_catalog",
  "sourceRef": { "rid": 365419690, "buffId": 1898558353, "locaId": 70106 },
  "trigger": "attack_phase",
  "target": "attacker_self",
  "modifier": "weapon_damage",
  "operation": "add",
  "value": { "scalar": 0.01, "unit": "fraction" },
  "conditions": [{ "type": "defender_burning" }],
  "category": "combat",
  "confidence": "authoritative"
}
```

### B) Officer crit chance first 2 rounds

```json
{
  "id": "officer:gorkon:cm",
  "source": "lcars_officer",
  "sourceRef": { "officerId": "gorkon", "abilityId": "cm" },
  "trigger": "combat_begin",
  "target": "attacker_self",
  "modifier": "crit_chance",
  "operation": "add",
  "value": { "by_rank": [0.1, 0.15, 0.2, 0.25, 0.3], "unit": "fraction" },
  "conditions": [{ "type": "round_range", "min": 1, "max": 2 }],
  "category": "combat"
}
```

## Migration plan (high level)

1. Introduce `CombatEffectSpec` structs + serde schema in `src/data/combat_effect_spec.rs`.
2. Add adapters:
  - LCARS -> spec
  - research row -> spec
  - optional stfc.cc CSV -> spec
3. Add a compiler: spec -> current engine structs.
4. Move existing `research_derived_attack_phase_seats` logic onto spec compiler output.
5. Add parity tests:
  - current behavior vs compiled behavior for officers and research fixtures.
6. Keep old paths temporarily behind a feature flag; remove after parity confidence.

## Phase 1 scope (implementation)

Phase 1 delivers the **canonical IR + serde**, a **compiler to existing engine structs**, and the **research-derived attack-phase seat** path (always via the spec adapter + compiler in-tree). **No intended combat timing or formula changes** for migration: golden tests lock adapter output against the public API.

### Task 9 — LCARS officers (shipped)

Dynamic officer LCARS effects (`stat_modify`, `morale`, `burning`, `hull_breach`, `assimilated`, and the other `resolve_effect` modes) are authored in YAML as before, adapted by [`src/lcars/effect_spec_adapter.rs`](../src/lcars/effect_spec_adapter.rs), and compiled by [`compile_officer_combat_spec`](../src/combat/effect_spec_compile.rs) in [`src/lcars/resolver.rs`](../src/lcars/resolver.rs) (`resolve_effect`). **Runtime `Ability.condition` on those effects** is the AND-combined compile of `CombatEffectSpec.conditions` (same IR as YAML `condition` via `lcars_condition_to_spec`); if YAML has a `condition` block the adapter cannot encode, `lcars_effect_to_combat_effect_spec` returns no row (no ungated effect). Canonical condition tokens are mapped by [`map_canonical_condition_token`](../src/lcars/canonical_conditions.rs) for validation and reporting. Static passive-permanent `stat_modify` rows and `extra_attack` proc aggregation stay in `resolve_crew_to_buff_set` unchanged.

- **Parity:** [`tests/lcars_captain_spec_parity_tests.rs`](../tests/lcars_captain_spec_parity_tests.rs) compares the spec pipeline to `resolve_officer_ability` for captain maneuvers (tiers 1–5) and samples bridge / below-decks tiers; [`tests/lcars_combat_effect_spec_parity_tests.rs`](../tests/lcars_combat_effect_spec_parity_tests.rs) covers synthetic LCARS rows.
- **Next (optional follow-up):** extend IR/compiler for any new LCARS `effect_type` values before they appear in `captain_ability` (the parity test allow-list will fail CI if an unlisted type ships).

## Non-goals

- Replacing LCARS YAML as officer authoring input.
- Committing to raw stfc.cc field names as the internal runtime API.
- Expanding unsupported mechanics without engine evidence/tests.

