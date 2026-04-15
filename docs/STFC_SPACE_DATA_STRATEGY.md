# data.stfc.space Import Strategy for KOBAYASHI

## Overview

This document outlines a strategy to import ship and hostile data from **data.stfc.space** (the stfc.space backend API) into KOBAYASHI, replacing the outdated STFCcommunity baseline (3+ years old). We will build a **phased approach** that maintains backwards compatibility while progressively integrating fresher data.

---

## Part 1: KOBAYASHI Data Requirements

### Hostile Data Model (from `src/data/hostile.rs`)

**Hostile Record** (`HostileRecord`) — Normalized stats for one hostile. Combat uses **defender** fields when the player attacks the hostile (`to_defender_stats()`), and **attacker** fields when the hostile deals damage to the player (e.g. counter-fire: `to_attacker_stats()` plus per-weapon rows from `components`).

**Identity and hull class**

- `id`: Unique string id (stfc.space pipeline: numeric string from upstream, e.g. `"111884576"`)
- `hostile_name`: Display name (may be a placeholder until `loca_id` is translated)
- `level`: Level/tier (u32)
- `ship_class`: One of `battleship`, `explorer`, `interceptor`, `survey`, `armada` (derived from upstream `hull_type`)

**Defender stats (incoming player damage)**

- `armor`, `shield_deflection`, `dodge`: Mitigation inputs (f64)
- `hull_health`, `shield_health`: HP pools (f64)
- `shield_mitigation`: `Option<f64>` — fraction of damage to shields vs hull when shields are up (often `0.8`; some hostiles use `0.2`)
- `apex_barrier`: Flat true-damage mitigation after other steps (f64; default `0.0` if absent in older JSON)
- `isolytic_defense`: Defender stat for isolytic mitigation (f64; default `0.0`; combat uses isolytic taken ∝ `1 / (1 + isolytic_defense)`)
- `mitigation_floor`, `mitigation_ceiling`, `mystery_mitigation_factor`: Optional tuning for rare mitigation formulas (`Option`)

**Attacker / offensive stats (hostile → player)**

- `accuracy`, `armor_piercing`, `shield_piercing`: Aggregate weapon stats (f64); same role as on player ships for pierce-through / hit-vs-dodge style resolution when this hostile is the attacker
- `crit_chance`, `crit_damage`: Hostile crit parameters (f64)

**Upstream aggregates and metadata (stfc.space; defaulting to `0` / empty for legacy files)**

- `stat_health`, `stat_defense`, `stat_attack`, `dpr`, `stat_strength`: Mirrored composite stats from upstream `stats.`*
- `loca_id`, `faction`, `upstream_ship_type`, `hull_type_raw`, `rarity`, `is_scout`, `is_outpost`, `strength`, `systems`, `xp_amount`, `warp`, `warp_with_superhighway`

**Raw payloads preserved for normalization and future mechanics**

- `components`: Full upstream `components` JSON array (weapons, shield, warp, etc.); weapon rows feed sub-round / counter-attack construction where applicable
- `ability`: Full upstream `ability` array
- `resources`: Parsed drop ranges from upstream `resources[]`

**Upstream `ship_type` (u32):** Separate from hull line (`hull_type` → `ship_class`). Kobayashi maintains a reverse-engineered mapping in `src/data/upstream_hostile_ship_type.rs` and applies it at combat time via `HostileRecord::ship_type_for_combat` (e.g. value `1` = armada target). See [ROADMAP.md](ROADMAP.md) § *Hostile upstream `ship_type`* and [DESIGN.md](DESIGN.md) §3 (LCARS conditions / defender class note).

**Hostile Index** — Lookup catalog in `data/hostiles/index.json`:

- `data_version`: Semver or date string (e.g., `"stfcspace-2026-03-01"`)
- `source_note`: Attribution (e.g., `"stfc.space API (data.stfc.space/hostile/summary.json)"`)
- `hostiles[]`: Array of `HostileIndexEntry` — required: `id`, `hostile_name`, `level`, `ship_class`; optional: `rarity`, `upstream_ship_type`, `loca_id`

**Example hostile record** (abbreviated from `data/hostiles/111884576.json`; `components` / `ability` truncated):

