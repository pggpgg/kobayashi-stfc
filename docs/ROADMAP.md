# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

_Last updated 2026-06-20. The June 2026 audit backlog (engine decomposition, assurance gates, upstream drift automation, June patch content, import faction resolution, and related prep) is **shipped**. Durable design detail lives in linked docs, not here._

## Planned

- **Component upgrades** — Model per-component tier upgrades (Impulse, Shield, Warp, weapon turrets, etc.) separately from ship tier/level. Today combat stats come from `data/ships_extended` tier/level rows only; synced `profiles/*/ships.imported.json` component ids are not resolved into stat deltas. Needed for accurate optimize/sim when players run upgraded components above hull tier (e.g. T11 phasers on a T10 hull). Likely path: map component ids → upstream component curves, merge into attacker/defender stats at scenario build; profile sync already carries component lists.

## Optional follow-ups (low priority)

- **Hostile names in data files** — the hostile picker already shows proper names (`/api/hostiles` resolves `display_name` from upstream translations; [HostilePicker](../frontend/src/components/HostilePicker.tsx) labels rows accordingly). On-disk `hostile_name` in `data/hostiles/*.json` still uses `Hostile {id}` placeholders from the normalizer — cosmetic data debt, not a UI gap. Optional: bake resolved names into `normalize_hostiles_stfc_space` and refresh stale notes in [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md).

## Assessed, no action planned

From the 2026-06-09 audit, these came back clean or justified — don't re-litigate without new evidence:

- `scenario.rs` (~3,800 lines) is large but well-factored internally; no split needed.
- The 39 `clippy::too_many_arguments` allows are justified by the registry/DTO architecture.
- Error handling: panics are confined to defensive asserts, malformed API input 400s cleanly, job registries are bounded with prune-on-insert.
- Dependencies: minimal, stable, no git deps, no duplicates.
- Station-defense conditions remain a non-goal — see [NOT_ROADMAP.md](NOT_ROADMAP.md).
