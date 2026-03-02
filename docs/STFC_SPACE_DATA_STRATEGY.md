# data.stfc.space Import Strategy for KOBAYASHI

## Overview

This document outlines a strategy to import ship and hostile data from **data.stfc.space** (the stfc.space backend API) into KOBAYASHI, replacing the outdated STFCcommunity baseline (3+ years old). We will build a **phased approach** that maintains backwards compatibility while progressively integrating fresher data.

---

## Part 1: KOBAYASHI Data Requirements

### Hostile Data Model (from `src/data/hostile.rs`)

**Hostile Record** — Full stats for a single hostile (used in combat resolution):
- `id`: Unique identifier (e.g., `actian_apex_33_interceptor`)
- `hostile_name`: Display name (e.g., "Actian Apex")
- `level`: Level/tier (u32)
- `ship_class`: One of `battleship`, `explorer`, `interceptor`, `survey`, `armada`
- `armor`: Defense stat (f64)
- `shield_deflection`: Defense stat (f64)
- `dodge`: Defense stat (f64)
- `hull_health`: Total hull HP (f64)
- `shield_health`: Total shield HP (f64)
- `shield_mitigation`: Optional, fraction of damage sent to shields vs hull (default 0.8); Some hostiles (e.g., Sarcophagus) use 0.2
- `apex_barrier`: Optional, true damage mitigation (flat reduction after other mitigations)
- `isolytic_defense`: Optional, flat reduction to isolytic damage taken

**Hostile Index** — Lookup catalog in `data/hostiles/index.json`:
- `data_version`: Semver or date string (e.g., `"stfcspace-2026-03-01"`)
- `source_note`: Attribution (e.g., `"stfc.space API (data.stfc.space/hostile/summary.json)"`)
- `hostiles[]`: Array of `HostileIndexEntry` (id, hostile_name, level, ship_class)

**Example hostile record** (`data/hostiles/actian_apex_33_interceptor.json`):
```json
{
  "id": "actian_apex_33_interceptor",
  "hostile_name": "Actian Apex",
  "level": 33,
  "ship_class": "interceptor",
  "armor": 10137.0,
  "shield_deflection": 170250.0,
  "dodge": 10137.0,
  "hull_health": 44300066.0,
  "shield_health": 56381929.0
}
```

---

### Ship Data Model (from `src/data/ship.rs`)

**Ship Record** — Full stats for a player ship at a chosen tier:
- `id`: Unique identifier (e.g., `amalgam`)
- `ship_name`: Display name (e.g., `"AMALGAM"`)
- `ship_class`: One of `battleship`, `explorer`, `interceptor`, `survey`, `armada`
- `armor_piercing`: Weapon stat aggregated from components (f64)
- `shield_piercing`: Weapon stat aggregated from components (f64)
- `accuracy`: Weapon stat aggregated from components (f64)
- `attack`: Representative damage/attack value per round (f64)
- `crit_chance`: Critical strike probability (f64, 0.0–1.0)
- `crit_damage`: Critical damage multiplier (f64, e.g., 1.5 = 150%)
- `hull_health`: Total hull HP (f64)
- `shield_health`: Total shield HP (f64)
- `shield_mitigation`: Optional, fraction of damage sent to shields vs hull (default 0.8)
- `apex_shred`: Optional, reduces defender's effective Apex Barrier (decimal, e.g., 0.1 = 10%)
- `isolytic_damage`: Optional, bonus damage vs isolytic defense (decimal, e.g., 0.15 = 15%)
- `weapons`: Optional, per-weapon attack values for sub-round resolution (Vec<WeaponRecord>)

**Ship Index** — Lookup catalog in `data/ships/index.json`:
- `data_version`: Semver or date string
- `source_note`: Attribution
- `ships[]`: Array of `ShipIndexEntry` (id, ship_name, ship_class)

**Example ship record** (`data/ships/amalgam.json`):
```json
{
  "id": "amalgam",
  "ship_name": "AMALGAM",
  "ship_class": "survey",
  "armor_piercing": 477.0,
  "shield_piercing": 477.0,
  "accuracy": 477.0,
  "attack": 1814.5,
  "crit_chance": 0.1,
  "crit_damage": 1.5,
  "hull_health": 19200.0,
  "shield_health": 9600.0
}
```