```json
{
  "id": "111884576",
  "hostile_name": "Hostile 111884576",
  "level": 18,
  "ship_class": "explorer",
  "armor": 689.0,
  "shield_deflection": 172.0,
  "dodge": 172.0,
  "hull_health": 2680.0,
  "shield_health": 2010.0,
  "shield_mitigation": 0.8,
  "apex_barrier": 0.0,
  "isolytic_defense": 0.0,
  "loca_id": 10010,
  "upstream_ship_type": 2,
  "hull_type_raw": 3,
  "stat_health": 2345.0,
  "stat_defense": 5165.0,
  "stat_attack": 5468.0,
  "dpr": 3402.0,
  "stat_strength": 12978.0,
  "accuracy": 172.0,
  "armor_piercing": 172.0,
  "shield_piercing": 1033.0,
  "crit_chance": 0.1,
  "crit_damage": 1.5,
  "components": [],
  "ability": []
}
```

---

### Ship Data Model (from `src/data/ship.rs`)

Ships are stored **on disk** under `data/ships_extended/` (extended schema: all tiers + per-level hull/shield bonuses). Those files are **produced by** `cargo run --bin normalize_data_stfc_space`, which reads cached per-ship JSON under `data/upstream/data-stfc-space/ships/` (see `src/bin/normalize_data_stfc_space.rs` and `scripts/fetch_stfcspace_ships.mjs` for the cache layout). The simulator loads an `ExtendedShipRecord` and resolves a flat `ShipRecord` for the requested **tier** and **level** via `ExtendedShipRecord::to_ship_record` (see `src/data/loader.rs` — the old flat `data/ships/` tree is removed).

**Extended ship record** (`ExtendedShipRecord`) — one JSON file per ship, e.g. `data/ships_extended/amalgam.json`:

- `id`, `ship_name`, `ship_class`: same meaning as on `ShipRecord`
- `tiers`: `Vec<TierStats>` — combat scalars **per tier** (weapon aggregates, HP, optional `weapons` list)
- `levels`: `Vec<LevelBonus>` — additive `shield` / `health` bonuses by ship **level** (merged into tier base HP when resolving)
- `crew_slots`: Below-decks slot unlock schedule from upstream (`Vec<CrewSlotUnlock>`); optional, may be empty
- `abilities`: Optional `Vec<ShipAbility>` — hull abilities from upstream (timing, effect type, conditions); resolved onto the flat `ShipRecord` at tier/level resolution

**Per tier** (`TierStats`): `tier`, `armor_piercing`, `shield_piercing`, `accuracy`, `attack`, `crit_chance`, `crit_damage`, `hull_health`, `shield_health`, optional `shield_mitigation`, optional `weapons` (`Vec<WeaponRecord>`).

**Per weapon** (`WeaponRecord`): `attack`; optional `shots`, `pierce` (damage-through override), `armor_piercing`, `shield_piercing`, `accuracy` (per-weapon raw values for mitigation math), `crit_chance`, `crit_multiplier` (JSON name on the weapon row; tier-level scalar is still `crit_damage`), `proc_chance`, `proc_multiplier`.

**Resolved ship record** (`ShipRecord`) — in-memory combat row for one **tier + level** choice:

- `id`, `ship_name`, `ship_class`
- `armor_piercing`, `shield_piercing`, `accuracy`, `attack`, `crit_chance`, `crit_damage`
- `hull_health`, `shield_health` (tier base + level `health` / `shield` bonuses)
- `shield_mitigation`: `Option<f64>` — fraction to shields when shields are up (often `0.8`)
- `apex_shred`, `isolytic_damage`: `f64` (default `0.0`); resolving from `ExtendedShipRecord` currently sets both to `0.0` — hull abilities on `abilities` and profile/research merges in the optimizer scenario can supply non-zero combat values
- `weapons`: `Option<Vec<WeaponRecord>>` — when present, drives `Combatant.weapons` / sub-rounds; when absent, a single implicit weapon uses scalar `attack`
- `abilities`: `Option<Vec<ShipAbility>>` — copy of hull abilities for that resolution (level-scaled `values[]` curves folded into `value` where applicable)

**Ship index** — `data/ships_extended/index.json` (`ExtendedShipIndex`): `data_version`, `source_note`, `ships[]` with `ExtendedShipIndexEntry` (`id`, `ship_name`, `ship_class`).

**Example extended ship** (abbreviated from `data/ships_extended/amalgam.json`; one tier; shortened `levels` / `crew_slots`; hull `abilities` omitted in this snippet — the real file has the full tier ladder and optional `abilities`):

