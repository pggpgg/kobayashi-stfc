# data.stfc.space Import Strategy for KOBAYASHI

## Overview

This document describes how Kobayashi imports ship and hostile data from **data.stfc.space** (the stfc.space backend API) into `**data/ships_extended/`** and `**data/hostiles/`**, alongside optional buildings/research importers. The **core pipeline is implemented** (Node fetch + Rust normalizers). Remaining work is mostly **backlog** (display names, catalogs), with automated **summary drift detection** in CI and weekly refresh PRs (see Part 7). Legacy **STFCcommunity** path kept optional for older baselines.

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

**Upstream `ship_type` (u32):** Separate from hull line (`hull_type` → `ship_class`). Kobayashi maintains a reverse-engineered mapping in `src/data/upstream_hostile_ship_type.rs` and applies it at combat time via `HostileRecord::ship_type_for_combat` (e.g. value `1` = armada target). Enumerated ids and evidence: [UPSTREAM_HOSTILE_SHIP_TYPES.md](UPSTREAM_HOSTILE_SHIP_TYPES.md). See also [ROADMAP.md](ROADMAP.md) § *Hostile upstream `ship_type`* and [DESIGN.md](DESIGN.md) §3 (LCARS conditions / defender class note).

**Hostile Index** — Lookup catalog in `data/hostiles/index.json`:

- `data_version`: Opaque string (e.g. default hostile stamp `stfcspace-hostiles-YYYY-MM-DD`, or override via `STFCSPACE_HOSTILES_VERSION` — see Part 5)
- `source_note`: Attribution (default hostile text points at cached upstream `hostiles/` — see Part 5)
- `hostiles[]`: Array of `HostileIndexEntry` — required: `id`, `hostile_name`, `level`, `ship_class`; optional: `rarity`, `upstream_ship_type`, `loca_id`

**Example hostile record** (abbreviated from `data/hostiles/111884576.json`; `components` / `ability` truncated):

```json
{
  "id": "111884576",
  "hostile_name": "Hostile 111884576",
  "level": 18,
  "ship_class": "battleship",
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

**Input:** one cached detail file per numeric id (same `id` as in `summary-hostile.json`). **Implementation:** `src/bin/normalize_hostiles_stfc_space.rs` + [`hostile_hull_type_raw_to_ship_class`](../src/data/hostile.rs) in `src/data/hostile.rs` (player ships use [`player_hull_type_raw_to_ship_class`](../src/data/hostile.rs) in `scripts/normalize_stfcspace_ships.mjs` via the same integer→class table).

**Top-level fields**


| Upstream (detail JSON)                                                                                   | KOBAYASHI field      | Notes                                                                                                                                             |
| -------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id` (number)                                                                                            | `id`                 | **Decimal string**, e.g. `"2918121098"` — same id as stfc.space, not a legacy `explorer_30` slug                                                  |
| —                                                                                                        | `hostile_name`       | Placeholder `Hostile {id}` until `loca_id` → name pipeline exists (Part 2 / Part 3)                                                               |
| `level`                                                                                                  | `level`              | Coerced to `u32`                                                                                                                                  |
| `hull_type`                                                                                              | `ship_class`         | Via [`hostile_hull_type_raw_to_ship_class`](../src/data/hostile.rs) (aligned with client combat triangle): `0` interceptor, `1` survey, `2` explorer, `3` battleship, `4`/`5` survey; unknown → `battleship` + stderr warning |
| `hull_type`                                                                                              | `hull_type_raw`      | Raw copy                                                                                                                                          |
| `ship_type`                                                                                              | `upstream_ship_type` | Hostile **category** enum (armada/swarm/etc. semantics in `upstream_hostile_ship_type.rs`), distinct from hull line                               |
| `faction`                                                                                                | `faction`            | `HostileFactionRef { id, loca_id }` passthrough                                                                                                   |
| `loca_id`                                                                                                | `loca_id`            | Optional `u64`                                                                                                                                    |
| `rarity`, `is_scout`, `is_outpost`, `strength`, `systems`, `xp_amount`, `warp`, `warp_with_superhighway` | same / typed         | `systems` → `Vec<u64>`; `strength` from top-level field                                                                                           |
| `resources`                                                                                              | `resources`          | `Vec<HostileResourceDrop>`                                                                                                                        |
| `components`                                                                                             | `components`         | Full `serde_json::Value` array preserved                                                                                                          |
| `ability` (API key)                                                                                      | `ability`            | Same array preserved (combat interpretation still evolving)                                                                                       |


