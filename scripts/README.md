# Scripts

Prefer **`cargo xtask --help`** from the repo root for a single discoverable entry point; each subcommand runs the same steps documented below. Under the hood this crate invokes the `node` / `python` / `cargo` commands in this file.

## Automated refresh (CI)

A **weekly** GitHub Action ([`.github/workflows/data-refresh.yml`](../.github/workflows/data-refresh.yml)) runs the same high-level sequence as a local refresh: snapshot summaries → catalog fetch → ship/hostile/research detail fetches (scheduled: ships `--full`, hostiles/research missing-only) → `npm run data:refresh -- --stfcspace` → `cargo test` + `validate_data`, and opens a PR when files change (body includes summary drift report).

**CI drift gate:** every CI run includes job `upstream_drift` ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)), which fails when live data.stfc.space ship/hostile/research summaries diverge from committed caches. Remediation: merge the weekly bot PR or refresh locally.

**Local check:**

```bash
cargo xtask check-upstream-drift
# or: node scripts/check_stfcspace_summary_drift.mjs --check
```

**Manual run:** Actions → **Data refresh (stfc.space)** → *Run workflow*. Inputs:

| Input | Effect |
|-------|--------|
| `full_fetch` | Pass `--full` to all three `fetch_stfcspace_*.mjs` scripts (long; re-downloads all cached ids). |
| `dry_run` | Run fetch + normalize + tests but **skip** opening a PR. |

Scheduled runs use ships `--full` automatically; hostiles/research stay missing-only unless **full_fetch** is enabled on manual dispatch.

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

## data.stfc.space: catalog vs per-id detail

**Catalog (summaries + translations)** — refresh when you want the latest index and strings (run often):

```bash
python3 scripts/fetch_stfcspace_page_upstream.py
# or: node scripts/fetch_stfcspace_page_upstream.mjs
```

**Per-id JSON** — large; default is **missing-only** (skip if `data/upstream/data-stfc-space/.../{id}.json` exists). Use **`--full`** (or **`--force`**) to overwrite all cached ids from the summary.

| Script | Cache directory | Normalizer / next step |
|--------|-----------------|-------------------------|
| `node scripts/fetch_stfcspace_ships.mjs` | `ships/` | `python3 scripts/build_ship_registry.py` → `cargo run --bin normalize_data_stfc_space` |
| `node scripts/fetch_stfcspace_hostiles.mjs` | `hostiles/` | `cargo run --bin normalize_hostiles_stfc_space` |
| `node scripts/fetch_stfcspace_officers.mjs` | `officers/` | After refresh: `python3 scripts/normalize_officer_id_strings.py` syncs decimal ids + **`below_decks` slots** from these files into `officers.canonical.json`, then `cargo run --bin generate_lcars` rebuilds `officers.lcars.yaml` |
| `node scripts/fetch_stfcspace_research.mjs` | `research/` (tracked) | `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0` |
| `node scripts/fetch_stfcspace_forbidden_tech.mjs` | `forbidden_tech/` | Manual / CSV workflows (see `data/README.md` § Forbidden tech) |

**Unknown mappings report:** `cargo run --bin report_unknown_mappings` lists canonical `conditions` tokens that do not map to LCARS yet, hostile `upstream_ship_type` values from `data/hostiles/index.json`, research mapping gaps (unmapped buff ids + suspect global scopes via `scripts/research_mapping_gaps.mjs`), building opaque `buff_*` actionable gaps (vs `data/buildings/opaque_buff_allowlist.json`), and forbidden-tech bonus routing gaps. See [docs/CANONICAL_CONDITIONS.md](../docs/CANONICAL_CONDITIONS.md) § Regenerate unknown-mappings report, [docs/building_gaps.md](../docs/building_gaps.md), [docs/research_unmapped_triage.md](../docs/research_unmapped_triage.md), and [data/README.md](../data/README.md) § Research).

**Building opaque buff gaps:** `node scripts/seed_building_opaque_allowlist.mjs` (`--write` to merge economy/meta entries; `--check` for CI after import). `cargo run --bin report_building_mapping_gaps` (actionable-only Markdown; also `docs/building_gaps.md`). Allowlist: `data/buildings/opaque_buff_allowlist.json`. Baseline: `data/buildings/mapping_gaps_baseline.json`. Regression: `cargo test --test building_opaque_buff_baseline`. Strict validate: `KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS=1` or `cargo run --bin validate_data -- --strict`.

**Research mapping gaps:** `node scripts/research_mapping_gaps.mjs` (or `--json` for CI/validate). Baseline for regression checks: `data/research/mapping_gaps_baseline.json`. Strict validate: `KOBAYASHI_REQUIRE_RESEARCH_MAPS=1` or `cargo run --bin validate_data -- --strict`.

**Owner-faction triage (human review):** `npm run triage:research:faction` (or `node scripts/triage_research_owner_faction.mjs`) joins `research/*.json`, `translations-research.json` (`research_project_name` / `research_project_description`), and `data/buildings/buff_id_to_semantics.json`, then buckets lines into likely player-hull vs vs-opponent wording. Use `--json` for machine-readable output; `--skip-economy` drops obvious component/repair/tritanium lines so combat candidates surface faster.

**Bulk faction gates on mappings** (experimental; heuristic — review diffs): `npm run gen:research:faction-patch` (same as `node scripts/gen_research_faction_buff_patch.mjs`) merges `attacker_faction` / `defender_faction` into `data/research/buff_id_to_stat.json` for lines emitted by triage (`--skip-economy`). Stats are resolved with the same resolver as `import_stfcspace_research.mjs`. Use `--dry-run` to inspect the patch JSON; `--force-all` regenerates mappings that already declare `attacker_faction`. After substantive mapping edits run `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0` so `research_catalog.json` picks them up.

**Unmapped buff triage:** pipe `--dump-unmapped` JSON to `node scripts/triage_research_unmapped.mjs` (see [docs/research_unmapped_triage.md](../docs/research_unmapped_triage.md)).

**Hull/shield unmapped triage:** same pipe to `node scripts/triage_research_hull_shield_unmapped.mjs`. **Building buff patch:** `node scripts/gen_research_hull_shield_building_buff_patch.mjs` copies shared `hull_hp` / `shield_hp` buff ids from `data/buildings/buff_id_to_stat.json` into `data/research/buff_id_to_stat.json` when they appear in research upstream.

**Orchestrator** (same flags; **`--entities` required** — comma-separated subset):

```bash
npm run fetch:stfcspace:details -- --entities ships,hostiles --limit 50
npm run fetch:stfcspace:details -- --entities ships,hostiles,officers,research,forbidden_tech --full
```

Shared helpers: [scripts/lib/stfcspace_detail_fetch.mjs](lib/stfcspace_detail_fetch.mjs).

### Ability catalogs (ships + hostiles)

| Script | Output | Audit doc |
|--------|--------|-----------|
| `python3 scripts/generate_full_ship_ability_catalog.py` | `data/upstream/data-stfc-space/ship_ability_catalog.json` | [docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](../docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) |
| `python3 scripts/generate_full_hostile_ability_catalog.py` | `data/upstream/data-stfc-space/hostile_ability_catalog.json` | [docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md](../docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md) |

Both merge optional `*_catalog_overrides.json` after heuristics. Run after refreshing upstream `ships/` or `hostiles/` detail JSON.

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