```json
{
  "id": "amalgam",
  "ship_name": "AMALGAM",
  "ship_class": "survey",
  "tiers": [
    {
      "tier": 1,
      "armor_piercing": 477.0,
      "shield_piercing": 477.0,
      "accuracy": 477.0,
      "attack": 1814.5,
      "crit_chance": 0.1,
      "crit_damage": 1.5,
      "hull_health": 16612.0,
      "shield_health": 9600.0,
      "shield_mitigation": 0.8,
      "weapons": [
        {
          "attack": 1814.5,
          "shots": 1,
          "armor_piercing": 477.0,
          "shield_piercing": 477.0,
          "accuracy": 477.0,
          "crit_chance": 0.1,
          "crit_multiplier": 1.5
        }
      ]
    }
  ],
  "levels": [
    { "level": 1, "shield": 100.32, "health": 456.657 }
  ],
  "crew_slots": [
    { "slots": "1", "unlock_level": 5 },
    { "slots": "2", "unlock_level": 10 }
  ]
}
```

---

## Part 2: data.stfc.space API surface (accuracy from repo tooling)

**Base URL:** `https://data.stfc.space`

**Canonical patterns** (see `scripts/fetch_stfcspace_page_upstream.mjs` and `scripts/lib/stfcspace_detail_fetch.mjs`):

- **Catalog / summary:** `GET /{segment}/summary.json` → JSON array (or object) listing ids and metadata for that domain.
- **Detail record:** `GET /{segment}/{numeric_id}.json` → one entity (ship, hostile, officer, research row, forbidden tech, …). Segment names are **singular** in the API path (`ship`, `hostile`, `building`, …).
- **Translations (English):** `GET /translations/en/{category}.json` → string map used by the stfc.space SPA.

**Local mirror** (after running the fetch scripts from repo root): `data/upstream/data-stfc-space/`. Summaries are saved as `summary-{name}.json` (e.g. `summary-ship.json`, `summary-hostile.json`). Translation mirrors use the prefix `translations-{category}.json` (e.g. `translations-materials.json`), not a nested `translations/en/` directory.

### Summary catalogs (`/{segment}/summary.json`)

These segments are fetched by `fetch_stfcspace_page_upstream.mjs` and written to the filenames below. “Used by Kobayashi” means a maintainer-facing script or Rust normalizer reads the cached summary (or the live URL in a few importers).


| Segment (`{segment}`) | Remote URL                     | Cached as                     | Kobayashi usage                                                                           |
| --------------------- | ------------------------------ | ----------------------------- | ----------------------------------------------------------------------------------------- |
| `ship`                | `/ship/summary.json`           | `summary-ship.json`           | Ship id registry + `fetch_stfcspace_ships.mjs` id list; feeds `normalize_data_stfc_space` |
| `hostile`             | `/hostile/summary.json`        | `summary-hostile.json`        | `fetch_stfcspace_hostiles.mjs`; feeds `normalize_hostiles_stfc_space`                     |
| `building`            | `/building/summary.json`       | `summary-building.json`       | `import_stfcspace_buildings.mjs` (live or `--from-upstream`)                              |
| `research`            | `/research/summary.json`       | `summary-research.json`       | `fetch_stfcspace_research.mjs`; feeds `import_stfcspace_research.mjs`                     |
| `officer`             | `/officer/summary.json`        | `summary-officer.json`        | `fetch_stfcspace_officers.mjs` (reference cache only; combat stays LCARS)                 |
| `forbidden_tech`      | `/forbidden_tech/summary.json` | `summary-forbidden_tech.json` | `fetch_stfcspace_forbidden_tech.mjs`; forbidden-tech import tooling                       |
| `system`              | `/system/summary.json`         | `summary-system.json`         | Cached for reference; not wired into core combat loaders                                  |
| `consumable`          | `/consumable/summary.json`     | `summary-consumable.json`     | Cached for reference                                                                      |
| `hazards`             | `/hazards/summary.json`        | `summary-hazards.json`        | Cached for reference                                                                      |
| `wave_defense`        | `/wave_defense/summary.json`   | `summary-wave_defense.json`   | Cached for reference                                                                      |
| `pvp_bands`           | `/pvp_bands/summary.json`      | `summary-pvp_bands.json`      | Cached for reference                                                                      |
| `mission`             | `/mission/summary.json`        | `summary-mission.json`        | Cached for reference                                                                      |
| `resource`            | `/resource/summary.json`       | `summary-ressource.json`      | Filename matches upstream spelling (`ressource`); cached for reference                    |