`**stats` object** (all optional; default `0.0` when missing)


| Upstream `stats.`*                               | KOBAYASHI field                                                      |
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


| `hull_type` | `ship_class` (after mapping) | Count |
| ----------- | ---------------------------- | ----- |
| 0           | interceptor                  | 1,414 |
| 1           | survey                       | 369   |
| 2           | explorer                     | 1,373 |
| 3           | battleship                   | 1,214 |
| 5           | survey                       | 598   |


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


| Location                         | Fields                                                       | Purpose                                                                                                     |
| -------------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `data/hostiles/index.json`       | `data_version`, `source_note`                                | Human-readable hostile dataset stamp                                                                        |
| `data/ships_extended/index.json` | `data_version`, `source_note`                                | Human-readable ship extended dataset stamp                                                                  |
| `data/registry.json`             | Per-dataset `source`, `data_version`, `last_updated`, `path` | Loader-facing registry (`hostiles` → `hostiles/index.json`; `ships` key → path `ships_extended/index.json`) |


There is **no** `data/ships/index.json` — flat ships were removed; ships live under `**data/ships_extended/`**.

### `data_version` and `source_note` (implemented)

**Hostiles** (`normalize_hostiles_stfc_space`):

- `data_version`: from env `STFCSPACE_HOSTILES_VERSION`, else default `stfcspace-hostiles-{UTC_YYYY-MM-DD}`.
- `source_note`: from env `STFCSPACE_HOSTILES_SOURCE_NOTE`, else `"data.stfc.space hostile detail (cached under data/upstream/data-stfc-space/hostiles)"`.

**Ships extended** (`normalize_data_stfc_space`):

- `data_version`: from env `STFCSPACE_SHIPS_VERSION`, else default `stfcspace-ships-{UTC_YYYY-MM-DD}`.
- `source_note`: from env `STFCSPACE_SHIPS_SOURCE_NOTE`, else `"From normalize_data_stfc_space"`.
- `data/registry.json` `ships` row: updated on each normalize via `merge_registry_entry` (same `data_version` + run date as `last_updated`).

The doc’s older suggestion of a bare `stfcspace-YYYY-MM-DD` string is only a **style guideline**; live data uses prefixed stamps (`stfcspace-hostiles-…`, `stfcspace-ships-…`).

### Fields **not** implemented

The following do **not** appear in index JSON or normalizers today:

- `last_imported_at`
- `import_url`

`**registry.json` `last_updated`:** set when hostile or ship normalizers run (`merge_registry_entry` uses the run date). Both `hostiles` and `ships` rows update on their respective normalize passes.

### How “freshness” actually works

1. **Upstream cache (Node):** `fetch_stfcspace_*.mjs` decides **missing-only** vs **full** re-download per detail file. There is **no** hash comparison against `data/hostiles/index.json` in those scripts.
2. **Normalize (Rust / Node):** `normalize_hostiles_stfc_space` and `normalize_data_stfc_space` **re-read whatever JSON is present** under `data/upstream/data-stfc-space/{hostiles,ships}/` and rewrite `data/hostiles/*.json` + `data/hostiles/index.json`, or `data/ships_extended/*.json` + `data/ships_extended/index.json`, wholesale for the parsed set. They do not diff per-id against a prior index.
3. **Logs:** Fetch runs emit `data/import_logs/fetch-stfcspace-{ships|hostiles|…}-YYYY-MM-DD.json`. Building imports emit `buildings-stfcspace-*.json`. There is **no** standard `stfcspace-import-YYYY-MM-DD.json` file produced by current tooling.