---

## Part 2: Known data.stfc.space Endpoints

Based on the existing `scripts/import_stfcspace_buildings.mjs`, we know:

| Endpoint | Format | Status |
|----------|--------|--------|
| `https://data.stfc.space/building/summary.json` | Array of building metadata + per-level buffs | ✅ Confirmed |
| `https://data.stfc.space/hostile/summary.json` | Array of 4,838 hostile entries (id, level, ship_type, hull_type, faction, systems, resources) | ✅ Confirmed |
| `https://data.stfc.space/hostile/{id}.json` | Single hostile with full components + stats + abilities | ✅ Confirmed (e.g., `/hostile/2918121098.json`) |
| `https://data.stfc.space/translations/en/materials.json` | 8,063 localization strings (materials/items, NOT hostile names) | ✅ Confirmed — does NOT contain hostile names |
| `https://data.stfc.space/ship/summary.json` | (Inferred) Array of ship stats | ❓ To discover |
| `https://data.stfc.space/officer/summary.json` | (Inferred) Officer metadata | ❓ To discover |
| `https://data.stfc.space/translations/en/hostiles.json` | (Inferred) Hostile name translations keyed by loca_id | ❓ To discover |

### Coverage Assessment

**stfc.space hostile count: 4,838** vs **KOBAYASHI current: 2,241** — stfc.space has **2.16× more hostiles**, covering levels 1–81.

**Key gaps that stfc.space fills:**
- High-level hostiles (levels 60–81) that KOBAYASHI may be missing
- Rare/epic/legendary hostiles (rarity 2/3/4 = 471 entries)
- Hostile special abilities (percentage buffs with proc chances)
- Hostile offensive stats (accuracy, piercing, damage, crit) — useful for future defender-side simulation

**Key gap in stfc.space:**
- **Hostile names**: The `loca_id` field requires a translation lookup. `materials.json` maps to material names, NOT hostile names. We need to discover the correct translation endpoint (likely `translations/en/hostiles.json` or `translations/en/ships.json` or a unified `translations/en/loca.json`).

---

## Part 3: Endpoint Discovery Strategy

### Phase 1a: Manual URL Inventory

To confirm available endpoints without code changes, try these in sequence:

1. **Hostile endpoint candidates:**
   - `https://data.stfc.space/hostile/summary.json` → Array of hostiles with stats
   - `https://data.stfc.space/hostiles.json` → Alternative naming
   - `https://data.stfc.space/hostiles/summary.json` → Plural variant

2. **Ship endpoint candidates:**
   - `https://data.stfc.space/ship/summary.json` → Array of ships with stats
   - `https://data.stfc.space/ships.json` → Alternative naming
   - `https://data.stfc.space/ships/summary.json` → Plural variant

3. **Inspect an individual entry from summary:**
   - `https://data.stfc.space/hostile/{id}.json` → Detailed hostile stats
   - `https://data.stfc.space/ship/{id}.json` → Detailed ship stats

Once the summary endpoints are confirmed, **log the structure** (especially field names, presence of level info, and stat aggregation) in `data/import_logs/stfcspace-endpoint-discovery.json`.

---

## Part 4: Data Mapping Strategy

### Hostile Mapping (stfc.space → KOBAYASHI)

The stfc.space hostile detail JSON has a flat `stats` object and a `components` array. The `stats` object provides pre-aggregated values that map directly to KOBAYASHI fields.

**Summary endpoint fields:** `id`, `faction`, `level`, `ship_type`, `is_scout`, `is_outpost`, `loca_id`, `hull_type`, `rarity`, `count`, `strength`, `systems`, `warp`, `resources`

**Detail endpoint fields:** All of the above plus `components[]`, `ability[]`, `stats`