### Detail JSON (`/{segment}/{id}.json`)


| Segment          | Example URL                 | Local cache dir                        | Fetch script(s)                                                                           |
| ---------------- | --------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------------- |
| `ship`           | `/ship/{id}.json`           | `data/upstream/data-stfc-space/ships/` | `fetch_stfcspace_ships.mjs` (also one sample ship in `fetch_stfcspace_page_upstream.mjs`) |
| `hostile`        | `/hostile/{id}.json`        | `…/hostiles/`                          | `fetch_stfcspace_hostiles.mjs`                                                            |
| `officer`        | `/officer/{id}.json`        | `…/officers/`                          | `fetch_stfcspace_officers.mjs`                                                            |
| `research`       | `/research/{rid}.json`      | `…/research/`                          | `fetch_stfcspace_research.mjs`                                                            |
| `forbidden_tech` | `/forbidden_tech/{id}.json` | `…/forbidden_tech/`                    | `fetch_stfcspace_forbidden_tech.mjs`                                                      |


**Batch driver:** `fetch_stfcspace_details.mjs --entities ships,hostiles,officers,research,forbidden_tech` runs the same detail logic as the per-entity scripts.

**Buildings:** Kobayashi’s building importer (`import_stfcspace_buildings.mjs`) is driven by `**/building/summary.json`** (plus `translations-starbase_modules.json` for names). There is **no** separate `fetch_stfcspace_buildings.mjs` detail pass in tree; extend here if per-building detail URLs become necessary.

### Translation bundles (`/translations/en/{category}.json`)

Fetched by `fetch_stfcspace_page_upstream.mjs` into `translations-{category}.json`. Categories in script include: `materials`, `ships`, `officers`, `officer_names`, `officer_buffs`, `officer_flavor_text`, `traits`, `research`, `starbase_modules`, `factions`, `systems`, `ship_components`, `blueprints`, `consumables`, `mission_titles`, `navigation`, `ship_buffs`, `loyalty`, `forbidden_tech`, `event_titles`, `player_avatars`, `hud`.

`translations-materials.json` is for **materials / items**, not hostile encounter titles. There is **no** `hostiles` category in that fetch list today; resolving `loca_id` → human hostile name still needs a verified mapping (custom export, another category, or a future API addition).

### Coverage assessment (point-in-time; refresh changes counts)

Derived from the **checked-in** `data/upstream/data-stfc-space/summary-hostile.json` and `data/hostiles/index.json` in this repo:


| Metric                                    | Value (example snapshot)                  |
| ----------------------------------------- | ----------------------------------------- |
| Rows in `summary-hostile.json`            | 4,968                                     |
| Entries in `data/hostiles/index.json`     | 4,930                                     |
| Rows in `summary-ship.json`               | 113                                       |
| Ships in `data/ships_extended/index.json` | 113 (aligned with upstream ship list)     |
| Hostiles with `rarity` ≥ 2 in summary     | 544 (200 rare + 178 epic + 166 legendary) |


Recompute anytime with: `node -e "console.log(require('./data/upstream/data-stfc-space/summary-hostile.json').length)"` (and similarly for `index.json`).

**What stfc.space data adds vs legacy STFCcommunity baselines:**

- Full hostile ladder (levels through **81** in current summaries), rare tiers, `ability[]` payloads, and offensive stats on `HostileRecord` (counter-fire / two-way combat).
- Structured ship JSON for **all tiers** and level curves consumed by `normalize_data_stfc_space`.

**Remaining product gaps (data, not API “unknown”):**

- **Hostile display names:** `normalize_hostiles_stfc_space` still emits `Hostile {id}` until a trusted `loca_id` → string map exists; `materials` translations are the wrong namespace for that.

---

## Part 3: Discovery, refresh, and verification workflow

**Part 2** documents the **known-good URL surface** the repo already calls. **Part 3** is how you keep that surface current, prove it still matches production, and extend Kobayashi when upstream JSON gains new fields or segments.

### 3.1 Standard refresh (happy path)

Run from repo root unless a script says otherwise.

1. **Pull catalogs + translations** (cheap, single sweep):
  ```bash
   node scripts/fetch_stfcspace_page_upstream.mjs
  ```
   Writes `data/upstream/data-stfc-space/summary-*.json`, `translations-*.json`, and (by default) one sample `ships/{id}.json`.