**Practical workflow:** refresh upstream caches (Part 3), rerun normalizers, bump `STFCSPACE_HOSTILES_VERSION` / `STFCSPACE_HOSTILES_SOURCE_NOTE` when you need an explicit provenance change, commit the resulting `index.json` + `registry.json` diffs.

---

## Part 6: Commands and automation (actual surface)

There is **no** unified `kobayashi fetch-data` (or similar) subcommand. Upstream HTTP is handled by **Node** scripts using `fetch()`; normalization and combat data loading are **Rust** binaries. The library CLI used in tests (`src/cli.rs`) exposes `serve`, `simulate`, `optimize`, `import` (roster / Spock’s JSON only), `validate`, `resolve`, `mitigation-sensitivity`—**not** stfc.space bulk download. The **release** binary (`src/main.rs`) adds `generate-lcars` and the same stfc.space–unrelated verbs; neither entry point implements upstream fetch.

### npm scripts (`package.json`)


| Script                                             | What it runs                                                                                        |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `npm run data:refresh`                             | `node scripts/data-refresh.mjs` — optional `--stfccommunity` / `--stfcspace` / `--all` (see Part 3) |
| `npm run fetch:stfcspace:details`                  | `node scripts/fetch_stfcspace_details.mjs` — requires `--entities ships,hostiles,…`                 |
| `npm run import:buildings:stfcspace`               | Live `import_stfcspace_buildings.mjs`                                                               |
| `npm run import:buildings:stfcspace:from-upstream` | Same importer with `--from-upstream`                                                                |


### Node: fetch and import (direct)

Run from repo root; see `--help` on each script.


| Task                                              | Typical command                                                                                                                                                                                                                            |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Refresh summaries + EN translations + sample ship | `node scripts/fetch_stfcspace_page_upstream.mjs`                                                                                                                                                                                           |
| Detail JSON (subset or full)                      | `node scripts/fetch_stfcspace_ships.mjs`, `fetch_stfcspace_hostiles.mjs`, `fetch_stfcspace_research.mjs`, `fetch_stfcspace_officers.mjs`, `fetch_stfcspace_forbidden_tech.mjs`, or `node scripts/fetch_stfcspace_details.mjs --entities …` |
| Buildings → `data/buildings/`                     | `node scripts/import_stfcspace_buildings.mjs` or `--from-upstream`                                                                                                                                                                         |
| Research → `data/research_catalog.json`           | `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0` (after research cache exists)                                                                                                                                       |


Shared HTTP behavior is in `scripts/lib/stfcspace_detail_fetch.mjs` (`BASE_URL`, retries, delay between requests, JSON logs under `data/import_logs/`).

### Rust: normalize and validate


| Binary                                                | Command                                                                 |
| ----------------------------------------------------- | ----------------------------------------------------------------------- |
| Ships → `data/ships_extended/`                        | `cargo run --release --bin normalize_data_stfc_space`                   |
| Hostiles → `data/hostiles/` + registry hostiles entry | `cargo run --release --bin normalize_hostiles_stfc_space`               |
| Optional env (hostiles index metadata)                | `STFCSPACE_HOSTILES_VERSION`, `STFCSPACE_HOSTILES_SOURCE_NOTE` (Part 5) |
| Dataset validation                                    | `cargo run --release --bin validate_data`                               |


### Why not a Rust `reqwest` importer?

Bulk fetch was implemented in **JavaScript** to match the stfc.space SPA’s URL layout, share one retry/logging path across entity types, and avoid pulling HTTP client complexity into the hot-path simulator crate. A future Rust binary could wrap the same URLs, but it would **duplicate** behavior unless the Node scripts are retired deliberately.

---

## Part 7: Roadmap and remaining work

Older drafts of this document described **phased delivery** of a Rust `fetch-data` binary and per-id hash freshness. The **implemented** path is Node fetch scripts + Rust normalizers (Parts 3–6). This section replaces those phases with **current reality** and a **maintainer backlog**.

### What is already in place