| stfc.space Field | KOBAYASHI Field | Notes |
|------------------|-----------------|-------|
| `id` (numeric) | `id` | Numeric in stfc.space; KOBAYASHI uses string IDs like `explorer_30`. Need name+level+class composite key |
| `loca_id` → translation lookup | `hostile_name` | **Name resolution needed**: `loca_id` maps to a translation file. `materials.json` does NOT have hostile names — need to discover `translations/en/hostiles.json` or similar |
| `level` | `level` | Direct mapping |
| `hull_type` | `ship_class` | **Mapping: 0→battleship, 1→survey, 2→interceptor, 3→explorer, 5→survey** |
| `stats.armor` | `armor` | Direct — e.g., `119094112350` for a level 81 |
| `stats.absorption` | `shield_deflection` | stfc.space calls it "absorption"; KOBAYASHI calls it "shield_deflection" |
| `stats.dodge` | `dodge` | Direct — e.g., `17864116852` |
| `stats.hull_hp` | `hull_health` | Direct — e.g., `2223800222236970` |
| `stats.shield_hp` | `shield_health` | Direct — e.g., `108857353536076` |
| Shield component `mitigation` | `shield_mitigation` | From `components[].data.mitigation` where `tag == "Shield"`. Example: `0.8` |
| (not present) | `apex_barrier` | Default `0.0` — not found in stfc.space data |
| (not present) | `isolytic_defense` | Default `0.0` — not found in stfc.space data |

**Additional fields available from stfc.space (not currently used by KOBAYASHI):**
- `stats.accuracy`, `stats.armor_piercing`, `stats.shield_piercing` — hostile offensive stats
- `stats.critical_chance`, `stats.critical_damage` — hostile crit stats
- `stats.attack`, `stats.dpr`, `stats.strength`, `stats.health`, `stats.defense` — aggregate stats
- `components[].data` for weapons: `shots`, `warm_up`, `cool_down`, `accuracy`, `penetration`, `modulation`, `minimum_damage`, `maximum_damage`, `crit_chance`, `crit_modifier`, `weapon_type`
- `ability[]` — hostile special abilities with percentage values and chances
- `ship_type` — hostile category (swarm=5, explorer=2, interceptor=7, etc.) distinct from `hull_type`
- `faction.id` — faction affiliation (Klingon, Romulan, etc.)
- `rarity` — 1=common, 2/3/4=rare/epic/legendary
- `systems[]` — system IDs where the hostile spawns

**Hull type → ship_class mapping** (confirmed from 4,838 entries):
```
hull_type 0 → battleship (1,414 hostiles)
hull_type 1 → survey (354 hostiles)
hull_type 2 → interceptor (1,337 hostiles)
hull_type 3 → explorer (1,211 hostiles)
hull_type 5 → survey variant (522 hostiles)
```

**Ship type distribution** (hostile category, NOT ship class):
```
ship_type  5 → 1,171 entries (levels 1-81)   — likely "Swarm" / generic
ship_type  7 → 1,153 entries (levels 5-81)   — likely faction hostiles
ship_type  2 → 1,025 entries (levels 8-81)   — likely Explorer-type encounters
ship_type  1 →   522 entries (levels 21-80)  — likely Survey encounters
ship_type  8 →   434 entries (levels 20-55)  — likely Borg or special
ship_type 10 →   152 entries (levels 25-80)
ship_type  4 →   112 entries (levels 20-51)
ship_type 11 →   111 entries (levels 20-51)
ship_type 12 →    84 entries (levels 35-43)
```

### Ship Mapping (stfc.space → KOBAYASHI)

Assuming stfc.space ship summary returns an array of ships with per-tier stats:

