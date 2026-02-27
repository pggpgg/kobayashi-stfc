### Building combat bonuses – import pipeline

This directory holds **normalized building combat bonuses** for the simulator.
The target schema is `BuildingRecord` / `BuildingLevel` / `BonusEntry`
(`src/data/building.rs`) with:

- `stat`: engine/LCARS stat key (e.g. `weapon_damage`, `hull_hp`,
  `shield_hp`, `defense_platform_damage`, `crit_damage`,
  `isolytic_damage`, `isolytic_defense`).
- `value`: numeric bonus, **fractional when percentage-based**
  (e.g. `0.35` = `+35%`).
- `operator`: `"add"` (additive) or `"multiply"` (stacked as
  \((1 + current) * (1 + value) - 1\)).
- Optional `conditions`: tags scoping when the bonus applies
  (e.g. `["defense_platform_only"]`, `["vs_hostiles"]`).
- Optional `notes`: free-form comments for edge cases or non-combat effects.

The **index** file is `data/buildings/index.json` (`BuildingIndex`), which lists
all buildings plus optional provenance:

- `data_version`: short identifier for the upstream snapshot
  (e.g. `"stfccommunity-main"`, `"stfcspace-2026-02-26"`).
- `source_note`: human-readable description of the source and validation.

Each building has its own file `data/buildings/<id>.json` with a single
`BuildingRecord`.

---

### 1. STFCcommunity → normalized buildings

The primary pipeline for buildings mirrors ships/hostiles:

- Upstream JSON from STFCcommunity lives under the configured
  `UPSTREAM_BUILDINGS_SUFFIX` directory.
- `src/bin/normalize_stfc_data.rs` reads those files and writes normalized
  output under `data/buildings/`:
  - Per-building files `<id>.json` matching `BuildingRecord`.
  - `index.json` (`BuildingIndex`) with `data_version` and `source_note`.
- `RawBuildingBonusMeta.percentage` is used to convert percentage-style values
  into fractional bonuses; see `raw_to_building_record`:
  - If `percentage == true`, raw values are divided by `100.0`.
  - Otherwise values are copied as-is.
- Human-readable upstream bonus labels are mapped to engine stat keys by
  `bonus_name_to_stat` in `normalize_stfc_data.rs`. This mapping must stay
  consistent with `src/data/syndicate_combat.rs` and the ship/hostile schemas.

Running the normalizer:

- Fetch upstream data (`scripts/fetch_stfc_data.ps1`).
- Run the normalizer binary (see project README for exact command).
- Inspect `data/buildings/index.json` and a few sample `<id>.json` files.

---

### 2. Spreadsheets → intermediate raw → normalized buildings

For spreadsheet-based sources (personal sheets, community workbooks):

1. **Export to CSV/TSV** per logical dataset:
   - Recommended layout: one row per `(building_id, level)` with columns for
     the various bonuses.
   - Store raw exports under `data/raw/buildings/spreadsheets/`.

2. **Column/label mapping**:
   - Maintain a small mapping table (in code or config) from column labels to
     engine stat keys and operators, similar to:
     - `"Defense Platform Damage"` → `stat = "defense_platform_damage"`, `operator = "add"`.
     - `"Station Hull Health"` → `stat = "hull_hp"`, `operator = "add"`.
   - Reuse the same stat key names as:
     - `src/data/ship.rs` (`ShipRecord`)
     - `src/data/hostile.rs` (`HostileRecord`)
     - `src/data/syndicate_combat.rs` (syndicate stat mapping)

3. **Import script (future work)**:
   - A small import tool (Rust binary or script) should:
     - Read CSV rows from `data/raw/buildings/spreadsheets/*.csv`.
     - Normalize building identifiers (`building_id`) and `level`.
     - Apply the mapping table to produce `BonusEntry` values
       (`stat`, `value`, `operator`).
     - Aggregate rows by `(building_id, level)` and write/update
       `data/buildings/<id>.json`.
     - Emit an import report (JSON) under `data/import_logs/` with counts and
       any unmapped/ambiguous columns.

4. **Conflict resolution**:
   - When spreadsheet values differ from STFCcommunity or other sources,
     record the precedence rule in `source_note` and/or a short note in
     `data/buildings/_known_issues.md`.

---

### 3. stfc.space → API → normalized buildings

For stfc.space, building data is imported via the public JSON API:

1. **API fetch**:
   - `scripts/import_stfcspace_buildings.mjs` fetches
     `https://data.stfc.space/building/summary.json` directly (no web scraping).
   - The script maps building ids and buff ids to engine stat keys via
     `BUILDING_META` and `BUFF_MAPPING` in the script.
   - Produces or updates `data/buildings/<id>.json` and `data/buildings/index.json`.

2. **Running the importer**:
   - From repo root: `npm run import:buildings:stfcspace`
   - Extend `BUILDING_META` and `BUFF_MAPPING` in the script as you add more
     buildings or buff mappings.

3. **Provenance and comparison**:
   - Use distinct `data_version` values for different sources
     (e.g. `"stfcspace-2026-02-26"` vs `"stfccommunity-main"`).
   - When both STFCcommunity and stfc.space provide the same field, either:
     - Prefer one source globally and document that in `source_note`, or
     - Prefer per-building/per-field overrides and record exceptions in
       `data/buildings/_known_issues.md`.

---

### 4. Logging and issue tracking

Import tools (normalizer, spreadsheet importer, stfc.space importer) should:

- Write structured logs under `data/import_logs/`, e.g.:
  - `data/import_logs/buildings-stfccommunity-2026-02-26.json`
  - `data/import_logs/buildings-stfcspace-2026-02-26.json`
- Include in each log:
  - Source descriptor (e.g. `"stfccommunity"`, `"stfc.space"`, `"spreadsheet:<file>"`).
  - `data_version` used.
  - Counts of buildings and `(building_id, level)` rows processed.
  - Number of unmapped labels and a sample of them.

Known discrepancies or TODOs for manual follow-up can be tracked in
`data/buildings/_known_issues.md`.

