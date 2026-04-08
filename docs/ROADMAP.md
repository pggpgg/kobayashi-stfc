# Roadmap

Planned features and priorities for Kobayashi.

## High priority — combat correctness

### `on_hull_breach` timing (**resolved** — matches engine)

**What was wrong:** `[TimingWindow::HullBreach](../src/combat/abilities.rs)` used to fire from a **hardcoded defender hull fraction** (below 50%), which was not the same mechanic as the timed **hull breached** state.

**Current behavior (code):** `on_hull_breach` runs when the defender **first enters** that state: after a successful [`AbilityEffect::HullBreach`](../src/combat/abilities.rs) proc extends `hull_breach_rounds_remaining` from **0** to a **positive** remaining duration (round start or attack phase). [`apply_hull_breach_timing_window`](../src/combat/engine.rs) applies timed effects, activations, and burning rolls for that transition. There is **no** hull-% threshold.

**Concepts (keep separate):** [`AbilityEffect::HullBreach`](../src/combat/abilities.rs) (proc + duration), [`TimingWindow::HullBreach`](../src/combat/abilities.rs) (timed abilities on state **entry**), and [`AbilityCondition::DefenderHullBreach`](../src/combat/abilities.rs) (true while the state is active).

**Test:** [`burning_triggers_on_hull_breach_state_entry`](../tests/combat_tests.rs).

**Remaining uncertainty:** Client may still differ on edge cases (e.g. firing again when breach duration is **refreshed** while already active, or extra triggers not modeled). Revisit with battlelogs / recorded fights when available.

---

## Combat buffs support

- **Data:** `[data/support_buffs.json](../data/support_buffs.json)` defines selectable ids (aligned with the workspace UI). Each entry may include `research_levels` (`rid` + `level`) merged in-memory via the same path as synced research, and optional `static_bonuses` (engine keys consumed by `apply_static_buffs_to_combatant` / mitigation). `exclusive_group` + `priority` resolve overlapping picks (e.g. Titan-A fort vs max).
- **API:** Optional `support_buffs: string[]` on simulate, optimize, compare crews, and replay-seed; capped length with unknown-id warnings in JSON responses.
- **Titan-A fort / max (alliance buff):** Static slice from in-game text: Fort → `crit_damage` 1.25 (+25% critical hit damage). Max → same plus `weapon_damage` 3.5 (+250% base weapon damage as 100% + 250% = 3.5× attack). Text also references bonuses scaling with the **recipient’s** Titan-A research; that portion is expected from the synced profile merge, not duplicated in `research_levels` here unless we later add explicit synthetic RIDs.
- **Cerritos / Defiant:** Tune `static_bonuses` / optional `research_levels` when authoritative values are available.
- **Mantis debuff:** Defender-side; not modeled here (see fidelity backlog).

---

## Officers (canonical, id registry, LCARS)

### Implemented (readability + sync)

- **Decimal game ids in JSON:** `data/officers/officers.canonical.json` uses plain integer strings for `source_officer_id` and `abilities[].ability_id` (no scientific notation). `data/officers/id_registry.json` keys are the same. Regenerate after upstream re-import with `python3 scripts/normalize_officer_id_strings.py` from the repo root.
- **Mod sync lookup:** `oid_to_map_key` in [`src/server/sync.rs`](../src/server/sync.rs) formats numeric `oid` values as decimal strings so roster ingress matches `id_registry.json` keys.

### Roadmap / backlog

- **Harry Kim below-decks Morale (LCARS + seating):** Upstream officer `1458469333` (`data/upstream/data-stfc-space/officers/1458469333.json`) places ability **`568169426`** on **`below_decks_ability`** (round-start Morale proc, `chance` 0.1 → 1.0 by rank; in-game card name from loca, e.g. “To the Journey!”). Canonical still marks that row **`slot`: `officer`**, so [`generate_lcars`](../src/bin/generate_lcars.rs) folds it into **`bridge_ability`** and LCARS ends with **`below_decks_ability: null`**. **Fix:** set canonical `slot` to **`below_decks`** (or derive seat layout from upstream per-ability blocks), run `cargo run --bin generate_lcars`, confirm `resolve_crew_to_buff_set` emits round-start `morale` from below decks and that `EnemyHostile` (and armada if applicable) matches scenario tests / traces.

