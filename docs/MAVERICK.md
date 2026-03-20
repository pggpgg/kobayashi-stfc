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
2. **Hostiles** — Add normalized hostile JSON under `data/hostiles/` and index entries **only** from verified ship stats (e.g. data.stfc.space or recorded fights). Do not ship guessed hull/shield/damage.
3. **Forbidden tech** — If Maverick introduces new `fid` values, extend `data/import/forbidden_chaos_tech.csv` and re-run `cargo run --bin import_forbidden_chaos` so sync merge can match by `fid`.
4. **Calibration** — Add or extend fixtures under `tests/fixtures/recorded_fights/` for Maverick-relevant fights when logs are available.

## Uncertainty

Exact numeric values for new ships, armadas, and research nodes are **not** assumed in this repo until they come from a validated upstream source or in-game capture. The roadmap item remains **open** until those assets land in `data/` with provenance.
