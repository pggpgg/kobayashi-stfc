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

**CI:** The integration test `tests/scenario_research_integration_tests.rs` requires a populated `data/research_catalog.json` when `CI=true` (GitHub Actions). If you work without that file, other tests still run; the scenario test skips unless you set `KOBAYASHI_REQUIRE_RESEARCH_CATALOG=1` to match CI.

Related docs: [data/README.md](../data/README.md), [docs/STFC_SPACE_DATA_STRATEGY.md](../docs/STFC_SPACE_DATA_STRATEGY.md), [data/import/README.md](../data/import/README.md).

---

## Post-sync verification

After pulling changes from another machine, run:

```bash
npm run verify
```

This mirrors [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) in order:

| Stage | Commands |
|-------|----------|
| **Rust** | `cargo fmt --all -- --check` → `cargo test` → `cargo build --release` → `cargo clippy --all-targets -- -D warnings` → `cargo audit` |
| **Frontend** | `npm ci` → `npm audit --audit-level=high` → `npm run lint` → `npm run typecheck` → `npm run test` → `npm run build` (all in `frontend/`) |
| **Python** | `python -m pip install -r tools/combat_engine/requirements-test.txt` → `python -m pytest tools/combat_engine/tests/ -v` |

**Prerequisites:** stable Rust with `rustfmt` and `clippy` (same as CI), [`cargo-audit`](https://github.com/rustsec/rustsec) on your PATH (`cargo install cargo-audit`), Node.js **20** (see CI), and Python **3.12** with `pip`. The script picks the first working interpreter among `python`, `python3`, and on Windows `py -3.12` / `py -3`, or use **`PYTHON`** to point at an executable (e.g. `PYTHON=C:\\Python312\\python.exe`). Test data: `data/officers/officers.canonical.json` and (recommended) `data/ships_extended/`, `data/hostiles/` indices.

If a tool is not installed locally, you can skip individual steps (not used in CI): `VERIFY_SKIP_CARGO_FMT=1`, `VERIFY_SKIP_CARGO_AUDIT=1`, or `VERIFY_SKIP_PYTHON=1`. If Python is missing entirely, the script exits with a short message instead of an obscure shell error.

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
