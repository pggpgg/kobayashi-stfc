# Roadmap

Planned features and priorities for Kobayashi.

---

## Officers

- **Addition of FCM Data officer to the simulator** — Add the FCM Data officer to the LCARS officer database and ensure its abilities are correctly modeled in combat simulation.

---

## Ship Abilities

- **Ship ability implementation** — Implement ship abilities from the data.stfc.space `ability` array (e.g. "when hit, increase armor piercing / shield piercing / accuracy"). These are distinct from officer abilities and affect combat when the ship takes damage or performs actions. Requires extending the combat engine to evaluate ship-specific effects per round.

---

## Sync (STFC Community Mod)

- **Persisted today:** officer, research, buildings, ships, and **forbidden tech (`type: "ft"`)** — see [SYNC.md](SYNC.md). Research is written to `profiles/{id}/research.imported.json` and merged into the player profile when a research catalog is present. FT is written to `profiles/{id}/forbidden_tech.imported.json` and merged into the player profile (bonuses from `data/forbidden_chaos_tech.json`).

- **Optional next sync work** — the mod also sends payload types that are accepted (200) but not stored. Candidates for future persistence (as product needs and data shapes are clarified): **tech** (e.g. tech tree / research state from the game), traits, slots, buffs, resources, missions, battlelogs, inventory, jobs. Reception and persistence of **tech** is a priority candidate so optimizer and profile can use full tech state if the mod exposes it.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.

---

## Buildings (ship combat)

Buildings are **fully modeled for ship combat** per the “buildings full modeling” plan: catalog, level/ops data, buff normalization, sync path, ops context, profile merge, and tooling are in place. Optional and backlog items remain on the roadmap.

### Implemented

- **Catalog:** `building_bid_resolver` resolves game `bid` → Kobayashi `id` via translations and index; index entries `building_{bid}` are included so new buildings from sync resolve when the building file exists. See [data/README.md](../data/README.md) § Buildings.
- **Level data:** Import script sets `ops_min` from stfc.space `unlock_level` when available; `level_matches_context` filters by ops in the engine.
- **Buff normalization:** `data/buildings/buff_id_to_stat.json` is merged into common combat buff normalization at import time; combat-relevant bonuses are emitted with engine stat names. See [BUFF_ID_TO_STAT_NAME.md](../data/upstream/data-stfc-space/BUFF_ID_TO_STAT_NAME.md).
- **Sync:** Buildings payloads are written to `profiles/{id}/buildings.imported.json`; default profile path is used consistently. See [SYNC.md](SYNC.md).
- **Ops & context:** Ops level is inferred from Operations Center (bid 0 → `ops_center`); `PlayerProfile.ops_level` override is supported. Scenario uses `BuildingBonusContext { ops_level, mode: ShipCombat }`.
- **Profile/combat:** Building bonuses (normalized stats) are merged in `merge_building_bonuses_into_profile` and applied in combat via profile.
- **Tooling:** `cargo run --bin building_combat_bonuses [--profile <id>]` prints combat bonuses (all at max, or profile’s synced levels with ops_level). Validation warns on unmapped `buff_*` stats.

### Optional / backlog (roadmap items)

