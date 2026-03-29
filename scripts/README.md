# Scripts

## Single data-refresh entrypoint (audit task 19)

From the **repo root**, run importers and normalizers in a **fixed order**:

```bash
# Default: CSV-based importers only (forbidden tech, syndicate if CSV present)
npm run data:refresh

# data.stfc.space pipeline (needs cache under data/upstream/data-stfc-space/ — see below)
npm run data:refresh -- --stfcspace

# STFCcommunity baseline: fetch zip + normalize_stfc_data
npm run data:refresh -- --stfccommunity

# Everything above (long-running; needs upstream data on disk)
npm run data:refresh -- --all
```

Implementation: `scripts/data-refresh.mjs`. **Canonical order** (do not reorder without updating the script and this doc):

| Phase | Steps |
|-------|--------|
| **Core** (default) | `cargo run --bin import_forbidden_chaos` → `cargo run --bin import_syndicate_reputation` if `data/import/syndicate_reputation.csv` exists |
| **STFCcommunity** (`--stfccommunity`) | `scripts/fetch_stfc_data.ps1` (PowerShell; use `pwsh` on Linux/macOS if installed) → `cargo run --bin normalize_stfc_data` |
| **data.stfc.space** (`--stfcspace`) | Requires `summary-ship.json` + `translations-ships.json` under `data/upstream/data-stfc-space/`. Then: `python scripts/build_ship_registry.py` → `cargo run --bin normalize_data_stfc_space` → `cargo run --bin normalize_hostiles_stfc_space` (if `hostiles/*.json` present) → `node scripts/build_hull_id_registry.mjs` → `node scripts/import_stfcspace_buildings.mjs` (`--from-upstream` if `summary-building.json` exists, else live fetch) → `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0` if `summary-research.json` + `research/*.json` exist |

Related docs: [data/README.md](../data/README.md), [docs/STFC_SPACE_DATA_STRATEGY.md](../docs/STFC_SPACE_DATA_STRATEGY.md), [data/import/README.md](../data/import/README.md).

---

## Post-sync verification

After pulling changes from another machine, run:

```bash
npm run verify
```

This runs `cargo test`, `cargo build --release`, `cargo clippy`, then `npm ci`, `npm run test`, and `npm run build` in `frontend/`. Mirrors CI. Requires `data/officers/officers.canonical.json` and (recommended) `data/ships_extended/`, `data/hostiles/` indices.

After refreshing game data locally, run **`npm run data:refresh`** (with flags as needed), then **`npm run verify`**.

---

## Data pipeline (STFCcommunity baseline) — manual detail

The `--stfccommunity` flag runs this sequence for you. Manual equivalent:

1. **Fetch upstream:** From repo root, run  
   `powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1`  
   Downloads STFCcommunity/data and extracts hostiles + ships to `data/upstream/stfccommunity-data/`.

2. **Normalize:**  
   `cargo run --bin normalize_stfc_data`  
   Reads upstream JSON, writes KOBAYASHI-format `data/hostiles/` (and optionally buildings/factions). Ship output was removed; use `data/ships_extended/` from `normalize_data_stfc_space` + `build_ship_registry.py` instead.

3. **Optional:** Set `STFC_DATA_VERSION` (e.g. a git commit) when running the normalizer to record the source.

Upstream is treated as a read-only baseline (repo is outdated ~3y). Newer entries can be added under `data/hostiles/` with the same schema. Ships use `data/ships_extended/` (see data/README.md).
</think><｜tool▁call▁begin｜>
Shell