- **API surface:** Summaries, detail URLs, and translation bundles are documented in **Part 2**; ship and hostile catalogs are confirmed in production.
- **Upstream cache:** `data/upstream/data-stfc-space/` populated by `fetch_stfcspace_page_upstream.mjs` and `fetch_stfcspace_*.mjs` / `fetch_stfcspace_details.mjs`.
- **Hostiles:** `normalize_hostiles_stfc_space` → `data/hostiles/` + `data/registry.json` (`hostiles` entry).
- **Ships:** `normalize_data_stfc_space` → `data/ships_extended/` (extended tiers/levels, weapons, hull abilities via `ship_ability_catalog.json`).
- **Other importers:** Buildings (`import_stfcspace_buildings.mjs`), research (`import_stfcspace_research.mjs`), optional officer/forbidden-tech caches — see **Part 4**.
- **Combat mapping:** `hull_type` → `ship_class`; hostile `upstream_ship_type` → combat `ShipType` via `src/data/upstream_hostile_ship_type.rs` and `HostileRecord::ship_type_for_combat`; faction tags for LCARS via `opponent_faction_from`_* helpers in `hostile.rs`.

### Backlog (highest value first)


| Priority | Item                              | Notes                                                                                                                                                                                                                                             |
| -------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1        | **Hostile display names**         | `loca_id` is stored; normalizer still sets `hostile_name` to `Hostile {id}`. Need a verified string source (translation category, game dump, or new upstream field). The standard translation fetch list has **no** `hostiles` category (Part 2). |
| 2        | **Provenance parity**             | ~~Ships `registry.json` row~~ *shipped 2026-06-14* — `normalize_data_stfc_space` now calls `merge_registry_entry` with env-driven `STFCSPACE_SHIPS_VERSION` / `STFCSPACE_SHIPS_SOURCE_NOTE`. |
| 3        | **Hostile abilities in combat**   | ~~Catalog + resolver shipped 2026-06-14~~ — [`hostile_ability_catalog.json`](../data/upstream/data-stfc-space/hostile_ability_catalog.json) (976 ids), [`hostile_ability_resolve.rs`](../src/data/hostile_ability_resolve.rs) → `defender_crew`, audit [`HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md`](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md). Regen: `python3 scripts/generate_full_hostile_ability_catalog.py`. Remaining gaps: crit floor, multi-stat crit, PvP/armada scope (see audit). |
| 4        | **Research / buildings coverage** | Extend `BUFF_MAPPING` / buff-id maps and research importers as new STFC buff ids appear; use `--dump-unmapped` on research import.                                                                                                                |
| 5        | **CI summary drift gate**         | ~~Optional~~ *shipped 2026-06-14* — CI job `upstream_drift` runs `scripts/check_stfcspace_summary_drift.mjs --check`; weekly [`.github/workflows/data-refresh.yml`](../.github/workflows/data-refresh.yml) opens remediation PRs.                |


### Explicitly deferred (see Part 6)

- **Single Rust binary** for bulk HTTP download (would duplicate Node unless scripts are removed).
- **Per-id hash skip** inside Rust normalizers (current behavior: full rewrite from cached upstream tree).

### Success criteria (for data refreshes)

When refreshing stfc.space data, a merge is in good shape when:

1. `cargo test` and `cargo run --bin validate_data` pass.
2. `data/hostiles/index.json` and `data/ships_extended/index.json` deserialize; spot-check a few ids in-game or via API simulate.
3. Fetch logs under `data/import_logs/` show expected fetched/skipped counts; no unexplained mass HTTP failures.

---

## Part 8: Field mapping gaps and refinement

Mappings live in **code** (`normalize_hostiles_stfc_space`, `normalize_data_stfc_space`, importers), not a standalone `data/stfcspace_mappings.json`. When upstream JSON changes, **edit the normalizer or catalog JSON**, then update **Part 4** in this document.

### Hostiles

- **Naming:** `stats.absorption` → `shield_deflection`; other keys match Part 4. New `stats.`* keys need explicit handling in `RawStats` + `raw_to_record`.
- **Mitigation extras:** `apex_barrier`, `isolytic_defense`, floor/ceiling — not in stfc.space export; remain defaults unless hand-edited per record.
- `**ability[]`:** Stored verbatim; combat semantics may require `hostile_ability_catalog.json` entries (Part 7).
- **Unknown `hull_type`:** Falls back to `battleship` with a warning — add a mapping in `player_hull_type_raw_to_ship_class` / `hostile_hull_type_raw_to_ship_class` if the game introduces a new hull line.

