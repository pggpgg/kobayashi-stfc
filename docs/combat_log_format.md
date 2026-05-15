# Combat Log Format (raw / ingested)

This document describes the format used for raw STFC combat logs that can be ingested and compared to simulator output.

For how to read mitigation and pierce fields in a trace, see [COMBAT_TRACE.md](COMBAT_TRACE.md).

## Purpose

- **Replay/parity**: Compare simulator trace and `SimulationResult` to real or exported combat.
- **Regression**: Add fixture logs and tests that assert parsed outcomes and event counts.

## Supported formats

### JSON export (ingested)

A single JSON object with:


| Field                       | Type              | Description                                     |
| --------------------------- | ----------------- | ----------------------------------------------- |
| `schema_version`            | number (optional) | Ingest format revision; omit or `1` for legacy logs. `2` enables strict canonical timeline validation ([`validate_canonical_timeline`](../src/combat/log_validate.rs)). `3` adds strict structured **state snapshot** pairing for **simulator-style** logs. `4` adds **client/toolbox** provenance rules ([`stats_snapshot` conventions](#stats_snapshot-provenance-client-vs-simulator)) and optional [`client_kind` registry](client_combat_log_mapping.md) checks; round tails follow the client profile (no mandatory closing `state_snapshot` unless you emit simulator snapshots). See [client_combat_log_mapping.md](client_combat_log_mapping.md). |
| `rounds_simulated`          | number            | Number of rounds completed.                     |
| `total_damage`              | number            | Total damage dealt to defender (hull + shield). |
| `attacker_won`              | boolean           | True if attacker won.                           |
| `defender_hull_remaining`   | number            | Defender hull HP at end.                        |
| `defender_shield_remaining` | number (optional) | Defender shield HP at end (0 if depleted).      |
| `events`                    | array             | Ordered list of events (see below).             |


Each event in `events`:


| Field             | Type              | Description                                                                                               |
| ----------------- | ----------------- | --------------------------------------------------------------------------------------------------------- |
| `event_type`      | string            | e.g. `round_start`, `damage_application`, `mitigation_calc`.                                              |
| `round_index`     | number            | 1-based round.                                                                                            |
| `phase`           | string            | e.g. `round`, `attack`, `damage`, `proc`, `defense`, `counter`, `end`.                                   |
| `values`          | object (optional) | Key-value pairs (e.g. `final_damage`, `running_total`, `shield_damage`, `hull_damage`).                   |
| `weapon_index`    | number (optional) | Sub-round (weapon) index when the simulator uses multi-weapon resolution; omitted for round-level events. |
| `sequence`        | number (optional) | Strictly increasing timeline index within the log; when present on any event, timeline validation runs (warnings only when `schema_version` is 1). |
| `client_kind`     | string (optional) | Opaque upstream/toolbox label for correlation only — **not** trusted as equivalent to Kobayashi `phase`. |
| `client_payload`  | any JSON (optional) | Raw snippet from upstream capture for debugging / future mapping.                                       |
| `stats_snapshot`  | object (optional) | Flat map of observable stats at this step for reverse-engineering (keys are conventional; document what you emit). |
| `state_snapshot`  | object (optional) | Typed combat state row ([`CombatStateSnapshot`](../src/combat/snapshot.rs)) for schema_version ≥ 3; Kobayashi trace rows use `event_type` `state_snapshot` with the same JSON under `values.snapshot`. |


Event types aligned with simulator trace for parity:

- `round_start` — start of round
- `attack_roll`, `pierce_calc`, `crit_resolution`, `proc_triggers`, `stack_resolution` — outbound weapon pipeline when captured from Kobayashi trace or enriched imports
- `damage_application` — damage applied this step (may include `shield_damage`, `hull_damage`, `running_hull_damage`, `defender_shield_remaining`)
- `mitigation_calc` — mitigation used (outbound phase `defense`, counter-fire phase `counter` when emitted)
- `end_of_round_effects` — bonus/burning
- `state_snapshot` — **Simulator-only** enriched row when `SimulationConfig.emit_state_snapshots` is true with `TraceMode::Events`. Carries a structured [`CombatStateSnapshot`](../src/combat/snapshot.rs) at canonical anchors (`after_round_start`, `before_outbound_shot`, `after_outbound_damage`, `after_subround`, `end_of_round_post_effects`). All fields are **simulator-sourced** unless you label external enrichment.

**CLI validation:** `kobayashi validate-log <path.json>` parses JSON, hydrates `values.snapshot` into `state_snapshot` when needed, and runs [`validate_canonical_timeline`](../src/combat/log_validate.rs) (strict errors when `schema_version` ≥ 2; **3** = simulator snapshot tail + pairing when using `state_snapshot` rows; **4** = client provenance / registered `client_kind` expectations).

### Source fidelity matrix (TSV vs ingested JSON vs simulator trace)

| Concern | Game TSV export ([`export_csv`](../src/combat/export_csv.rs)) | Ingested JSON (`IngestedCombatLog`) | Simulator `TraceMode::Events` |
| ------- | ------------------------------------------------------------- | ----------------------------------- | ------------------------------ |
| Fight summary (outcome, end hull/shield) | **Observable** (summary rows) | **Observable** (top-level numeric fields) | **Observable** (`SimulationResult` totals) |
| Fleet stats → `Combatant` | **Observable** (aggregated columns); some inferred defaults (e.g. shield mitigation 0.8) | N/A (inputs come from elsewhere) | From scenario/`Combatant` you pass in |
| Per-round event list | **Partial** — coarse `Type` column, damage columns | **Configurable** — any `event_type` / `weapon_index` you encode | **High** — `mitigation_calc`, `stack_resolution`, etc. |
| Sub-round (`weapon_index`) | **Optional** — column may be absent in vanilla export | **Optional** on each `IngestedEvent` | **Yes** when multi-weapon |
| Intermediate stat stacks / mitigation breakdown | **Unavailable** in TSV | **Optional** — `stats_snapshot`, `state_snapshot`, `values` | **Yes** — trace `values` + optional `state_snapshot` emission |
| Canonical timeline ordering | **Not validated** (TSV is not `IngestedCombatLog`) | **Validated** (`validate_canonical_timeline`) | Matches engine order |

Use this table when choosing `schema_version` and when claiming parity: **never label simulator-only trace fields as client-observed** unless you captured them from the game or toolbox with known provenance.

### `stats_snapshot` provenance (client vs simulator)

For reverse-engineering, consumers must not mix **measured** and **derived** numbers.

- Prefer **key prefixes** on entries inside `stats_snapshot` (flat map of string → JSON value):
  - `observed.*` — read from client/toolbox/game export without formula applied in Kobayashi.
  - `inferred.*` — computed in your import pipeline (e.g. derived from other observed fields); document the formula in commit or doc.
  - `sim.*` — copied from Kobayashi simulator output (for hybrid logs only).
- Alternatively (or additionally), include a **`_provenance`** object on the same map:
  - `\"_provenance\": { \"source\": \"client\" }` — whole map is client-leaning.
  - `\"source\": \"sim\"` | `\"merged\"` for hybrid toolchains.

**schema_version 4:** when `stats_snapshot` is present, validation requires either `_provenance.source` **or** that every key is `_provenance`, `_repeat_meta`, or starts with `observed.`, `inferred.`, or `sim.` (underscore-prefixed internal keys reserved).

### Collapsed UI repeats (optional expansion)

When the game UI **collapses** multiple identical mechanical applications into one line but the capture still records **how many** applied, you may encode:

- `values.collapsed_repeat_count` — integer **≥ 2** meaning “this row stands for N applications”.

**Normalization (no new combat math):** call [`expand_collapsed_repeat_events`](../src/combat/log_import_normalize.rs) to replace one row with **N** copies, each with `values.application_index` (0-based) and `values.application_count`. If the source does not expose `N`, do **not** invent it — keep a single row and document lossy parity.

Optional companion keys for ambiguous UIs:

- `values.repeat_group_id` — stable string shared by expanded siblings for debugging.
- `values.collapsed_ambiguous: true` — **schema_version 4** validation may **warn** (future: optional strict forbid).

See fixtures `tests/fixtures/recorded_fights/collapsed_repeat_before.json` and `collapsed_repeat_expanded.json` (after normalization).

Mapping toolbox/client strings to `event_type` / `phase` / `client_kind`: [client_combat_log_mapping.md](client_combat_log_mapping.md).

### schema_version 3 (simulator-style state snapshots)

Use when logs include Kobayashi-style `state_snapshot` events for full timeline regression.

- Each `state_snapshot` row must include a parseable [`CombatStateSnapshot`](../src/combat/snapshot.rs) (via `state_snapshot` or `values.snapshot`).
- **Pairing (strict errors):** every outbound `damage_application` (`phase` `damage`) must be **immediately** followed by a `state_snapshot` whose `anchor` is `after_outbound_damage`. Every `end_of_round_effects` must be **immediately** followed by a `state_snapshot` whose `anchor` is `end_of_round_post_effects`.
- **Round tail:** for each round, the last two events in timeline order must be `end_of_round_effects` then that closing `state_snapshot`.

**schema_version 4 with snapshots:** if the log contains any `state_snapshot` rows, the same pairing and round-tail rules apply (hybrid client + simulator traces).

Fixture: `tests/fixtures/recorded_fights/schema_v3_minimal_snapshot_log.json`.

Enable emission from the simulator: `TraceMode::Events` and `emit_state_snapshots: true` on [`SimulationConfig`](../src/combat/types.rs).

## Round/sub-round ordering

The simulator implements canonical STFC order:

1. `START_ROUND` → `HULL_REPAIR_START` / `HULL_REPAIR_END`
2. Per sub-round (weapon index 0, 1, …): officer/ship abilities → forbidden/chaos tech → attacker weapon `i` → defender weapon `i`
3. `END_ROUND`: burning tick (1% of target max hull per round while burning active), cleanup, next round (max 100)

Trace events for attack/damage include optional `weapon_index` when multi-weapon resolution is used. The ingested format may include sub-round granularity for parity; per-round events remain sufficient for summary parity.

### Game CSV/TSV export

The game can export a fight log as a **tab-separated** file with several sections. Use `parse_fight_export()` in `src/combat/export_csv.rs` to parse it, then `export_to_combatants()` to build attacker/defender `Combatant`s for the simulator.

**Sections (in order):**

1. **Summary** — Header row starting with `Player Name`. Two data rows: player (attacker) and enemy (defender).
  - Key columns: `Outcome` (VICTORY/DEFEAT), `Ship Name`, `Officer One`, `Officer Two`, `Officer Three`, `Hull Health Remaining`, `Shield Health Remaining`.
  - Player row outcome = attacker_won; defender hull/shield remaining come from the enemy row. `Ship Name` is used to infer attacker ship type; officer columns are used to build crew (see below). `"--"` or empty cells are treated as absent.
2. **Rewards** — Optional; header `Reward Name`, then reward rows. Skipped for combat parity.
3. **Fleet stats** — Header row starting with `Fleet Type`. Two data rows: `Player Fleet 1` and `Enemy Fleet 1`.
  - Used to build `Combatant` stats. Column names (exact match):
  - **Attack / defense**: `Attack`, `Defense`, `Damage Per Round` → engine uses `Damage Per Round` as attacker `attack`.
  - **Piercing / accuracy**: `Armour Pierce`, `Shield Pierce`, `Accuracy` → `AttackerStats` for mitigation/pierce.
  - **Defense**: `Armour`, `Shield Deflection`, `Dodge` → `DefenderStats`.
  - **Health**: `Hull Health`, `Shield Health` → combatant hull/shield HP.
  - **Crit**: `Critical Chance`, `Critical Damage` → combatant `crit_chance`, `crit_multiplier`.
  - Shield mitigation is defaulted to 0.8 if not present.
4. **Events** — Header row starting with `Round`. One row per battle event (Attack, Shield Depleted, Combatant Destroyed, etc.).
  - Columns are looked up by **header name** (not by position), so the parser tolerates reordered or added columns. Used names: `Round`, `Type`, `Critical Hit?`, `Hull Damage`, `Shield Damage`, `Total Damage`.
  - Optional `**Weapon Index`** (unsigned integer): sub-round / weapon slot index for parity with JSON ingested logs and simulator traces. Standard game exports usually omit this column; `FightExportEvent.weapon_index` is then `None`. Extended or synthetic exports may include it — see `tests/fixtures/recorded_fights/fight_export_weapon_index.tsv` and `export_parse_tests` in `src/combat/export_csv.rs`.
  - Summary parity uses total damage from summary (initial HP − remaining).

**Mapping to engine:**

- Attacker = `Player Fleet 1`, defender = `Enemy Fleet 1` (player is attacker in the export).
- Defender mitigation and attacker pierce are computed with `mitigation()` and `pierce_damage_through_bonus()` from `DefenderStats` and `AttackerStats` derived from the fleet rows.
- Ship type for mitigation weights is inferred from names (e.g. `HOSTILE BATTLESHIP` → Battleship); default Battleship if unknown.

**Crew from export:** Use `export_to_crew(export)` or `export_to_combat_input(export)` to get a `CrewConfiguration` from the summary officer slots. Slot convention: **Officer One** = captain, **Officer Two** = first bridge slot, **Officer Three** = second bridge slot; below_decks = [] unless the format is extended. Officer names are matched to canonical officers (`data/officers/officers.canonical.json`); unknown or empty slots are skipped. Use the returned crew when calling `simulate_combat` so the simulator runs with the same crew as the recorded fight.

**Attacker ship type:** Inferred from the player’s **Ship Name** in the summary. Known name → type mappings: `REALTA` → Explorer; names containing `BATTLESHIP`, `EXPLORER`, `INTERCEPTOR`, `SURVEY`, or `ARMADA` use that class. Default is Battleship if unknown. Stored as `attacker_ship_type` on `FightExport` for consistency and future use (e.g. morale primary piercing by ship type).

**Sample:** `fight samples/realta vs takret militia 10.csv` at repo root. Calibration test: `fight_export_realta_vs_takret_militia_10_matches_simulation` in `tests/recorded_fight_calibration_tests.rs` (uses `export_to_combat_input` and passes the crew into `simulate_combat`).

## Fixtures

- `tests/fixtures/recorded_fights/*.json` — sample logs for parser and parity tests (including `sample_combat_log.json` and `multi_weapon_round_log.json` for multi–sub-round `weapon_index` in one round).
- `tests/fixtures/recorded_fights/rich_engine_aligned_log.json` — synthetic `schema_version` 2 excerpt aligned with Kobayashi trace ordering for subsequence parity tests.
- `tests/fixtures/recorded_fights/invalid_timeline_v2.json` — intentional timeline violation for validator tests.
- `tests/fixtures/recorded_fights/invalid_sequence_v2.json` — non-monotonic `sequence` under `schema_version` 2 for validator tests.
- `tests/fixtures/recorded_fights/schema_v4_client_minimal.json` — `schema_version` 4 client-profile log (no `state_snapshot` rows).
- `tests/fixtures/recorded_fights/collapsed_repeat_before.json` / `collapsed_repeat_expanded.json` — collapsed vs expanded repeat encoding ([`expand_collapsed_repeat_events`](../src/combat/log_import_normalize.rs)).
- `tests/fixtures/recorded_fights/fight_export_weapon_index.tsv` — minimal TSV with optional `Weapon Index` column (fight export parser).
- `fight samples/*.csv` — game CSV/TSV exports for calibration (e.g. Realta vs Takret Militia 10).

### Drift calibration fixtures (`drift_*.json`)

Synthetic scenarios (not raw combat logs) used for regression: each file describes attacker/defender stats, `simulation.rounds` / `seed`, and inclusive numeric **bands** for key `SimulationResult` fields. The library module `kobayashi::calibration` loads a fixture, runs `simulate_combat` with an empty crew, and builds a **drift report** (per-metric σ from band midpoint, in/out of band). Use `format_drift_summary` to print a multi-fixture table (including the largest |σ| metrics — what moved farthest from the reference center). Tests: `tests/calibration_drift_tests.rs`.