2. **Pull detail JSON** for entities you care about (rate-limited; large):
  ```bash
   node scripts/fetch_stfcspace_details.mjs --entities ships,hostiles --missing-only
  ```
   Or the individual scripts in `scripts/fetch_stfcspace_*.mjs` (ships, hostiles, officers, research, forbidden_tech). Logs land in `data/import_logs/fetch-stfcspace-*-YYYY-MM-DD.json`.
3. **Normalize into Kobayashi datasets** — either stepwise (`python3 scripts/build_ship_registry.py`, `cargo run --bin normalize_data_stfc_space`, `cargo run --bin normalize_hostiles_stfc_space`, `node scripts/import_stfcspace_buildings.mjs --from-upstream`, research import, etc.) or the orchestrated path:
  ```bash
   npm run data:refresh -- --stfcspace
  ```
   `data-refresh.mjs` **requires** `summary-ship.json` and `translations-ships.json` before it runs the ship pipeline; hostiles and research steps **skip** unless the corresponding cache directories / summaries exist (see comments in `scripts/data-refresh.mjs`).

### 3.2 Verification (prove the pipeline, not the browser)

- **Counts:** After refresh, compare row counts in `summary-*.json` vs file counts under `ships/`, `hostiles/`, `research/`, and vs Kobayashi indices (`data/ships_extended/index.json`, `data/hostiles/index.json`). Mismatches usually mean an incomplete fetch or a normalizer filter.
- **Rust validation:** `cargo run --bin validate_data` (and `cargo test` for integration coverage) catch schema drift on normalized outputs.
- **Spot-check detail shape:** Pick one id from `summary-hostile.json`, open `data/upstream/data-stfc-space/hostiles/{id}.json`, and confirm `stats`, `components`, and `ability` still match what `normalize_hostiles_stfc_space` and `src/data/hostile.rs` expect.

### 3.3 Discovering new fields or segments

When Scopely / stfc.space adds columns you do not yet map:

1. **Diff** a detail file before and after an upstream refresh (or compare two ids at the same level).
2. **Record** the finding in **Part 4** (mapping table) or in a short note beside the normalizer (`src/bin/normalize_*_stfc_space.rs`), including whether the value is observed, inferred, or assumed.
3. **Optional hand log:** There is no required `stfcspace-endpoint-discovery.json` in tree today; if you maintain one under `data/import_logs/`, keep it as **human intent + sample keys**, not a second copy of whole JSON blobs.

**Fetch-script extensions:** New summary domains appear in the stfc.space client bundle from time to time. Add another `["segment", "summary-filename.json"]` row to `SUMMARY_SEGMENTS` in `fetch_stfcspace_page_upstream.mjs`, re-run the script, and decide whether Kobayashi should consume it or only archive it. New translation categories get a string in `TRANSLATION_PATHS` the same way.

**Ability semantics:** New combat-relevant `ability[]` / hull effects usually need rows in `data/upstream/data-stfc-space/hostile_ability_catalog.json` or `ship_ability_catalog.json` (and sometimes resolver work in `src/data/ship_ability_resolve.rs`). Discovery is not “find a URL” — it is **catalog + engine** alignment.

### 3.4 Optional manual URL probes

Only needed when you suspect **CDN / path renames** outside the SPA bundle this repo was derived from (e.g. plural `GET /ships/summary.json`). The production app and Kobayashi fetchers use **singular** segments (`/ship/`, `/hostile/`). If a probe 404s, treat that as negative evidence and do **not** wire plural guesses into scripts without confirming stfc.space still serves them.

### 3.5 Open discovery targets (product gaps, not mystery URLs)

- **Hostile `loca_id` → display string:** No `hostiles` entry in the translation fetch list today (Part 2). Discovery work is locating the correct string table (game dump, another translation category, or a new API field) — not guessing path variants of `summary.json`.

---

## Part 4: Data mapping strategy (implemented behavior)

This section matches the **checked-in normalizers**, not a wishlist. When upstream JSON changes, update the code first, then mirror the behavior here.


| Binary / script                                 | Reads                                                                                                       | Writes                                                                                               |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `cargo run --bin normalize_hostiles_stfc_space` | `data/upstream/data-stfc-space/hostiles/*.json`                                                             | `data/hostiles/<id>.json`, `data/hostiles/index.json`, merge `data/registry.json` (`hostiles` entry) |
| `cargo run --bin normalize_data_stfc_space`     | `data/upstream/data-stfc-space/ships/*.json`, `ship_id_registry.json`, optional `ship_ability_catalog.json` | `data/ships_extended/<id>.json`, `data/ships_extended/index.json`                                    |
| `node scripts/import_stfcspace_buildings.mjs`   | `summary-building.json` (+ translations) live or cached                                                     | `data/buildings/*.json` (see script tables)                                                          |
| `node scripts/import_stfcspace_research.mjs`    | `summary-research.json` + `research/*.json`                                                                 | `data/research_catalog.json` (see script)                                                            |


