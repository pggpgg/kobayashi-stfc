# Client / toolbox → `IngestedCombatLog` mapping

This document is **sample-driven**: extend it when redacted captures show repeatable client strings or JSON shapes. Until then it defines **conventions** and a small **registry** of `client_kind` values used in tests and validation (`schema_version` ≥ 4).

Related: [`combat_log_format.md`](combat_log_format.md) (IR fields, fidelity matrix, collapsed UI), [`log_validate.rs`](../src/combat/log_validate.rs) (`validate_canonical_timeline`).

## Principles

1. **Unknown stays opaque** — If you cannot map a client label to a Kobayashi `event_type` / `phase` with confidence, store the raw line in `client_payload` and choose a conservative `event_type` (e.g. keep a generic `ability_tick` or `unknown`) **without** claiming engine phase equality. Do **not** force a mapping to `damage_application` / `mitigation_calc` unless semantics match.
2. **Discriminator vs phase** — `client_kind` is for correlation and registry checks only; `phase` must follow Kobayashi semantics (`round`, `attack`, `damage`, `defense`, `counter`, `proc`, `end`, …).
3. **Sub-round** — When the toolbox exposes a weapon or shot index, copy it to `weapon_index` (0-based). If absent, omit `weapon_index`; do not guess.
4. **Ordering** — Timeline must match [`combat_log_format.md`](combat_log_format.md) § Round/sub-round ordering once mapped. Use monotonic `sequence` when emitting strict `schema_version` ≥ 2 logs.
5. **Stats provenance** — Use `stats_snapshot` key prefixes (`observed.*`, `inferred.*`, `sim.*`) or `_provenance.source` as described in [`combat_log_format.md`](combat_log_format.md).

## Synthetic / fixture `client_kind` registry

These entries are validated when `schema_version` ≥ 4: mapped rows **must** match the listed `event_type` and `phase`. Add new rows only after maintainer review.

| `client_kind`                 | Expected `event_type`   | Expected `phase` | Notes                                      |
| ----------------------------- | ----------------------- | ------------------ | ------------------------------------------ |
| `fixture_kob_outbound_damage` | `damage_application`    | `damage`           | Test / documentation only; not a game ID.  |

**Custom upstream labels** — If your capture uses strings like `START_ROUND` or toolbox-specific enums, record them here when you have confirmed equivalents:

| Client / toolbox signal (placeholder) | Suggested `event_type` | Suggested `phase` | `weapon_index` | Notes |
| ------------------------------------- | ------------------------ | ----------------- | -------------- | ----- |
| *TBD*                                 | *TBD*                    | *TBD*             | *TBD*          | Replace with real samples when available. |

## Round / sub-round identifiers

| Concept (client) | IR field(s) | Validator notes (`schema_version` ≥ 2) |
| ---------------- | ----------- | ---------------------------------------- |
| Round number (1-based) | `round_index` | Must be consistent per round block. |
| Sub-round / weapon | `weapon_index` | Optional; aligns with simulator multi-weapon order. |
| Monotonic global order | `sequence` | If any event has `sequence`, all must; strictly increasing when strict. |

## `client_payload` shape

Use any JSON value the capture provides (object, string, array). Prefer **no PII**; redact player names or IDs in committed fixtures.

## Collapsed UI repeats

When the UI collapses N identical applications into one row but N is known, set `values.collapsed_repeat_count` and run [`expand_collapsed_repeat_events`](../src/combat/log_import_normalize.rs). If N is **unknown**, set `values.collapsed_ambiguous: true` (validator emits a **warning** under `schema_version` 4) and do not expand.

## Change log

- **Initial:** Registry stub + `fixture_kob_outbound_damage` for CI/tests; placeholder table for real client strings.
