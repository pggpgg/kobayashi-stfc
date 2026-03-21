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
- **Buff → stat mapping (import):** `node scripts/import_stfcspace_research.mjs` resolves each research buff in this order: inline `RESEARCH_BUFF_MAPPING` in the script, `data/research/buff_id_to_stat.json` (buff id → stat; may be `{}` until you add research-only ids), `data/buildings/buff_id_to_stat.json`, then `data/research/loca_id_to_stat.json` using the API’s `buff.loca_id` (aligned with `translations-research.json` ids). The loca map intentionally omits station, economy, and faction-conditional nodes so we do not emit bogus **global** ship combat bonuses. Composite descriptions (e.g. “Shield Deflection, Armor and Dodge”) map to a **single** representative stat—under-modeled; prefer tightening with buff-level overrides when evidence exists.
- **Flat vs percentage values:** Nodes with `value_is_percentage: false` and large numeric values (e.g. flat hull/shield points from the API) are not converted to profile multipliers and are skipped by the importer until a defined conversion exists.
- **Refresh from upstream:** Run `node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456]`. The script reads `data/upstream/data-stfc-space/summary-research.json`, fetches per-research detail from data.stfc.space for each node, and writes `data/research_catalog.json`. With no resolvable mappings the script leaves the existing catalog unchanged.
- **Checked-in combat subset:** Regenerate with e.g. `--from-upstream --rid 2232304457,1914017763,2207112158,2488759501,3666105341,3016411865,3814450797,1240350440,1016099357,3322225012,3146647716,995454242,1852208276` (13 items: prior 9 plus **global** isolytic Starships nodes mapped via `loca_id` 55313/55314/57002/57003). Many other isolytic lines in `translations-research.json` are PvP-, class-, or armada-conditional; they stay unmapped so the optimizer does not treat them as unconditional ship bonuses.

## Upstream data-stfc-space mapping layer

- **Purpose:** JSON under `data/upstream/data-stfc-space/mapping/` documents upstream data.stfc.space artifacts: every root-level `summary-*.json`, `translations-*.json`, `ship_id_registry.json`, and the two markdown guides—**except** bulk per-id caches (`ships/*.json`, `hostiles/*.json`), which are described only by glob/pattern.
- **Canonical inventory:** `mapping/upstream_catalog.json` lists each of those files with a short semantic description, primary ids, related translation tables, and Kobayashi touchpoints where applicable.
- **Index:** `mapping/index.json` points at domain mapping files: `upstream_catalog`, `ships`, `hostiles`, `buildings`, `research`, `officers`, `translations`.
- **Per-domain mappings:** `ships.json`, `hostiles.json`, `buildings.json`, `research.json`, `officers.json`, `translations.json` add entity shapes, field notes, and normalization targets.
- **Validation:** `node scripts/validate_stfcspace_mapping.mjs` checks mapping files, that catalog paths exist on disk, that bulk globs have at least one file, and that **every** `summary-*.json` / `translations-*.json` at the upstream root appears in `upstream_catalog.json` (so new upstream drops fail CI until documented). When you add upstream files, extend `upstream_catalog.json` and run the validator.

## Forbidden tech: catalog and partial status

- **Catalog:** `data/forbidden_chaos_tech.json` (source: `data/import/forbidden_chaos_tech.csv`). Import with `cargo run --bin import_forbidden_chaos`. CSV columns: name, tech_type, tier, fid, stat, value, operator. `fid` is optional (game ID for matching synced FT; importer can fill from upstream translations when the name matches).
- **Where the optimizer reads FT state:** Either synced `profiles/{profile_id}/forbidden_tech.imported.json` or the profile’s `forbidden_tech_override` (list of fids). Bonuses from the catalog (by `fid`) are merged into the player profile; add and mult operators are supported.
- **Level/tier scaling:** Opt-in via env `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` (linear model; see `forbidden_tech_level_tier_scaling_enabled_from_env` in `src/data/profile.rs`).
- **Partial / gaps:** Prefer calibrating S31 Torpedo Pods (and similar) from [`upstream/data-stfc-space/forbidden_tech/{fid}.json`](upstream/data-stfc-space/forbidden_tech/473132032.json) when available: use `values[level-1].value` as a percentage → catalog decimal. Some upstream copy is class-specific (e.g. Battleship) while the simulator applies generic profile keys. See ROADMAP for combat-timing uncertainty (profile-only vs per-sub-round).