### Hostile mapping (stfc.space → `HostileRecord`)

**Input:** one cached detail file per numeric id (same `id` as in `summary-hostile.json`). **Implementation:** `src/bin/normalize_hostiles_stfc_space.rs` + `hull_type_raw_to_ship_class` in `src/data/hostile.rs`.

**Top-level fields**


| Upstream (detail JSON)                                                                                   | KOBAYASHI field      | Notes                                                                                                                                             |
| -------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id` (number)                                                                                            | `id`                 | **Decimal string**, e.g. `"2918121098"` — same id as stfc.space, not a legacy `explorer_30` slug                                                  |
| —                                                                                                        | `hostile_name`       | Placeholder `Hostile {id}` until `loca_id` → name pipeline exists (Part 2 / Part 3)                                                               |
| `level`                                                                                                  | `level`              | Coerced to `u32`                                                                                                                                  |
| `hull_type`                                                                                              | `ship_class`         | Via `hull_type_raw_to_ship_class`: `0` battleship, `1` survey, `2` interceptor, `3` explorer, `5` survey; unknown → `battleship` + stderr warning |
| `hull_type`                                                                                              | `hull_type_raw`      | Raw copy                                                                                                                                          |
| `ship_type`                                                                                              | `upstream_ship_type` | Hostile **category** enum (armada/swarm/etc. semantics in `upstream_hostile_ship_type.rs`), distinct from hull line                               |
| `faction`                                                                                                | `faction`            | `HostileFactionRef { id, loca_id }` passthrough                                                                                                   |
| `loca_id`                                                                                                | `loca_id`            | Optional `u64`                                                                                                                                    |
| `rarity`, `is_scout`, `is_outpost`, `strength`, `systems`, `xp_amount`, `warp`, `warp_with_superhighway` | same / typed         | `systems` → `Vec<u64>`; `strength` from top-level field                                                                                           |
| `resources`                                                                                              | `resources`          | `Vec<HostileResourceDrop>`                                                                                                                        |
| `components`                                                                                             | `components`         | Full `serde_json::Value` array preserved                                                                                                          |
| `ability` (API key)                                                                                      | `ability`            | Same array preserved (combat interpretation still evolving)                                                                                       |


`**stats` object** (all optional; default `0.0` when missing)


| Upstream `stats.*`                               | KOBAYASHI field                                                      |
| ------------------------------------------------ | -------------------------------------------------------------------- |
| `armor`                                          | `armor`                                                              |
| `absorption`                                     | `shield_deflection`                                                  |
| `dodge`                                          | `dodge`                                                              |
| `hull_hp`                                        | `hull_health`                                                        |
| `shield_hp`                                      | `shield_health`                                                      |
| `accuracy`, `armor_piercing`, `shield_piercing`  | same names                                                           |
| `critical_chance`, `critical_damage`             | `crit_chance`, `crit_damage`                                         |
| `health`, `defense`, `attack`, `dpr`, `strength` | `stat_health`, `stat_defense`, `stat_attack`, `dpr`, `stat_strength` |


**Shield mitigation:** first `components[].data` with `tag == "Shield"` and numeric `mitigation` → `shield_mitigation: Some(value)`; else `None` (engine default 0.8 when absent on load).

**Not sourced from stfc.space today:** `apex_barrier`, `isolytic_defense`, mitigation floor/ceiling / mystery factor — left at defaults / `None` unless hand-edited in normalized JSON.

`**components` weapons:** full JSON kept on disk; counter-attack weapon parsing for combat is in `HostileRecord::weapon_stats_from_component_data` (`src/data/hostile.rs`) — aggregate pierce lives on `AttackerStats`, not on each `WeaponStats.pierce`.

**Hull type frequency** (from checked-in `summary-hostile.json`, **4,968** rows — recompute after refresh):


| `hull_type` | `ship_class` | Count |
| ----------- | ------------ | ----- |
| 0           | battleship   | 1,414 |
| 1           | survey       | 369   |
| 2           | interceptor  | 1,373 |
| 3           | explorer     | 1,214 |
| 5           | survey       | 598   |


`**ship_type` frequency** (same summary snapshot; encounter category, **not** hull class):


| `ship_type` | Count | `ship_type` | Count |
| ----------- | ----- | ----------- | ----- |
| 5           | 1,171 | 10          | 167   |
| 7           | 1,184 | 11          | 111   |
| 2           | 1,025 | 12          | 84    |
| 1           | 598   | 13          | 28    |
| 8           | 436   | 14          | 42    |
| 4           | 115   | 0           | 3     |
| 6           | 1     | 3           | 3     |


*(Rare enums 0, 3, 6 appear in data; treat labels as heuristic until catalogued.)*

### Ship mapping (stfc.space → `ExtendedShipRecord`)

**Input:** `data/upstream/data-stfc-space/ships/{numeric_id}.json`. **Identity row** comes from `data/upstream/data-stfc-space/ship_id_registry.json` (`numeric_id` → Kobayashi `id`, `ship_name`, `ship_class`). The normalizer **does not** rename ships from arbitrary upstream `name` fields on the detail file — registry strings win.

**Implementation:** `src/bin/normalize_data_stfc_space.rs` (`raw_to_extended`, `extract_tier_combat`).

**Per tier:** each `tiers[]` element must have `tier` (number) and `components[]`. Combat extraction walks `components[].data` by `tag`:


| Component `tag` | Upstream fields used                                                                                                                                                                                       | `TierStats` / weapons                                                                                                                                                                                                                                                                                                                                                                                                                              |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Weapon`        | `order` (sort key; missing / negative → last), `penetration`, `modulation`, `accuracy`, `minimum_damage`, `maximum_damage`, `shots`, `crit_chance`, `crit_modifier` or `crit_damage`, optional proc fields | One `WeaponRecord` per weapon: per-weapon `armor_piercing` = `penetration`, `shield_piercing` = `modulation`, `accuracy`; `attack` = average of min/max damage; `shots` ≥ 1. Tier scalars: **sum** pierce/accuracy across weapons, then **divide by weapon count** for `armor_piercing`, `shield_piercing`, `accuracy`; `attack` = Σ(avg_damage × shots). Tier `crit_chance` / `crit_damage` taken from the **first** weapon after sort (primary). |
| `Shield`        | `hp`, `mitigation`                                                                                                                                                                                         | `shield_health`, `shield_mitigation` (defaults to `0.8` if tag missing mitigation)                                                                                                                                                                                                                                                                                                                                                                 |
| `Armor`         | `hp`                                                                                                                                                                                                       | `hull_health`                                                                                                                                                                                                                                                                                                                                                                                                                                      |


