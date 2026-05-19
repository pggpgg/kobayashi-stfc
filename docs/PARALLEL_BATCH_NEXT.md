# Next parallel batch recommendation

**Date:** 2026-05-19  
**Completed:** Track D audit + Track D2 implementer (4 ship hull abilities).

## Recommended next batch: **Track A — Reload speed / LCARS**

**Why:** Active branch work (`tests/officer_reload_speed.rs`, Uhura/Chang/Kuron/Pon/Rom/Ortegas) already has engine + LCARS changes; per-officer subagents can add tests and fidelity notes without touching shared resolver core.

**Parallelism:** 6–7 agents (one officer each) + 1 integrator.

**Avoid:** Concurrent edits to `effect_spec_compile.rs` / `resolver.rs` — land shared IR first.

## Runner-up: **Track B — Officer fidelity**

Shard alphabetically through `officers.lcars.yaml`; expand `officer_modeling_fidelity.yaml` for the 48 `combat_tag_gaps` rows. Read-only classification agents scale to 8+ without engine risk.

## Defer

- **Track C** (drift fixtures) — high value but needs fight logs / tolerance judgment per fixture.
- **Track E** (buildings) — mostly non-combat `buff_*`; narrow to combat-relevant rows first.

See the parallel subagent playbook plan for wave structure and test policy.
