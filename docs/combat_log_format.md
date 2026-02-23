# Combat Log Format (raw / ingested)

This document describes the format used for raw STFC combat logs that can be ingested and compared to simulator output.

## Purpose

- **Replay/parity**: Compare simulator trace and `SimulationResult` to real or exported combat.
- **Regression**: Add fixture logs and tests that assert parsed outcomes and event counts.

## Supported formats

### JSON export (ingested)

A single JSON object with:

| Field | Type | Description |
|-------|------|-------------|
| `rounds_simulated` | number | Number of rounds completed. |
| `total_damage` | number | Total damage dealt to defender (hull + shield). |
| `attacker_won` | boolean | True if attacker won. |
| `defender_hull_remaining` | number | Defender hull HP at end. |
| `defender_shield_remaining` | number (optional) | Defender shield HP at end (0 if depleted). |
| `events` | array | Ordered list of events (see below). |

Each event in `events`:

| Field | Type | Description |
|-------|------|-------------|
| `event_type` | string | e.g. `round_start`, `damage_application`, `mitigation_calc`. |
| `round_index` | number | 1-based round. |
| `phase` | string | e.g. `round`, `attack`, `damage`, `end`. |
| `values` | object (optional) | Key-value pairs (e.g. `final_damage`, `running_total`, `shield_damage`, `hull_damage`). |

Event types aligned with simulator trace for parity:

- `round_start` — start of round
- `damage_application` — damage applied this step (may include `shield_damage`, `hull_damage`, `running_hull_damage`, `defender_shield_remaining`)
- `mitigation_calc` — mitigation used
- `end_of_round_effects` — bonus/burning

## Round/sub-round ordering (reference)

Canonical order from STFC client (for future sub-round support):

1. `START_ROUND` → `HULL_REPAIR_START` / `HULL_REPAIR_END`
2. Per sub-round: officer/ship abilities → forbidden/chaos tech → attacks (weapon index)
3. `END_ROUND`: burning tick (1% initial hull), cleanup, next round (max 100)

The ingested format does not require sub-round granularity; per-round events are sufficient for summary parity.

### Game CSV/TSV export

The game can export a fight log as a **tab-separated** file with several sections. Use `parse_fight_export()` in `src/combat/export_csv.rs` to parse it, then `export_to_combatants()` to build attacker/defender `Combatant`s for the simulator.

**Sections (in order):**

1. **Summary** — Header row starting with `Player Name`. Two data rows: player (attacker) and enemy (defender).
   - Key columns: `Outcome` (VICTORY/DEFEAT), `Hull Health Remaining`, `Shield Health Remaining`.
   - Player row outcome = attacker_won; defender hull/shield remaining come from the enemy row.

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
   - Columns: `Round`, `Type`, `Critical Hit?`, `Hull Damage`, `Shield Damage`, `Total Damage`, …
   - Parsed for optional round-by-round comparison; summary parity uses total damage from summary (initial HP − remaining).

**Mapping to engine:**

- Attacker = `Player Fleet 1`, defender = `Enemy Fleet 1` (player is attacker in the export).
- Defender mitigation and attacker pierce are computed with `mitigation()` and `pierce_damage_through_bonus()` from `DefenderStats` and `AttackerStats` derived from the fleet rows.
- Ship type for mitigation weights is inferred from names (e.g. `HOSTILE BATTLESHIP` → Battleship); default Battleship if unknown.

**Sample:** `fight samples/realta vs takret militia 10.csv` at repo root. Calibration test: `fight_export_realta_vs_takret_militia_10_matches_simulation` in `tests/recorded_fight_calibration_tests.rs`.

## Fixtures

- `tests/fixtures/recorded_fights/*.json` — sample logs for parser and parity tests.
- `fight samples/*.csv` — game CSV/TSV exports for calibration (e.g. Realta vs Takret Militia 10).