**Fallbacks inside `extract_tier_combat`:** if no weapons produced positive damage, `attack` → `100.0`; if `shield_health` ≤ 0 → `1000.0`; if `hull_health` ≤ 0 → `2 × shield_health`. Empty weapon list → `weapons: None`; else `Some(Vec<WeaponRecord>)` with `pierce: None` at tier level.

`**levels[]`:** each `{ level, shield, health }` → `LevelBonus` (additive shield/hull at that ship level).

`**crew_slots[]`:** `{ slots?, unlock_level }` → `CrewSlotUnlock` list.

`**ability[]`:** only rows whose stringified `id` exists in `data/upstream/data-stfc-space/ship_ability_catalog.json` `entries` become `ShipAbility` (timing, effect type, percentage handling, conditions, optional `values_scale_with_ship_level` curves). Uncatalogued ids are skipped silently.

**Resolved `ShipRecord`:** built at runtime via `ExtendedShipRecord::to_ship_record(tier, level)` (`src/data/ship.rs`). API and optimizer accept **tier** and **level**; when omitted, loaders default to tier **1** and level **1** — not hard-coded to tier 1 only.

### Buildings & research (brief)

- **Buildings:** `import_stfcspace_buildings.mjs` maps a small allowlisted set of stfc.space building buffs to engine stats; see script header `BUILDING_META` / `BUFF_MAPPING` and `data/buildings/buff_id_to_stat.json` for extensions.
- **Research:** `import_stfcspace_research.mjs` merges per-`rid` upstream research JSON into `data/research_catalog.json` using buff-id mapping tables in the script; use `--dump-unmapped` to print unmapped buff id counts.

---

## Part 5: Freshness tracking & versioning

This section reflects **what the repo actually writes today**, not a future importer.

