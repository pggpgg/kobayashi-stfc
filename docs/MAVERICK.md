# Maverick faction (Update 88) — Kobayashi tracking

This document tracks **what Maverick-related support means** for Kobayashi and what is **not** invented here without upstream stats.

## Game scope (high level)

- New faction path (Ops 55+, [Warp Dive Bar](https://startrekfleetcommand.com/news/update-88-first-look-the-maverick-faction/)).
- Additional research nodes (isolytic, apex-style effects, armada-oriented bonuses, etc.).
- New hostiles / armada targets (e.g. Conqueror Borg solo armadas) that the simulator may need as selectable scenarios.

## What Kobayashi already has that applies

- **Engine stats** such as `isolytic_damage`, `isolytic_defense`, and related ship-ability hooks already exist; Maverick content should map into those mechanisms once data is present.
- **Research sync** (`type: "research"`) persists `rid` / `level` to `research.imported.json`. When `data/research_catalog.json` contains matching `rid` entries with combat bonuses, the optimizer merges them (see [ROADMAP.md](ROADMAP.md) § Research).
- **Forbidden / chaos tech sync** from stfc-mod uses JSON `type: "tech"` with `fid` / tier / level / shard_count; Kobayashi persists that to `forbidden_tech.imported.json` (same file as `type: "ft"`). Catalog: `data/forbidden_chaos_tech.json`.

## Data work checklist (when upstream IDs are available)

1. **Research** — Add or import Maverick tree nodes into `data/research_catalog.json` via `scripts/import_stfcspace_research.mjs` once buff id → engine stat mappings exist for those nodes ([ROADMAP.md](ROADMAP.md) § Research).
   - **Update (upstream drop):** `summary-research.json` + `translations-research.json` now carry isolytic project text and ids. The Maverick **tree** in summary is `research_tree.id` **2259437121** (`loca_id` 72001); it currently has a single node (`rid` **3875042924**) whose buff is numeric “1” (unlock-style), not a combat multiplier—left unmapped.
   - **Imported as global bonuses:** Starships tree nodes with unconditional wording—**Isolytic Defense / Damage for all ships** (`loca_id` **55313** / **55314**, rids **3322225012** / **3146647716**) and **base Isolytic Damage / Defense for all ships** (**57002** / **57003**, rids **995454242** / **1852208276**)—are in `data/research/loca_id_to_stat.json` and the checked-in catalog (regenerate with the rid list in [data/README.md](../data/README.md) § Research). PvP-only, class-conditional, armada-only, and similar lines remain **unmapped** on purpose (see README).
2. **Hostiles** — Add normalized hostile JSON under `data/hostiles/` and index entries **only** from verified ship stats (e.g. data.stfc.space or recorded fights). Do not ship guessed hull/shield/damage.
3. **Forbidden tech** — If Maverick introduces new `fid` values, extend `data/import/forbidden_chaos_tech.csv` and re-run `cargo run --bin import_forbidden_chaos` so sync merge can match by `fid`.
4. **Calibration** — Add or extend fixtures under `tests/fixtures/recorded_fights/` for Maverick-relevant fights when logs are available.

### Faction / station copy

- **`translations-factions.json`** — Maverick UI strings (e.g. task keys, Warp Dive Bar) and placeholder `id` **88001**; Kobayashi does not yet consume this file for combat—useful for tooling and future faction-aware UX.
- **`translations-starbase_modules.json`** — Warp Dive Bar and other module names for building sync / resolver; already part of the building bid → id pipeline ([data/README.md](../data/README.md) § Buildings).

## Uncertainty

Exact numeric values for new ships, armadas, and research nodes are **not** assumed in this repo until they come from a validated upstream source or in-game capture. The roadmap item remains **open** until those assets land in `data/` with provenance.