| stfc.space Field | KOBAYASHI Field | Notes |
|------------------|-----------------|-------|
| `id` | `id` | Use as-is for persistent identity |
| `name` | `ship_name` | Human-readable name |
| `ship_class` or `shipClass` | `ship_class` | Normalize to lowercase |
| `tiers[0].components.*.weapons.armor_pierce` | `armor_piercing` | Aggregate (sum or mean) across weapon components in tier 1 |
| `tiers[0].components.*.weapons.shield_pierce` | `shield_piercing` | Aggregate (sum or mean) across weapon components in tier 1 |
| `tiers[0].components.*.weapons.accuracy` | `accuracy` | Aggregate across weapon components |
| `tiers[0].components.*.weapons.damage` or `.max_damage` | `attack` | Aggregate or mean; representative DPR |
| `tiers[0].components.*.weapons.crit_chance` | `crit_chance` | Highest or mean from weapon components |
| `tiers[0].components.*.weapons.crit_damage` | `crit_damage` | Mean from weapon components (default 1.5) |
| `tiers[0].components.*.health` or `hull_health` | `hull_health` | From hull component or aggregated |
| `tiers[0].components.*.shield.health` or `shield_health` | `shield_health` | From shield component |
| (missing) | `shield_mitigation` | Default to `null`; override if field found |
| (missing) | `apex_shred` | Default to `0.0`; populate if field exists |
| (missing) | `isolytic_damage` | Default to `0.0`; populate if field exists |
| `tiers[0].components[].weapons` | `weapons` | Optional per-weapon array for sub-round resolution |

**Key decision:** For ships with multiple tiers, we will **default to Tier 1** (the starter tier) to provide a baseline. Later phases can support tier selection via CLI flags or API parameters.

---

## Part 5: Freshness Tracking & Versioning

### Data Version Format

Use ISO-8601 date format: `stfcspace-YYYY-MM-DD` (e.g., `stfcspace-2026-03-01`)

Record in:
- `data/hostiles/index.json` → `data_version` field
- `data/ships/index.json` → `data_version` field

### Timestamp Metadata

Store in each index file:
- `source_note`: Attribution string (e.g., `"stfc.space API (data.stfc.space/hostile/summary.json + per-hostile detail)"`)
- `last_imported_at`: ISO-8601 timestamp of import (optional, added at runtime)
- `import_url`: Full URL(s) used to fetch data (for debugging)

### Freshness Check Logic

When running import:
1. Fetch upstream summary endpoint
2. Compare each entry's `id` + metadata hash against existing `data/hostiles/index.json`
3. If identical, skip detailed fetch (cache hit)
4. If new or changed, fetch detailed record and update
5. Log differences in `data/import_logs/stfcspace-import-YYYY-MM-DD.json`

---

## Part 6: Proposed CLI Command

### New Subcommand: `fetch-data`

```bash
# Fetch all hostile data from stfc.space
cargo run --release -- fetch-data --hostile

# Fetch all ship data from stfc.space
cargo run --release -- fetch-data --ship

# Fetch both
cargo run --release -- fetch-data --all

# Fetch with version override
STFCSPACE_DATA_VERSION="stfcspace-2026-03-01" cargo run --release -- fetch-data --all

# Validate and log mismatches
cargo run --release -- fetch-data --hostile --validate --log-unmapped
```

### Implementation Location

- **Main logic:** `src/bin/import_stfcspace_data.rs` (new binary, paralleling `import_stfcspace_buildings.mjs`)
- **Core library:** `src/data/stfcspace_importer.rs` (handles endpoint discovery, mapping, freshness checks)
- **CLI dispatch:** Update `src/cli.rs` to handle `fetch-data` verb
- **HTTP client:** Use `reqwest` or similar (check existing dependencies)

### Error Handling & Logging

- If summary endpoint unavailable: warn and skip, preserve existing data
- If field mapping fails: log unmapped fields to `data/import_logs/` for later refinement
- If HTTP rate limit hit: exponential backoff (configurable via env var)
- Always write import log with:
  - Timestamp
  - Endpoint URLs
  - Count of fetched/updated records
  - List of unmapped field ids (for future mapping expansion)
  - Any validation errors

---

## Part 7: Implementation Phases

### Phase 1: Endpoint Discovery (PARTIALLY COMPLETE)

**Status:** Hostile endpoints confirmed (summary + detail). Translation endpoint for hostile names still needed. Ship endpoint not yet confirmed.

**Remaining tasks:**
1. Discover hostile name translation endpoint (try `translations/en/hostiles.json`, `translations/en/loca.json`, `translations/en/ships.json`)
2. Confirm ship endpoint (`ship/summary.json`)
3. Map `ship_type` values to hostile categories (swarm, borg, eclipse, etc.)
4. Map `faction.id` values to faction names

**Previous discoveries (from manual testing):**