### Where version strings live


| Location                         | Fields                                                       | Purpose                                                                                                              |
| -------------------------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `data/hostiles/index.json`       | `data_version`, `source_note`                                | Human-readable hostile dataset stamp                                                                                 |
| `data/ships_extended/index.json` | `data_version`, `source_note`                                | Human-readable ship extended dataset stamp                                                                           |
| `data/registry.json`             | Per-dataset `source`, `data_version`, `last_updated`, `path` | Loader-facing registry (`hostiles` → `hostiles/index.json`; `**ships`** entry points at `ships_extended/index.json`) |


There is **no** `data/ships/index.json` — flat ships were removed; ships live under `**data/ships_extended/`**.

### `data_version` and `source_note` (implemented)

**Hostiles** (`normalize_hostiles_stfc_space`):

- `data_version`: from env `STFCSPACE_HOSTILES_VERSION`, else default `stfcspace-hostiles-{UTC_YYYY-MM-DD}`.
- `source_note`: from env `STFCSPACE_HOSTILES_SOURCE_NOTE`, else `"data.stfc.space hostile detail (cached under data/upstream/data-stfc-space/hostiles)"`.

**Ships extended** (`normalize_data_stfc_space`):

- Currently **hardcoded** in the binary: `data_version` = `"data-stfc-space"`, `source_note` = `"From normalize_data_stfc_space"` (no env override in that normalizer).

The doc’s older suggestion of a bare `stfcspace-YYYY-MM-DD` string is only a **style guideline**; live data may use prefixes (`stfcspace-hostiles-…`) or the ship normalizer’s fixed label until someone threads env vars through `normalize_data_stfc_space`.

### Fields **not** implemented

The following do **not** appear in index JSON or normalizers today:

- `last_imported_at`
- `import_url`

`**registry.json` `last_updated`:** set when hostile normalizer runs (`merge_registry_hostiles` uses the run date). The ships registry row is **not** automatically bumped by `normalize_data_stfc_space` in the same way—maintainers may update `data/registry.json` manually or via other tooling; do not assume ship and hostile `last_updated` move in lockstep.

### How “freshness” actually works

1. **Upstream cache (Node):** `fetch_stfcspace_*.mjs` decides **missing-only** vs **full** re-download per detail file. There is **no** hash comparison against `data/hostiles/index.json` in those scripts.
2. **Normalize (Rust / Node):** `normalize_hostiles_stfc_space` and `normalize_data_stfc_space` **re-read whatever JSON is present** under `data/upstream/data-stfc-space/{hostiles,ships}/` and rewrite `**data/hostiles/*.json` + index** or `**data/ships_extended/*.json` + index** wholesale for the parsed set. They do not diff per-id against a prior index.
3. **Logs:** Fetch runs emit `data/import_logs/fetch-stfcspace-{ships|hostiles|…}-YYYY-MM-DD.json`. Building imports emit `buildings-stfcspace-*.json`. There is **no** standard `stfcspace-import-YYYY-MM-DD.json` file produced by current tooling.

**Practical workflow:** refresh upstream caches (Part 3), rerun normalizers, bump `STFCSPACE_HOSTILES_VERSION` / `STFCSPACE_HOSTILES_SOURCE_NOTE` when you need an explicit provenance change, commit the resulting `index.json` + `registry.json` diffs.

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


| Risk                                           | Likelihood | Impact               | Mitigation                                                                           |
| ---------------------------------------------- | ---------- | -------------------- | ------------------------------------------------------------------------------------ |
| **API endpoint not found**                     | Medium     | Blocker (Phase 1)    | Test URLs early; check community Discord for endpoint docs                           |
| **Field names differ widely**                  | Medium     | 4–6h delay (Phase 2) | Build flexible mapping registry; log unmapped fields                                 |
| **Numeric values scaled unexpectedly**         | Medium     | Incorrect sims       | Add scale conversion step; validate against known boss stats                         |
| **Rate limiting or downtime**                  | Low        | Delay                | Implement exponential backoff; cache responses locally                               |
| **Data quality issues** (missing stats, nulls) | Medium     | Validation failures  | Graceful skipping with detailed logging; human review before merge                   |
| **Breaking changes in API**                    | Low–Medium | Re-work mapping      | Document API contract in import logs; version data by date                           |
| **Merge conflicts with manual edits**          | Low        | Rebase friction      | Keep manual edits separate from auto-imported data; use `source_note` to distinguish |


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