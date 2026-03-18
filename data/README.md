# Combat data (ships, hostiles, buildings)

- **Ships:** Ship list and combat resolution use **`ships_extended/`** only: `ships_extended/index.json` plus per-ship `ships_extended/<id>.json` (tier + level stats). Built from upstream data via `scripts/build_ship_registry.py` and `cargo run --bin normalize_data_stfc_space`. Game hull_id (from sync) → Kobayashi ship id for Roster mode is in **`hull_id_registry.json`** at the data root; regenerate with `node scripts/build_hull_id_registry.mjs`.
- **Hostiles:** Loaded from `hostiles/index.json` plus per-id JSON files in the same directory.
- **Buildings:** Loaded from `buildings/index.json` plus per-building JSON files.

## Provenance

- **Index files** include optional `data_version` and `source_note` (e.g. `"data_version": "stfccommunity-main"`, `"source_note": "STFCcommunity baseline (outdated ~3y)"`). These document where the data came from and help detect drift.
- **Source:** Data is typically produced by the normalizer from STFCcommunity or other community sources. See the normalizer and `data_version` / `source_note` in each index for the current baseline.
- **Validation:** Run the test suite; `data_provenance_and_validation` (or similar) tests that indexes load and optional version fields are present. For manual checks, compare a subset of ship/hostile stats to a known source (e.g. toolbox or game).

## Schema

- **Ships:** See `src/data/ship.rs` (`ExtendedShipRecord`, `ShipRecord`). Extended files in `ships_extended/<id>.json` have `tiers[]` (per-tier combat stats) and `levels[]` (shield/health bonuses); resolved at request time to `ShipRecord` for a given tier/level. Fields include attack, hull_health, shield_health, shield_mitigation, apex_shred, isolytic_damage, etc.
- **Hostiles:** See `src/data/hostile.rs` (`HostileRecord`). Fields include armor, shield_deflection, dodge, hull_health, shield_health, shield_mitigation, apex_barrier, isolytic_defense, etc.
- **Buildings:** See `src/data/building.rs` (`BuildingRecord`). Each building has `levels` with `bonuses` (`stat`, `value`, `operator`, optional `conditions`/`notes`). Index is `data/buildings/index.json` (`BuildingIndex`).

## Buildings: sync and optimizer

Buildings are fully modeled for ship combat; optional and backlog items (station defense, strict validation report, building API/UI) are in [docs/ROADMAP.md](../docs/ROADMAP.md) § Buildings.

- **Where the optimizer reads building state:** `profiles/{profile_id}/buildings.imported.json` (see `profile_index::profile_path(profile_id, BUILDINGS_IMPORTED)`). The scenario loader uses the resolved profile id (default when none specified) and loads that file; building bonuses are merged into the player profile for combat.
- **Ops level for building context:** Building level rows can have `ops_min`/`ops_max`; only rows matching the player’s Operations level are applied. The optimizer infers ops level from **Operations Center** (building id `ops_center`, game bid 0) in the synced buildings list. If sync does not include ops_center or bid 0 does not resolve, ops_level is `None` and all level rows are applied. You can override by setting `profile.ops_level` in the profile JSON so simulation works without sync.
- **Strategy for new buildings:** Game `bid` → Kobayashi building `id` is resolved by `building_bid_resolver::load_bid_to_building_id` using (1) `data/upstream/data-stfc-space/translations-starbase_modules.json` (name match or fallback `building_{bid}` when that id exists in the index), and (2) any index entry whose id is `building_{bid}` is added to the map so sync for a new bid still resolves if that building file exists. To support a new building: run the stfc.space import script so the building appears in the index (and optionally update translations), or add a `building_{bid}.json` and an index entry so the resolver picks it up.
- **Validation:** Building validation warns when a bonus uses a `buff_*` stat (ignored by combat until mapped). Add mappings in `data/buildings/buff_id_to_stat.json` and re-run the import script so combat-relevant bonuses use engine stat names. See `data/upstream/data-stfc-space/BUFF_ID_TO_STAT_NAME.md`.
- **Tooling:** `cargo run --bin building_combat_bonuses` prints theoretical max combat bonuses (all buildings at max). Use `--profile <id>` to print effective combat bonuses for that profile’s synced building levels (ops_level from Operations Center, ShipCombat mode). If the profile has no buildings file or empty list, the binary falls back to “all at max” for that run.