**Tasks completed:**
1. ✅ Manually tested hostile/summary endpoint → confirmed 4,838 hostile entries
2. ✅ Downloaded sample hostile JSON via individual endpoints → confirmed detail structure
3. ✅ Documented field names, nesting depth, data types
4. ✅ Analyzed `stats` object and `components` array structure
5. ✅ Built hull_type → ship_class mapping (5 types, 4,838 entries categorized)

**Deliverable:**
- ✅ Confirmed endpoint URLs for hostile summary and detail
- ✅ Sample JSON responses analyzed
- ✅ Field mapping table finalized for hostiles (Part 4)
- ✅ Coverage assessment added (stfc.space: 4,838 vs KOBAYASHI: 2,241)

**Outstanding:**
- Ship endpoint confirmation
- Translation endpoint discovery for hostile names
- Faction ID mapping
- Ship type category mapping

---

### Phase 2: Hostile Import (Estimated 4–6 hours)

**Goal:** Ingest hostile data from stfc.space; validate against combat engine expectations.

**Tasks:**
1. Create `src/bin/import_stfcspace_data.rs` with:
   - `fetch_hostile_summary()` function
   - `fetch_hostile_detail(id)` function
   - Field mapping logic (hostile_name, level, ship_class, stats)
   - Index generation and update
2. Implement freshness checking:
   - Load existing `data/hostiles/index.json`
   - Skip unchanged entries
   - Fetch and overwrite changed entries
3. Write import log with:
   - Count of fetched/new/unchanged hostiles
   - Any unmapped fields or validation errors
4. Test:
   - Run on small subset (e.g., 5 hostiles) to validate mapping
   - Run full import
   - Verify `data/hostiles/index.json` is valid and loadable
   - Run `cargo test` to ensure no regression

**Deliverable:**
- `src/bin/import_stfcspace_data.rs` with hostile import capability
- Updated `data/hostiles/` directory with stfc.space data (or merge commit if successful)
- Import log in `data/import_logs/`

**Risk factors:**
- Field names may not match expectations → requires mapping adjustment
- Numeric values may be scaled differently (e.g., 0–100 vs 0.0–1.0) → scale conversion needed
- Some hostiles may have missing stats → skip with warning, log

---

### Phase 3: Ship Import (Estimated 4–6 hours)

**Goal:** Ingest ship data from stfc.space; validate against combat engine expectations.

**Tasks:**
1. Extend `src/bin/import_stfcspace_data.rs` with:
   - `fetch_ship_summary()` function
   - `fetch_ship_detail(id)` function
   - Per-tier aggregation logic (default to tier 1, but allow CLI override)
   - Field mapping logic (ship_name, ship_class, weapon/shield/armor stats)
2. Implement component aggregation:
   - Sum or mean armor_pierce, shield_pierce, accuracy across weapon components
   - Extract hull_health and shield_health from appropriate component
3. Implement per-weapon stats (if available):
   - Store `weapons[]` array in ShipRecord for sub-round resolution
4. Test:
   - Run on small subset (5 ships) to validate mapping
   - Run full import
   - Verify `data/ships/index.json` is valid
   - Run combat sims to ensure stats produce reasonable outcomes

**Deliverable:**
- Extended `src/bin/import_stfcspace_data.rs` with ship import
- Updated `data/ships/` directory with stfc.space data
- Import log in `data/import_logs/`

**Risk factors:**
- Ship tier structure may differ from expected (components, aggregation)
- Weapon damage may be a range (min/max) → need to choose representative value
- Some ships may have no tier 1 data → default to tier 0 or skip

---

### Phase 4: Automated Freshness Checks (Estimated 2–3 hours)

**Goal:** Enable CI/scheduled runs to detect stale data.

**Tasks:**
1. Add `--check-freshness` flag to `fetch-data` command:
   - Fetches summary endpoints only (no detail fetches)
   - Compares data_version timestamps
   - Reports "up-to-date" or "stale (X days old)"
2. Add GitHub Actions workflow `.github/workflows/check-data-freshness.yml`:
   - Runs weekly on Monday 0800 UTC
   - Calls `cargo run --release -- fetch-data --all --check-freshness`
   - Opens PR with updated index files if stale