---

## Hostile upstream `ship_type` (reverse engineering)

data.stfc.space hostile detail JSON (`hostiles/{id}.json`, same shape as normalized `data/hostiles/{id}.json`) includes `**ship_type` as a u32**: a **game category enum**, separate from `**hull_type`** (which the normalizer maps to `ship_class`: battleship / explorer / interceptor / survey). Labels for many values live only in client localization (e.g. `translations-navigation.json` key `armada_target_label` → “ARMADA TARGET” for value **1**).

### Implemented

- **Rust mapping table:** `[src/data/upstream_hostile_ship_type.rs](../src/data/upstream_hostile_ship_type.rs)` — `UpstreamHostileShipTypeProfile` + `upstream_hostile_ship_type_profile(u32)`; extend the `match` as new ids are confirmed.
- **Combat wiring:** `[HostileRecord::ship_type_for_combat](../src/data/hostile.rs)` chooses defender `[ShipType](../src/combat/types.rs)` for mitigation, player pierce-through vs that class, combat-begin ship-ability accuracy gates, and LCARS `defender_ship_type_is` when the hostile is the defender. **Mapped today:** `ship_type == 1` → treat as **armada target** (`ShipType::Armada`), so armada-gated officer/ship effects apply even when `ship_class` still looks like `survey` from hull mapping alone.

### Roadmap / backlog

- **Enumerate ids** — Cross-check `summary-hostile.json`, stfc.space UI, and in-game copy to document additional `ship_type` values and add `match` arms (and new `UpstreamHostileShipTypeProfile` fields if mechanics need more than `is_armada_target`).
- **Unknown values** — Optional validation or a small report: list normalized hostiles whose `upstream_ship_type` is not in the mapping (for triage).
- **Overlap with `EnemyTypes`** — If future categories do not fit `[EnemyType](../src/combat/types.rs)` / `ShipType` (e.g. multi-tag engagements), decide whether to thread `[EnemyTypes](../src/combat/types.rs)` through scenario vs growing the upstream profile struct.

---

## Sync (STFC Community Mod)

- **Persisted today:** officer, research, buildings, ships, **forbidden tech (`type: "ft"` / `type: "tech"`)**, buffs, and **battlelogs** (last 50 objects) — see [SYNC.md](SYNC.md). Research is written to `profiles/{id}/research.imported.json` and merged into the player profile when a research catalog is present. FT is written to `profiles/{id}/forbidden_tech.imported.json` and merged into the player profile (bonuses from `data/forbidden_chaos_tech.json`).
- **Battlelogs:** Mod **`battlelogs`** batches are persisted to `profiles/{id}/battlelogs.imported.json` (rolling **last 50** objects; see [SYNC.md](SYNC.md)). **Next:** wire stored logs into calibration, recorded-fight fixtures, replay, or analysis tooling (consumption path and schema interpretation still TBD).
- **Non-priority / deferred:** the mod also sends payload types that are accepted (200) but not stored. This is intentionally **not** a combat-accuracy priority right now, so it is tracked only as a “maybe later” note: traits, slots, resources, missions, inventory, jobs, and any additional raw tech-tree payloads if the mod exposes shapes beyond research project levels (already covered by `research` sync). **Note:** stfc-mod’s JSON `type: "tech"` is forbidden/chaos tech (same as `ft`) and is already persisted to `forbidden_tech.imported.json`.

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
- **Tooling:** `cargo run --bin building_combat_bonuses [--profile <id>]` prints combat bonuses (all at max, or profile’s synced levels with ops_level). Validation warns on unmapped `buff`_* stats.

### Optional / backlog (roadmap items)

