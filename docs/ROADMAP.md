# Roadmap

Planned features and priorities for Kobayashi.

## Codex Speed Demon

Speeding up crew discovery is primarily a search-efficiency problem, not a raw simulator-throughput problem. The simulator is already fast; the roadmap here is about spending Monte Carlo budget on the most promising crews first and learning from prior runs.

### Operating principle

The intended direction is: **seed + prune + scout + confirm + learn**, rather than trying to brute-force ever-larger search spaces with uniform simulation effort.

- **Optimize / heuristics defaults:** Strict below-decks heuristic seeds (combat-relevant below-decks-slot modifiers only) and narrow below-decks officer pools are the default. The SPA opt-out is **Allow below-decks officers without combat abilities (prioritize highest power)** → API `allow_below_decks_without_combat_ability`. The removed field `prioritize_below_decks_ability` is rejected with a validation error (use `allow_below_decks_without_combat_ability` instead).

## Combat buffs support

- **Defender-side buffs & debuffs:** **Partial** — Catalog field `static_bonus_target: defender_if_player_opponent` (`[data/support_buffs.json](../data/support_buffs.json)`) routes **direct** `static_bonuses` onto the defender `[Combatant](src/combat/types.rs)` when the API uses `defender_opponent: player` (see `[aggregate_support_static_bonuses_split](../src/data/support_buffs.rs)`, `[SharedScenarioData::support_defender_static_buffs](../src/optimizer/monte_carlo/scenario.rs)`). Titan-A Fortify / Max Fortify, Defiant Reinforce, and placeholder `mantis_sting` use this path; Cerritos remains attacker-routed. **Still open:** support-gated **research** augmentation from `augment_static_buffs_with_support_gated_research` stays on the attacker merge; hostile-applied modifiers and full Mantis combat stats remain TBD.

---

## Officers (canonical, id registry, LCARS)

### Encounter spreadsheet labels (community curation) vs Kobayashi


| Label (spreadsheet)                             | In Kobayashi today                                                                                                                                                  |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| PvP Space                                       | `defender_opponent: player` on API (PvP-shaped toggle).                                                                                                             |
| PvP Station                                     | Same player toggle; station-specific scope not modeled in default ship combat.                                                                                      |
| Red Moving Space, Waves, QTrial, Mission Bosses | No per-hostile signal in `[HostileRecord](../src/data/hostile.rs)`; treat as **unsupported** unless you add manual scenario tags or battlelog-driven context later. |
| Group Armadas, Solo Armadas                     | Partially: `ShipType::Armada` + hostile stats; group vs solo not in normalized JSON.                                                                                |
| Invading Entities, Assaults                     | Unsupported without encounter ids in data.                                                                                                                          |
| Outpost Armadas, Outpost Retaliation Attackers  | `[HostileRecord::is_outpost](../src/data/hostile.rs)` exists; no LCARS condition wired yet—confirm in-game vs logs before adding `defender_is_outpost`-style gates. |


---

## Unified CombatEffectSpec (cross-source normalization)

Current state: officer effects are LCARS-native, while research uses stat rows plus targeted Rust routing for conditional attack-phase behavior. This split increases drift risk and special-case maintenance.

### Decision direction

- **Standardize to a single canonical effect IR** (`CombatEffectSpec`) compiled into existing engine types (`AbilityEffect`, `AbilityCondition`, `TimingWindow`).
- **Keep LCARS as officer authoring DSL**; do not remove LCARS files.
- **Use stfc.cc terminology as ingestion aliases**, not as the engine runtime contract.
  - Concretely: fields such as `AbilityModifier`, `AbilityConditions`, `AbilityTrigger`, `AbilityTarget`, `AbilityOperation`, `AbilityAttributes`, `AbilityChances`, and `AbilityValues` should map into canonical IR fields.

### Draft schema

- Spec draft and implementation notes: [COMBAT_EFFECT_SPEC.md](COMBAT_EFFECT_SPEC.md).

### Phase 1 non-goals (while the spec flag is rolling out)

- No changes to combat **timing windows**, **round structure**, or **core damage formulas** solely for the spec migration.
- No removal of the LCARS resolver until **LCARS ↔ spec parity** is proven for representative fixtures.

### Cutover status (completed)

- `**resolve_lcars_condition` deduplicated** onto the canonical `lcars_condition_to_spec` + `compile_condition` path. The legacy function is deprecated; remaining call sites (parity tests, resolver unit tests) are `#[allow(deprecated)]` regression coverage.
- **Effect-type coverage audited** — all six types in `officers.lcars.yaml` (`assimilated`, `burning`, `hull_breach`, `morale`, `stat_modify`, `tag`) are covered by the adapter.
- **Full parity confirmed** — 10/10 parity tests pass across all three spec parity suites.

### Open roadmap / backlog

- **Future effect types:** Extend the IR/compiler when new `effect_type` values appear in `captain_ability` data (the allow-list test in `tests/lcars_captain_spec_parity_tests.rs` gates this).
- **Remove `resolve_lcars_condition`** after parity confidence and full caller migration.

---

## Hostile upstream `ship_type` (reverse engineering)

data.stfc.space hostile detail JSON (`hostiles/{id}.json`, same shape as normalized `data/hostiles/{id}.json`) includes `**ship_type` as a u32**: a **game category enum**, separate from `**hull_type`** (which the normalizer maps to `ship_class`: battleship / explorer / interceptor / survey). Labels for many values live only in client localization (e.g. `translations-navigation.json` key `armada_target_label` → “ARMADA TARGET” for value **1**).

### Roadmap / backlog

