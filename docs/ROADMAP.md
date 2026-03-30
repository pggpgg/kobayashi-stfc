# Roadmap

Planned features and priorities for Kobayashi.

## Backlog / hygiene

- **i18n scaffolding** — String extraction / message catalog if non-English UI is planned.

## Next pillar (chosen)

After research sync/catalog merge work, the **next major combat-engine focus** is **ship abilities** (data.stfc.space `ability` on ships, distinct from officers). Maverick faction support remains a parallel content track; see [MAVERICK.md](MAVERICK.md).

---

## Ship Abilities

- **Ship ability implementation** — Implement ship abilities from the data.stfc.space `ability` array (e.g. "when hit, increase armor piercing / shield piercing / accuracy"). These are distinct from officer abilities and affect combat when the ship takes damage or performs actions. Requires extending the combat engine to evaluate ship-specific effects per round.

### Future task: audit `combat_noop`

Many ability ids still map to `effect_type: combat_noop` in [data/upstream/data-stfc-space/ship_ability_catalog.json](../data/upstream/data-stfc-space/ship_ability_catalog.json). **Planned audit:**

1. **Inventory** — List every noop id with its translated description (`translations-ship_buffs.json`, key `ship_ability_desc`, per-row `loca_id` when present).
2. **Bucket** — For each row, label why it is noop today: economy / progression, accuracy or other missing engine stat, opponent class or faction / Delta Quadrant tags, armada or defending scope, hostile-only debuffs, shield-depletion or other state gates not modeled, multi-step proc chains (e.g. hull breach + crit + cumulative), or intentional global approximation rejected (too uncertain).
3. **Decide** — For each bucket: keep noop, add a **documented** mapping in [scripts/generate_full_ship_ability_catalog.py](../scripts/generate_full_ship_ability_catalog.py) (regenerate JSON), extend [src/data/ship_ability_resolve.rs](../src/data/ship_ability_resolve.rs) / combat engine, or add scenario/hostile context so conditional text can be honored.
4. **Drift** — After `python3 scripts/generate_full_ship_ability_catalog.py`, diff the catalog against any **hand-tuned** entries; fold durable overrides into the script so regeneration does not erase them.

Approximations already used for modeled rows are summarized in [DESIGN.md](DESIGN.md) §3.6 (ship hull abilities — catalog approximations).

---

## Combat buffs support

- **Cerritos buff**
- **Titan-A buffs**
- **Defiant bugg**
- **Mantis debuff**

---

## Sync (STFC Community Mod)

- **Persisted today:** officer, research, buildings, ships, and **forbidden tech (`type: "ft"`)** — see [SYNC.md](SYNC.md). Research is written to `profiles/{id}/research.imported.json` and merged into the player profile when a research catalog is present. FT is written to `profiles/{id}/forbidden_tech.imported.json` and merged into the player profile (bonuses from `data/forbidden_chaos_tech.json`).

- **Optional next sync work** — the mod also sends payload types that are accepted (200) but not stored. **Note:** stfc-mod’s JSON `type: "tech"` is **forbidden/chaos tech** (same as `ft`); Kobayashi persists it to `forbidden_tech.imported.json`. Remaining candidates for future persistence: traits, slots, buffs, resources, missions, battlelogs, inventory, jobs, and any **additional** raw tech-tree payloads if the mod exposes shapes beyond research project levels (already covered by `research` sync).

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
- **Building summary API/UI** — Implemented: `GET /api/profile/buildings-summary` and Roster & Profile → Profile → “Buildings (sync → combat)”. Optional follow-up: editable building levels in the UI (today: sync or manual JSON / tooling such as `building_combat_bonuses`).

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
- **Level/tier:** `ForbiddenTechEntry` includes `level` and `tier`. The merge can optionally scale catalog bonuses by `tier`/`level` when `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` is set (linear scaling within a tier; conservative behavior when catalog tier disagrees with synced tier). The exact in-game scaling is still uncertain, so scaling remains opt-in until confirmed.
- **Combat timing:** DESIGN and [COMBAT_FEATURES_FROM_STFC_TOOLBOX.md](COMBAT_FEATURES_FROM_STFC_TOOLBOX.md) describe “forbidden tech + chaos tech buffs” as applying **per sub-round**. Current code applies FT only at **profile merge** (pre-combat). A per-sub-round FT phase would be a separate engine change; left as future unless we have evidence the game does it that way.