### Ships (extended)

- **Tier combat:** `extract_tier_combat` derives tier scalars from `Weapon` / `Shield` / `Armor` components (Part 4). Per-weapon `attack` uses **mean of min/max damage** × **shots**; tier `crit_`* follows the **first** weapon by `order`.
- **Fallbacks:** If damage sums to zero, attack defaults to `100.0`; empty shield → `1000.0` shield HP; empty hull → `2 × shield_health`. Document any change to these guards in the same PR as the code change.
- **Hull abilities:** Only abilities with ids present in `ship_ability_catalog.json` are emitted; new upstream ability ids require catalog rows (and sometimes resolver work).

### Buildings and research

- **Buildings:** Extend `BUILDING_META`, `BUFF_MAPPING`, and optionally `data/buildings/buff_id_to_stat.json` when new modules or buff ids appear.
- **Research:** Extend mapping in `import_stfcspace_research.mjs`; run with `--dump-unmapped` to list unknown buff ids.

---

## Part 9: Risks and mitigation


| Risk                                                           | Likelihood | Impact                              | Mitigation                                                                                                                                                                   |
| -------------------------------------------------------------- | ---------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Upstream JSON shape changes** (new/renamed fields)           | Medium     | Normalizer skips or mis-parses rows | Re-run `validate_data`; add `serde` defaults; extend `RawStats` / `Value` walks; spot-check one detail file after refresh                                                    |
| **Scale or unit surprises** (e.g. crit as percent vs fraction) | Medium     | Wrong combat numbers                | Compare to in-game or recorded fights; ship catalog already normalizes some `%` flags — mirror for new effect types                                                          |
| **Rate limits / outages**                                      | Low        | Incomplete cache                    | Node fetchers retry with backoff; keep committed cache under `data/upstream/` so CI can run offline                                                                          |
| **Sparse or null stats**                                       | Medium     | Parse warnings, bad defaults        | Normalizers log warnings; `validate_data` + manual review before merge                                                                                                       |
| **Merge conflicts** (hand-edited JSON vs regenerated)          | Low        | Lost edits                          | Prefer editing **catalogs** and **registry-driven ids**; document one-off overrides in commit messages; use `source_note` / `data_version` to mark provenance (Part 5)       |
| **Stale community baseline**                                   | Low        | Confusion about source of truth     | Hostiles/ships for production sims come from **stfc.space pipeline** + `ships_extended` / `hostiles`; legacy STFCcommunity path is optional (`normalize_stfc_data`, Part 11) |


---

## Part 10: Operations and maintenance

### Typical stfc.space refresh (see Part 3)

```bash
node scripts/fetch_stfcspace_page_upstream.mjs
node scripts/fetch_stfcspace_details.mjs --entities ships,hostiles --missing-only
# or: npm run data:refresh -- --stfcspace   (after prerequisites in data-refresh.mjs)

cargo run --release --bin normalize_data_stfc_space
cargo run --release --bin normalize_hostiles_stfc_space
```

Set `STFCSPACE_HOSTILES_VERSION` / `STFCSPACE_HOSTILES_SOURCE_NOTE` when you want the hostile index stamp to reflect a specific refresh (Part 5).

### Review and commit

```bash
git status
git diff data/upstream/data-stfc-space/ data/hostiles/ data/ships_extended/ data/registry.json data/import_logs/
```

Stage **upstream cache** changes only when you intend to commit refreshed JSON; large diffs are normal after `--full` fetches.

### Validate

```bash
cargo test
cargo run --release --bin validate_data
```

Optional spot-check (loads **ship + hostile** from `data/ships_extended/` and `data/hostiles/`):

```bash
cargo run --release -- mitigation-sensitivity amalgam 2918121098
```