3. Document in `docs/STFC_SPACE_DATA_STRATEGY.md` under "Operations"

**Deliverable:**
- `--check-freshness` logic in importer binary
- GitHub Actions workflow
- Documentation update

**Risk factors:**
- API rate limiting may prevent frequent checks
- PR auto-creation requires careful handling to avoid spam

---

## Part 8: Field Mapping Gaps & Refinement

### Expected Mapping Challenges

1. **Ship weapon stats:** stfc.space may return per-weapon damage, while KOBAYASHI expects aggregated attack + per-weapon array
   - **Solution:** Extract mean damage as `attack`, store individual weapon stats in `weapons[]`

2. **Critical hit stats:** stfc.space may not have per-ship crit_chance/crit_damage
   - **Solution:** Use hardcoded defaults from existing data, allow manual override in LCARS

3. **Apex Barrier / Isolytic Defense:** stfc.space may not expose these special stats
   - **Solution:** Default to `0.0`, manually edit per-hostile JSON as new content is discovered

4. **Ship class normalization:** stfc.space uses different casing/format
   - **Solution:** Implement canonical lowercase mapping: `Battleship` → `battleship`

5. **Level resolution:** Some entries may span level 1–60, others 1–70
   - **Solution:** Import all levels, but default simulator to use a single level (e.g., max level)

### Future Mapping Expansion

Store a **mapping registry** in `data/stfcspace_mappings.json`:
```json
{
  "hostile_fields": {
    "id": "id",
    "name": "hostile_name",
    "level": "level",
    "shipClass": "ship_class",
    "stats.armor": "armor",
    "stats.shield_deflection": "shield_deflection",
    "stats.dodge": "dodge",
    "stats.hull": "hull_health",
    "stats.shield": "shield_health",
    "_notes": "Add unmapped fields here as they're discovered"
  }
}
```

When a field is encountered that's not in the registry:
1. Log to unmapped_fields in import log
2. Operator reviews log, updates mapping registry
3. Re-run import with updated mapping

---

## Part 9: Risk Factors & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| **API endpoint not found** | Medium | Blocker (Phase 1) | Test URLs early; check community Discord for endpoint docs |
| **Field names differ widely** | Medium | 4–6h delay (Phase 2) | Build flexible mapping registry; log unmapped fields |
| **Numeric values scaled unexpectedly** | Medium | Incorrect sims | Add scale conversion step; validate against known boss stats |
| **Rate limiting or downtime** | Low | Delay | Implement exponential backoff; cache responses locally |
| **Data quality issues** (missing stats, nulls) | Medium | Validation failures | Graceful skipping with detailed logging; human review before merge |
| **Breaking changes in API** | Low–Medium | Re-work mapping | Document API contract in import logs; version data by date |
| **Merge conflicts with manual edits** | Low | Rebase friction | Keep manual edits separate from auto-imported data; use `source_note` to distinguish |

---

## Part 10: Operations & Maintenance

### Running the Import

```bash
# Fetch and import all data
cargo build --release
./target/release/kobayashi fetch-data --all

# Review changes
git diff data/hostiles/index.json data/ships/index.json

# Commit with data version
git add data/hostiles/ data/ships/ data/import_logs/
git commit -m "Import stfc.space data: stfcspace-2026-03-01"
```

### Validating After Import

```bash
# Ensure indices load without errors
cargo test load_indices

# Run combat sims to spot-check stat reasonableness
cargo run --release -- simulate 1000 42 --ship amalgam --hostile actian_apex_40_interceptor
```

### Monitoring & Alerting

- **Weekly freshness check** (Phase 4): GitHub Actions logs → PR comment if data is >30 days old
- **Import logs** in `data/import_logs/`: Review for unmapped fields, errors, count changes
- **Combat sanity tests**: Run pre-commit hook to validate sim outcomes against known benchmarks

### Deprecating Old Data

When stfc.space data becomes the canonical source:
1. Move `data/upstream/stfccommunity-data/` to archive
2. Update `source_note` in index files to reflect new source
3. Keep commit history for auditing

---

## Part 11: Integration with Existing Pipelines