## Research: catalog and optimizer

- **Catalog:** `data/research_catalog.json` (KOBAYASHI schema). Each item has `rid` (game research id), optional `name`, and `levels` (array of `{ level, bonuses: [{ stat, value, operator }] }`). Same engine stat keys as buildings (weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce, shield_mitigation, armor, dodge, damage_reduction). Bonuses are cumulative over levels 1..=player level.
- **Where the optimizer reads research state:** `profiles/{profile_id}/research.imported.json` (synced by the mod; each entry is `rid` + `level`). When building the scenario, the loader calls `merge_research_bonuses_into_profile` so research bonuses are applied to combat. If the catalog is missing or a rid is not in the catalog, that research is skipped (no crash).
- **Refresh from upstream:** Run `node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456]`. The script reads `data/upstream/data-stfc-space/summary-research.json`, fetches per-research detail from data.stfc.space for each node, maps buff ids to engine stats (via `RESEARCH_BUFF_MAPPING` in the script or `data/buildings/buff_id_to_stat.json`), and writes `data/research_catalog.json`. Add buff id to stat mappings to get combat-relevant research; with no mappings the script leaves the existing catalog unchanged.

## Upstream data-stfc-space mapping layer

- **Purpose:** JSON under `data/upstream/data-stfc-space/mapping/` documents upstream data.stfc.space artifacts: every root-level `summary-*.json`, `translations-*.json`, `ship_id_registry.json`, and the two markdown guides—**except** bulk per-id caches (`ships/*.json`, `hostiles/*.json`), which are described only by glob/pattern.
- **Canonical inventory:** `mapping/upstream_catalog.json` lists each of those files with a short semantic description, primary ids, related translation tables, and Kobayashi touchpoints where applicable.
- **Index:** `mapping/index.json` points at domain mapping files: `upstream_catalog`, `ships`, `hostiles`, `buildings`, `research`, `officers`, `translations`.
- **Per-domain mappings:** `ships.json`, `hostiles.json`, `buildings.json`, `research.json`, `officers.json`, `translations.json` add entity shapes, field notes, and normalization targets.
- **Validation:** `node scripts/validate_stfcspace_mapping.mjs` checks mapping files, that catalog paths exist on disk, that bulk globs have at least one file, and that **every** `summary-*.json` / `translations-*.json` at the upstream root appears in `upstream_catalog.json` (so new upstream drops fail CI until documented). When you add upstream files, extend `upstream_catalog.json` and run the validator.

## Forbidden tech: catalog and partial status

- **Catalog:** `data/forbidden_chaos_tech.json` (source: `data/import/forbidden_chaos_tech.csv`). Import with `cargo run --bin import_forbidden_chaos`. CSV columns: name, tech_type, tier, fid, stat, value, operator. `fid` is optional (game ID for matching synced FT).
- **Where the optimizer reads FT state:** Either synced `profiles/{profile_id}/forbidden_tech.imported.json` or the profile’s `forbidden_tech_override` (list of fids). Bonuses from the catalog (by `fid`) are merged into the player profile; add and mult operators are supported.
- **Partial / gaps:** Sync merge only matches catalog entries that have a `fid`; the game fid ↔ name mapping is not in-repo (see docs/ROADMAP.md § Forbidden tech). Level/tier from synced entries are not yet used (no per-level bonuses in catalog). See ROADMAP for combat-timing uncertainty (profile-only vs per-sub-round).
