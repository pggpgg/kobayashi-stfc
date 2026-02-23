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

## Fixtures

- `tests/fixtures/recorded_fights/*.json` — sample logs for parser and parity tests.