### Current Data Flow

```
scripts/fetch_stfc_data.ps1
  → data/upstream/stfccommunity-data/ (hostiles/*.json, ships/*.json)
    → cargo run --bin normalize_stfc_data
      → data/hostiles/index.json + data/hostiles/{id}.json
      → data/ships/index.json + data/ships/{id}.json
```

### New Data Flow (Proposed)

```
cargo run --release -- fetch-data --all
  → Fetch stfc.space/hostile/summary.json + per-hostile details
  → Fetch stfc.space/ship/summary.json + per-ship details
    → Map fields to KOBAYASHI schema
    → Write data/hostiles/index.json + data/hostiles/{id}.json
    → Write data/ships/index.json + data/ships/{id}.json
    → Log results to data/import_logs/stfcspace-import-YYYY-MM-DD.json
```

**Backwards compatibility:**
- If `fetch-data` fails, existing data in `data/hostiles/` and `data/ships/` is untouched
- Index file `source_note` clearly indicates source (stfc.space vs STFCcommunity)
- New import log format helps distinguish old vs new data

---

## Part 12: Success Criteria

✅ **Phase 1 Complete:**
- Confirmed endpoint URLs (or alternative endpoints discovered)
- Sample JSON responses logged in `data/import_logs/endpoint-discovery.json`
- Field mapping table finalized

✅ **Phase 2 Complete:**
- `src/bin/import_stfcspace_data.rs` binary builds and runs without errors
- 100+ hostiles imported with correct field mapping
- `data/hostiles/index.json` loads in simulator without errors
- Import log shows <5% unmapped fields

✅ **Phase 3 Complete:**
- 50+ ships imported with correct field mapping
- `data/ships/index.json` loads in simulator without errors
- Combat sims run against known hostiles produce "reasonable" hull remaining (within ±10% of historical baseline)

✅ **Phase 4 Complete:**
- GitHub Actions workflow runs weekly
- Manual freshness check via CLI completes in <30 seconds
- Import log clearly indicates freshness status

---

## Appendix: Example Import Log

File: `data/import_logs/stfcspace-import-2026-03-01.json`

```json
{
  "timestamp": "2026-03-01T14:23:45Z",
  "source": "stfc.space",
  "data_version": "stfcspace-2026-03-01",
  "endpoints_used": [
    "https://data.stfc.space/hostile/summary.json",
    "https://data.stfc.space/hostile/{id}.json (120 requests)"
  ],
  "hostiles": {
    "total_fetched": 120,
    "new": 5,
    "updated": 115,
    "unchanged": 0,
    "failed": 0
  },
  "ships": {
    "total_fetched": 65,
    "new": 3,
    "updated": 62,
    "unchanged": 0,
    "failed": 0
  },
  "unmapped_fields": {
    "hostile": ["stun_immunity", "debuff_resistance"],
    "ship": ["crew_speed", "command_points"]
  },
  "validation_errors": [
    "Hostile 'klingon_interceptor_5' missing shield_deflection stat; using 0.0",
    "Ship 'ushaan' has negative crit_chance; clamped to 0.0"
  ],
  "notes": "First import from stfc.space; recommend manual review of unmapped fields"
}
```

---

## Appendix: Existing Building Import as Reference

The `scripts/import_stfcspace_buildings.mjs` demonstrates the pattern we'll follow:

1. **Simple metadata table** (BUILDING_META) maps API ids to canonical ids
2. **Buff mapping registry** (BUFF_MAPPING) maps buff ids to stat keys
3. **Fallback behavior:** Unmapped buffs are auto-created with `buff_{id}` keys
4. **Logging:** Comprehensive import log tracks unmapped entries for future expansion
5. **Graceful degradation:** Missing data is skipped, not errored

We'll replicate this pattern for hostiles and ships.

---

## Summary

This strategy enables KOBAYASHI to consume fresher data from stfc.space while maintaining backwards compatibility with the existing STFCcommunity baseline. By breaking the effort into four phases, we can incrementally validate each step and course-correct early if the API structure differs from expectations.

**Next step:** Phase 1 endpoint discovery. Confirm the exact URLs and response formats before committing to implementation.