- **Enumerate ids** — Maintainer reference: [UPSTREAM_HOSTILE_SHIP_TYPES.md](UPSTREAM_HOSTILE_SHIP_TYPES.md). Cross-check `summary-hostile.json`, stfc.space UI, and in-game copy when adding `match` arms (and new `UpstreamHostileShipTypeProfile` fields if mechanics need more than `is_armada_target`).
- **Overlap with `EnemyTypes`** — If future categories do not fit `[EnemyType](../src/combat/types.rs)` / `ShipType` (e.g. multi-tag engagements), decide whether to thread `[EnemyTypes](../src/combat/types.rs)` through scenario vs growing the upstream profile struct.

---

## Sync (STFC Community Mod)

- **Persisted today:** officer, research, buildings, ships, **forbidden tech (`type: "ft"` / `type: "tech"`)**, buffs, and **battlelogs** (last 50 objects) — see [SYNC.md](SYNC.md). Research is written to `profiles/{id}/research.imported.json` and merged into the player profile when a research catalog is present. FT is written to `profiles/{id}/forbidden_tech.imported.json` and merged into the player profile (bonuses from `data/forbidden_chaos_tech.json`).
- **Battlelogs:** Mod `**battlelogs`** batches are persisted to `profiles/{id}/battlelogs.imported.json` (rolling **last 50** objects; see [SYNC.md](SYNC.md)). **Next:** wire stored logs into calibration, recorded-fight fixtures, replay, or analysis tooling (consumption path and schema interpretation still TBD).
- **Non-priority / deferred:** the mod also sends payload types that are accepted (200) but not stored. This is intentionally **not** a combat-accuracy priority right now, so it is tracked only as a “maybe later” note: traits, slots, resources, missions, inventory, jobs, and any additional raw tech-tree payloads if the mod exposes shapes beyond research project levels (already covered by `research` sync). **Note:** stfc-mod’s JSON `type: "tech"` is forbidden/chaos tech (same as `ft`) and is already persisted to `forbidden_tech.imported.json`.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.

---

## Buildings (ship combat)

Buildings are **fully modeled for ship combat** per the “buildings full modeling” plan: catalog, level/ops data, buff normalization, sync path, ops context, profile merge, and tooling are in place. Optional and backlog items remain on the roadmap.

### Optional / backlog (roadmap items)

- **Building id ↔ bid in index** — Add bid (or a small mapping file) to the building index for clarity and fallback resolution.
- **Conditions for station defense** — When station/starbase defense is in scope: populate `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) from import or mapping; support `BuildingMode::StationDefense` in the optimizer.

---

## Forbidden tech (open maintenance)

Forbidden tech ship-combat support is in place; the open roadmap here is maintenance and fidelity work (catalog upkeep, optional tier/level scaling).

### Maintenance / gaps

- **Catalog `fid` (maintenance):** CI requires every committed catalog item to have a unique `fid` (`forbidden_chaos::sync_readiness_tests`) so stfc-mod sync can match bonuses by `fid`. Rows without a `fid` never apply for synced players. Workflow: upstream `summary-forbidden_tech.json` + `translations-forbidden_tech.json`, manual CSV `fid`, or `scripts/build_chaos_tech_csv_rows.mjs` (chaos rows from live `data.stfc.space/forbidden_tech/{id}.json`). See [data/README.md](../data/README.md) § Forbidden tech.
- **Level/tier:** `ForbiddenTechEntry` includes `level` and `tier`. The merge can optionally scale catalog bonuses by `tier`/`level` when `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` is set (linear scaling within a tier; conservative behavior when catalog tier disagrees with synced tier). `build_shared_scenario_data_standalone` and the registry path both use the same merge helper and env flag. The exact in-game scaling is still uncertain, so scaling remains opt-in until confirmed.
- **Combat timing:** DESIGN documents the intentional approximation: forbidden/chaos bonuses are applied at **profile merge**, not as a separate per-sub-round phase. A per-sub-round FT phase would require new evidence and engine work. See [DESIGN.md](DESIGN.md) §3.6 Notes.
- **Chaos data fidelity:** Bulk chaos rows are generated with heuristics (PvP-only / armada / proc lines approximated or skipped). Review `data/import/forbidden_chaos_tech.csv` when balancing matters; re-run `node scripts/build_chaos_tech_csv_rows.mjs` after adjusting the script.

---

## Research (open maintenance)

Research sync + catalog merge are in place for ship-combat stats; remaining roadmap items focus on catalog mapping and scope accuracy.

### Operational expectations (catalog required in CI)

- `data/research_catalog.json` is **tracked in git** and is expected to be **non-empty** in CI.
- `tests/scenario_research_integration_tests.rs` fails when `CI=true` if the catalog is missing/empty, with a remediation message.
- Local runs can still proceed without the catalog (the scenario test skips), unless `KOBAYASHI_REQUIRE_RESEARCH_CATALOG=1` is set.

### Open roadmap items

- **Other combat stats** — Any future stat keys must be added to `normalize_profile_combat_stat` and wired in `apply_profile_to_attacker` / `apply_static_buffs_to_combatant` (or the mitigation path) before research mappings affect simulation.
- **Conditional bonuses** — Armada-, class-, PvP-, or faction-scoped lines may be mapped as **global** ship bonuses when descriptions look generic; tightening requires engine/scenario context or buff-level overrides in `data/research/buff_id_to_stat.json`.
- **Catalog refresh** — After upstream drops, re-run `fetch_stfcspace_research.mjs` then `import_stfcspace_research.mjs`; use `--dump-unmapped` to extend `data/research/buff_id_to_stat.json` / `loca_id_to_stat.json` for buff ids that still do not resolve.