(`kobayashi simulate` in the release binary uses fixed placeholder combatants for quick traces; it does **not** take `--ship` / `--hostile` to resolve full records. Use `mitigation-sensitivity` or `optimize --ship … --hostile …` for data-backed checks.)

### Monitoring

- `**data/import_logs/fetch-stfcspace-*.json`:** `fetched` / `skipped` / `failed` per segment; investigate non-zero `failed`.
- **No automated weekly job** is required today; optional CI is listed in Part 7.

### Legacy STFCcommunity data

Optional baseline: `scripts/fetch_stfc_data.ps1` → `normalize_stfc_data` (see `npm run data:refresh -- --stfccommunity`). Do not mix outputs with the stfc.space normalizers without understanding which `data/hostiles` generation you intend to keep.

---

## Part 11: Integration with existing pipelines

### Primary (data.stfc.space — current)

```
fetch_stfcspace_page_upstream.mjs  →  summary-*.json, translations-*.json under data/upstream/data-stfc-space/
fetch_stfcspace_{ships,hostiles,...}.mjs  →  ships/*.json, hostiles/*.json (cached detail)
  →  normalize_data_stfc_space  →  data/ships_extended/
  →  normalize_hostiles_stfc_space  →  data/hostiles/ + registry hostiles entry
optional: import_stfcspace_buildings.mjs, import_stfcspace_research.mjs  →  data/buildings/, data/research_catalog.json
```

Loader and API resolve ships from `**data/ships_extended/**` and hostiles from `**data/hostiles/**` (no flat `data/ships/`).

### Optional (STFCcommunity — older)

```
fetch_stfc_data.ps1  →  data/upstream/stfccommunity-data/
  →  cargo run --bin normalize_stfc_data  →  may write hostiles (and other datasets per that binary)
```

Use one provenance story per dataset; overwriting `data/hostiles/` from two sources without coordination will confuse `source_note` and testing.

---

## Part 12: Success criteria

A data refresh is **successful** when:

1. **Tests:** `cargo test` passes; `validate_data` reports no blocking errors for the touched datasets.
2. **Indices:** `data/hostiles/index.json` and `data/ships_extended/index.json` parse; run `validate_data` (and a quick `mitigation-sensitivity <ship_id> <hostile_id>` if you changed combat fields).
3. **Logs:** Fetch logs show expected `fetched`/`skipped` for the mode used (`missing-only` vs `--full`); failures are explained or fixed.
4. **Provenance:** `data_version` / `source_note` (and `registry.json` where updated) reflect the refresh intent.

Formal CI for “summary JSON drift” is implemented (Part 7 backlog item 5): see `upstream_drift` in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

---

## Appendix: Example fetch log (actual shape)

Detail fetch scripts write logs like `data/import_logs/fetch-stfcspace-hostiles-YYYY-MM-DD.json`:

```json
{
  "started": "2026-04-03T17:47:17.526Z",
  "source": "https://data.stfc.space",
  "segment": "hostile",
  "mode": "missing-only",
  "total": 4968,
  "fetched": 39,
  "skipped": 4929,
  "failed": 0,
  "failures": [],
  "finished": "2026-04-03T17:47:29.434Z"
}
```

`buildings-stfcspace-*.json` logs building imports separately. There is no separate `stfcspace-import-*.json` format for Rust normalizers.

---

## Appendix: Building import as a pattern for buff-style importers

`import_stfcspace_buildings.mjs` remains the reference for **allowlisted** API → engine stat mapping (`BUILDING_META`, `BUFF_MAPPING`, optional `buff_id_to_stat.json`). Hostiles and ships **do not** use that script; they use the Rust normalizers in Part 4. The same **discipline** applies: explicit mapping tables, conservative defaults, and logs for review.

---

## Summary

Kobayashi ingests **data.stfc.space** through a **cached upstream tree** and **Rust normalizers** into `data/hostiles/`, `data/ships_extended/`, and optional buildings/research outputs. The **API surface**, **commands**, **provenance fields**, and **backlog** are documented in Parts 2–7; maintainers refresh caches with Node, regenerate normalized JSON, run **tests + validation**, and commit with clear **version notes** when the game or upstream data changes.