---

## Research (partial)

Research **sync and catalog merge** are implemented for ship-combat stats. Gaps are mainly **stats the engine does not yet fold into the player profile** (e.g. accuracy) and **conditional in-game scopes** that the catalog still treats as unconditional bonuses.

### Implemented

- **Sync:** Research payloads (`type: "research"`, `rid`, `level`) are persisted to `profiles/{id}/research.imported.json`. See [SYNC.md](SYNC.md).
- **Catalog:** `data/research_catalog.json` (KOBAYASHI schema: `rid`, optional `name`, `levels[].bonuses` with engine stat keys). Loaded at startup into `DataRegistry.research_catalog`. Regenerated from data.stfc.space via `scripts/import_stfcspace_research.mjs` (description + name heuristics, `data/research/loca_id_to_stat.json`, `data/research/buff_id_to_stat.json`, and `data/buildings/buff_id_to_stat.json` where buff ids overlap).
- **Merge:** `merge_research_bonuses_into_profile` matches synced entries by `rid`, sums cumulative bonuses for levels 1..=level, and merges only combat stats (weapon_damage, hull_hp, shield_hp, isolytic_*, crit_*, pierce, shield_mitigation, armor, dodge, damage_reduction) into `profile.bonuses`. Merge order: forbidden tech → buildings → research.
- **Scenario wiring:** `build_shared_scenario_data_from_registry` loads `research.imported.json` and calls the merge when the catalog is present.
- **Import / cache:** Populate `data/upstream/data-stfc-space/research/{id}.json` (gitignored) via `scripts/fetch_stfcspace_research.mjs` or an external bulk fetch; `import_stfcspace_research.mjs` reads **only** those local files for per-rid detail (no HTTP for `research/{id}.json`). It may still fetch `research/summary.json` when `--from-upstream` is not used.
- **Sync status:** `GET /api/sync/status` includes `research_catalog_loaded` and `research_catalog_item_count` (see `src/server/sync.rs`).
- **Integration test:** `tests/scenario_research_integration_tests.rs` builds a temp profile with `research.imported.json` and asserts merged `profile.bonuses` when the catalog is present.
- **Docs:** [data/README.md](../data/README.md) § Research, [DESIGN.md](DESIGN.md) §5.4, [SYNC.md](SYNC.md).

### Partially implemented / gaps (roadmap items)

- **Accuracy** — `accuracy` is merged into `profile.bonuses` and scales ship `AttackerStats.accuracy` when computing hostile mitigation / pierce-through (`scenario.rs`). Catalog values are treated as fractional bonuses (×(1 + sum)), same convention as `weapon_damage`; in-game wording may differ—verify with logs/toolbox if fights look off.
- **Other combat stats** — Stats not in `normalize_profile_combat_stat` still need end-to-end wiring before research mappings affect simulation.

- **Apex (shred / barrier)** — Not merged from research into the player profile; add keys and merge rules if research-only apex must affect the scenario.

- **Conditional bonuses** — Armada-, class-, PvP-, or faction-scoped lines may be mapped as **global** ship bonuses when descriptions look generic; tightening requires engine/scenario context or buff-level overrides in `data/research/buff_id_to_stat.json`.

- **Catalog refresh** — After upstream drops, re-run `fetch_stfcspace_research.mjs` then `import_stfcspace_research.mjs`; use `--dump-unmapped` to extend `data/research/buff_id_to_stat.json` / `loca_id_to_stat.json` for buff ids that still do not resolve.

---

## Maverick faction

- **Maverick faction support** — Add support for the Maverick faction (Ops 55+, unlocked via Warp Dive Bar): combat-relevant research, hostiles (e.g. Conqueror Borg Solo Armadas), buildings/sync where applicable. See [Update 88 First Look: The Maverick Faction](https://startrekfleetcommand.com/news/update-88-first-look-the-maverick-faction/).

- **Low priority (backlog)** — Deferred items: **Maverick faction research** (catalog + mappings), **Maverick favors** (faction-store / favor bonuses — not modeled yet), **new Maverick artifacts** (exocomp/artifact bonuses — not modeled yet). Detailed checklist: [MAVERICK.md](MAVERICK.md) § Low priority (backlog).

**Tracking doc:** [MAVERICK.md](MAVERICK.md) — scope, data pipeline, Warp Dive Bar (`building_88`), uncertainty (no placeholder hostile stats in-repo).