- **Building id ↔ bid in index** — Add bid (or a small mapping file) to the building index for clarity and fallback resolution.
- **Conditions for station defense** — When station/starbase defense is in scope: populate `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) from import or mapping; support `BuildingMode::StationDefense` in the optimizer.
- **Strict validation report** — Report that lists all `buff_`* and unmapped conditions (e.g. strict mode or separate script).
- **Building summary API/UI** — Implemented: `GET /api/profile/buildings-summary` and Roster & Profile → Profile → “Buildings (sync → combat)”. Optional follow-up: editable building levels in the UI (today: sync or manual JSON / tooling such as `building_combat_bonuses`).

---

## Forbidden tech (implemented; ongoing maintenance)

Forbidden tech is implemented for ship combat; remaining items are maintenance/accuracy work (catalog upkeep, optional tier/level scaling).

### Implemented

- **Sync:** FT payloads (`type: "ft"`) are persisted to `profiles/{id}/forbidden_tech.imported.json` (by `fid`). See [SYNC.md](SYNC.md).
- **Catalog:** `data/forbidden_chaos_tech.json` (from `data/import/forbidden_chaos_tech.csv` via `cargo run --bin import_forbidden_chaos`). Optional `fid` column in CSV for sync match.
- **Merge:** `merge_forbidden_tech_bonuses_into_profile` matches synced entries by `fid`, applies bonuses; supports both additive and multiplicative (`operator`: add / mult).
- **Profile override:** `PlayerProfile.forbidden_tech_override` (optional list of fids). When set and non-empty, used instead of the synced file for the FT set. Enables “Use synced” / “None” / “Custom” in the UI.
- **Chaos tech:** Same catalog file and sync path as forbidden tech; `tech_type: chaos` in CSV/JSON. `PlayerProfile.chaos_tech_override` mirrors forbidden overrides. UI: separate “Chaos tech” controls on Roster & Profile → Profile.
- **API:** `GET /api/forbidden-tech` returns the catalog for the UI.
- **UI:** Roster & Profile → Profile tab → “Forbidden tech” dropdown (Use synced | None | Custom) and, for Custom, multi-select of catalog items that have a `fid`.

### Maintenance / gaps

- **Catalog `fid` (maintenance):** CI requires every committed catalog item to have a unique `fid` (`forbidden_chaos::sync_readiness_tests`) so stfc-mod sync can match bonuses by `fid`. Rows without a `fid` never apply for synced players. Workflow: upstream `summary-forbidden_tech.json` + `translations-forbidden_tech.json`, manual CSV `fid`, or `scripts/build_chaos_tech_csv_rows.mjs` (chaos rows from live `data.stfc.space/forbidden_tech/{id}.json`). See [data/README.md](../data/README.md) § Forbidden tech.
- **Level/tier:** `ForbiddenTechEntry` includes `level` and `tier`. The merge can optionally scale catalog bonuses by `tier`/`level` when `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` is set (linear scaling within a tier; conservative behavior when catalog tier disagrees with synced tier). `build_shared_scenario_data_standalone` and the registry path both use the same merge helper and env flag. The exact in-game scaling is still uncertain, so scaling remains opt-in until confirmed.
- **Combat timing:** DESIGN documents the intentional approximation: forbidden/chaos bonuses are applied at **profile merge**, not as a separate per-sub-round phase. A per-sub-round FT phase would require new evidence and engine work. See [DESIGN.md](DESIGN.md) §3.6 Notes.
- **Chaos data fidelity:** Bulk chaos rows are generated with heuristics (PvP-only / armada / proc lines approximated or skipped). Review `data/import/forbidden_chaos_tech.csv` when balancing matters; re-run `node scripts/build_chaos_tech_csv_rows.mjs` after adjusting the script.

---

## Research (implemented; ongoing maintenance)

Research sync + catalog merge are implemented for ship-combat stats; remaining items are catalog-mapping and scope-accuracy maintenance.

### Operational expectations (catalog required in CI)

- `data/research_catalog.json` is **tracked in git** and is expected to be **non-empty** in CI.
- `tests/scenario_research_integration_tests.rs` fails when `CI=true` if the catalog is missing/empty, with a remediation message.
- Local runs can still proceed without the catalog (the scenario test skips), unless `KOBAYASHI_REQUIRE_RESEARCH_CATALOG=1` is set.

### Implemented

- **Sync:** Research payloads (`type: "research"`, `rid`, `level`) are persisted to `profiles/{id}/research.imported.json`. See [SYNC.md](SYNC.md).
- **Catalog:** `data/research_catalog.json` (KOBAYASHI schema: `rid`, optional `name`, `levels[].bonuses` with engine stat keys). Loaded at startup into `DataRegistry.research_catalog`. Regenerated from data.stfc.space via `scripts/import_stfcspace_research.mjs` (description + name heuristics, `data/research/loca_id_to_stat.json`, `data/research/buff_id_to_stat.json`, and `data/buildings/buff_id_to_stat.json` where buff ids overlap).
- **Merge:** `merge_research_bonuses_into_profile` matches synced entries by `rid`, sums cumulative bonuses for levels 1..=level, and merges only combat stats (weapon_damage, hull_hp, shield_hp, isolytic_*, crit_*, pierce, shield_mitigation, armor, dodge, damage_reduction) into `profile.bonuses`. Merge order: forbidden tech → buildings → research.
- **Scenario wiring:** `build_shared_scenario_data_from_registry` loads `research.imported.json` and calls the merge when the catalog is present.
- **Import / cache:** Populate `data/upstream/data-stfc-space/research/{id}.json` (tracked in-repo) via `scripts/fetch_stfcspace_research.mjs` or an external bulk fetch; `import_stfcspace_research.mjs` reads **only** those local files for per-rid detail (no HTTP for `research/{id}.json`). It may still fetch `research/summary.json` when `--from-upstream` is not used.
- **Sync status:** `GET /api/sync/status` includes `research_catalog_loaded` and `research_catalog_item_count` (see `src/server/sync.rs`).
- **Integration test:** `tests/scenario_research_integration_tests.rs` builds a temp profile with `research.imported.json` and asserts merged `profile.bonuses` when the catalog is present.
- **Docs:** [data/README.md](../data/README.md) § Research, [DESIGN.md](DESIGN.md) §5.4, [SYNC.md](SYNC.md).

### Partially implemented / gaps (roadmap items)

- **Accuracy** — **Done:** `accuracy` merges into `profile.bonuses` and scales ship `AttackerStats.accuracy` for mitigation/pierce-through; catalog values are treated as fractional bonuses (×(1 + sum)), same convention as `weapon_damage`. Remaining risk: in-game wording/scopes may differ, so validate with additional recorded-fight fixtures if mismatches appear.
- **Other combat stats** — Any future stat keys must be added to `normalize_profile_combat_stat` and wired in `apply_profile_to_attacker` / `apply_static_buffs_to_combatant` (or the mitigation path) before research mappings affect simulation.
- **Apex (shred / barrier)** — **Done:** `apex_shred` and `apex_barrier` are normalized combat keys; research/building merges feed `profile.bonuses`, and `apply_profile_to_attacker` adds them to the player ship combatant (shred on outbound apex math; barrier on counter-attack defense). Import still depends on `import_stfcspace_research.mjs` mapping upstream buffs to those stat names in `research_catalog.json`.
- **Conditional bonuses** — Armada-, class-, PvP-, or faction-scoped lines may be mapped as **global** ship bonuses when descriptions look generic; tightening requires engine/scenario context or buff-level overrides in `data/research/buff_id_to_stat.json`.
- **Catalog refresh** — After upstream drops, re-run `fetch_stfcspace_research.mjs` then `import_stfcspace_research.mjs`; use `--dump-unmapped` to extend `data/research/buff_id_to_stat.json` / `loca_id_to_stat.json` for buff ids that still do not resolve.

---

## Maverick faction

- **Maverick faction track (content)** — Track and incrementally add Maverick support as the game’s content stabilizes: combat-relevant research, hostiles, and buildings/sync where applicable; keep in parallel with ship-ability coverage work. Source-of-truth checklist: [MAVERICK.md](MAVERICK.md).
- **Low priority (backlog)** — Deferred items: Maverick faction research (catalog + mappings), Maverick favors (faction-store / favor bonuses), new Maverick artifacts (exocomp/artifact bonuses). Detailed checklist: [MAVERICK.md](MAVERICK.md) § Low priority (backlog).

**Tracking doc:** [MAVERICK.md](MAVERICK.md) — scope, data pipeline, Warp Dive Bar (`building_88`), uncertainty (no placeholder hostile stats in-repo).