- **Building id ↔ bid in index** — Add bid (or a small mapping file) to the building index for clarity and fallback resolution.
- **Conditions for station defense** — When station/starbase defense is in scope: populate `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) from import or mapping; support `BuildingMode::StationDefense` in the optimizer.
- **Strict validation report** — Report that lists all `buff_*` and unmapped conditions (e.g. strict mode or separate script).
- **Building summary API/UI** — Endpoint or UI to show current profile’s building levels and effective combat bonuses from buildings; allow setting building levels or ops_level in profile for simulation without sync.

---

## Forbidden tech (partial)

Forbidden tech is **partially implemented**. The following is in place; remaining gaps and uncertainty are documented so we don’t lose track of what’s missing.

### Implemented

- **Sync:** FT payloads (`type: "ft"`) are persisted to `profiles/{id}/forbidden_tech.imported.json` (by `fid`). See [SYNC.md](SYNC.md).
- **Catalog:** `data/forbidden_chaos_tech.json` (from `data/import/forbidden_chaos_tech.csv` via `cargo run --bin import_forbidden_chaos`). Optional `fid` column in CSV for sync match.
- **Merge:** `merge_forbidden_tech_bonuses_into_profile` matches synced entries by `fid`, applies bonuses; supports both additive and multiplicative (`operator`: add / mult).
- **Profile override:** `PlayerProfile.forbidden_tech_override` (optional list of fids). When set and non-empty, used instead of the synced file for the FT set. Enables “Use synced” / “None” / “Custom” in the UI.
- **API:** `GET /api/forbidden-tech` returns the catalog for the UI.
- **UI:** Roster & Profile → Profile tab → “Forbidden tech” dropdown (Use synced | None | Custom) and, for Custom, multi-select of catalog items that have a `fid`.

### Partially implemented / gaps

- **Catalog `fid`:** Sync-based merge only matches catalog entries that have a `fid`. The mapping from game `fid` (e.g. 919296) to catalog names is not in-repo; it requires a community/game source (e.g. data.stfc.space or stfc-mod) or manual mapping. Until catalog items have the correct `fid`, synced FT may not apply.
- **Level/tier:** `ForbiddenTechEntry` has `level` and `tier`, but the merge does **not** use them: it applies the catalog record’s bonuses in full. Whether and how in-game FT scales by level/tier is unspecified; confirm before adding per-level bonuses to the catalog (e.g. research-style levels).
- **Combat timing:** DESIGN and [COMBAT_FEATURES_FROM_STFC_TOOLBOX.md](COMBAT_FEATURES_FROM_STFC_TOOLBOX.md) describe “forbidden tech + chaos tech buffs” as applying **per sub-round**. Current code applies FT only at **profile merge** (pre-combat). A per-sub-round FT phase would be a separate engine change; left as future unless we have evidence the game does it that way.

---

## Research (partial)

Research is **partially implemented**. The following is in place; remaining gaps are documented so we don't lose track of what's missing.

### Implemented

- **Sync:** Research payloads (`type: "research"`, `rid`, `level`) are persisted to `profiles/{id}/research.imported.json`. See [SYNC.md](SYNC.md).
- **Catalog:** `data/research_catalog.json` (KOBAYASHI schema: `rid`, `levels[].bonuses` with engine stat keys). Loaded at startup into `DataRegistry.research_catalog`. Currently contains a stub entry (rid 1) for testing; real combat research requires buff id → stat mappings.
- **Merge:** `merge_research_bonuses_into_profile` matches synced entries by `rid`, sums cumulative bonuses for levels 1..=level, and merges only combat stats (weapon_damage, hull_hp, shield_hp, etc.) into `profile.bonuses`. Merge order: forbidden tech → buildings → research.
- **Scenario wiring:** `build_shared_scenario_data_from_registry` loads `research.imported.json` and calls the merge when the catalog is present.
- **Import script:** `node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid ...]` reads summary-research.json, fetches per-research detail from data.stfc.space, and can write the catalog. It does **not** overwrite the catalog when no buff mappings exist (leaves stub in place).
- **Docs:** [data/README.md](../data/README.md) § Research, [DESIGN.md](DESIGN.md) §5.4, [SYNC.md](SYNC.md).

### Partially implemented / gaps (roadmap items)

- **Research buff id → stat mapping** — The import script has empty `RESEARCH_BUFF_MAPPING` and `data/buildings/buff_id_to_stat.json` has no research-specific entries. Data.stfc.space research nodes use buff `id` and `loca_id`; we need a mapping from those to engine stats (weapon_damage, hull_hp, shield_hp, etc.) so the import can emit combat bonuses. Options: derive from `data/upstream/data-stfc-space/translations-research.json` (or similar), reuse building buff mappings where the same buff id appears in research, or add a dedicated `data/research/buff_id_to_stat.json`. Until mappings exist, the catalog stays stub-only and only the test rid 1 affects combat.

- **Populate research catalog from upstream** — Once buff mappings exist, run the import script (with a subset of combat-relevant rids or `--limit`) to fill `data/research_catalog.json` with real research nodes. Prefer a subset of high-impact combat research first (e.g. weapon damage, hull/shield, mitigation) to validate the pipeline before scaling.

- **Integration test: scenario + research** — Add a test that builds `SharedScenarioData` for a profile that has `research.imported.json` with at least one rid present in the catalog, and asserts that `profile.bonuses` contains the expected combat stat(s). Requires a test profile dir (e.g. temp or fixture) with `research.imported.json` and a registry that has the catalog loaded; document if the test is skipped when run without data dir.

- **Optional: research catalog version in sync/status** — Expose in `GET /api/sync/status` (or data version API) that research is applied for combat when the catalog is present, e.g. `research_catalog_loaded: true` or `research_catalog_item_count: N`, so the UI or tools can show that research bonuses are active.

---

## Maverick faction

- **Maverick faction support** — Add support for the Maverick faction (Ops 55+, unlocked via Warp Dive Bar): combat-relevant research (e.g. Maverick Research Tree nodes such as Isolytic Defense / Apex Shred / Critical Damage Reduction vs. Conqueror Borg Solo Armadas), any sync or catalog data for Maverick bonuses, and related hostiles (Conqueror Borg Solo Armadas, etc.) as needed for the simulator and optimizer. See [Update 88 First Look: The Maverick Faction](https://startrekfleetcommand.com/news/update-88-first-look-the-maverick-faction/).
