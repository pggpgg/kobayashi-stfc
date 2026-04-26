# Roadmap

Planned features and priorities for Kobayashi.

## Codex Speed Demon

Speeding up crew discovery is primarily a search-efficiency problem, not a raw simulator-throughput problem. The simulator is already fast; the roadmap here is about spending Monte Carlo budget on the most promising crews first and learning from prior runs.

### Near-term priorities

- **Default broad searches to tiered optimization** — *(Shipped: workspace defaults to tiered; when `strategy` is omitted the server picks tiered vs exhaustive from **effective** candidate count — same pipeline as optimize: generation + `warm_start_crews` prepend + constraint filter — threshold `TIERED_AUTO_THRESHOLD` in `src/server/api/execution.rs`; optional `tiered_scout_sims` / `tiered_top_k`.)* Continue tuning thresholds and UX so large searches reliably stay on the cheap scout path first.
- **Lean harder on analytical prefiltering** — *(Shipped: closed-form expected hull-damage ranking before Monte Carlo; explicit `analytical_prefilter_keep` or automatic cap via `analytical_prefilter_keep_auto` in `src/optimizer/mod.rs`, which also considers `max_candidates` and `tiered_top_k` when the client omits a keep value.)* Further tuning by workload profile as needed.
- **Warm-start from heuristics and prior winners** — *(Shipped: `warm_start_crews` on optimize + SPA localStorage; **warm-start cache key v3** (`SCHEMA` 3 in `[frontend/src/lib/optimizeWarmStart.ts](../frontend/src/lib/optimizeWarmStart.ts)`) fingerprints defender default, sorted support buff ids, chain grind, prioritize-below-decks, resolved below-decks slot count, and fast-discovery mode. **Cross-session:** same fingerprint is sent as `optimize_cache_key` when a profile is active; the server persists tiered winners to `[profiles/{id}/optimize_history.json](../src/data/profile_index.rs)` via `[src/data/optimize_history.rs](../src/data/optimize_history.rs)` and reuses confirmation stats on matching tiered runs (scout/confirm MC skipped for cache hits; workspace shows “Cached warm start” when `optimize_history_confirm_hits` is positive).)* Next — tighter coupling to heuristics seeds and optional OpenAPI/docs surfacing of the new fields.
- **Bias discovery around constraints early** — *(Shipped on registry path: `narrow_officer_pools_for_constraints` in `src/optimizer/crew_generator.rs` tightens pools from exclude / captain_must_be / seat must-includes before enumeration; group constraints still filtered post-generation.)* Further push rules earlier where sound without combinatorial blow-up.
- **Stay anchored to the real roster** — Keep discovery flows tightly filtered to owned officers, legal seat eligibility, and unlocked below-decks slots so compute is not spent on impossible crews.

### Next optimizer upgrades

- **Matchup-aware pruning rules (soft)** — *(Shipped: analytical prefilter composite score in [`src/optimizer/matchup_priors.rs`](../src/optimizer/matchup_priors.rs) — static LCARS gate hints, encounter heuristics (armada / Conqueror Borg / scout / outpost), client `warm_start_crews` overlap, and **optimize_history** reference crews for matching `optimize_cache_key` + chain fingerprint; then truncation via `analytical_prefilter_keep` / auto.)* **Open:** catalog-backed captain/bridge **synergy** priors when [`src/data/synergy.rs`](../src/data/synergy.rs) gains loadable officer-pair rows.
- **Novelty-aware ranking** — Reward crews that are both strong and materially different from already-known winners so discovery does not collapse into the same few lineages.
- **Automatic local learning loop** — *(Partial: per-profile `optimize_history.json` stores tiered and exhaustive two-phase results for `optimize_cache_key` and re-injects matching crews on the next run — see `src/data/optimize_history.rs` and `src/server/api/execution.rs`.)* Still open: use history to tune exploration limits automatically and broader “learn” feedback loops.
- **First-class fast-discovery mode** — *(Shipped: `fast_discovery` on optimize merges expanded `heuristics_seeds` crews into the main warm-start path so they share analytical prefilter + tiered or exhaustive Monte Carlo; workspace Strategy panel checkbox; OpenAPI field.)* Optional genetic refinement pass after tiered confirm remains future work.

### Operating principle

The intended direction is: **seed + prune + scout + confirm + learn**, rather than trying to brute-force ever-larger search spaces with uniform simulation effort.

## Combat buffs support

- **Data:** `[data/support_buffs.json](../data/support_buffs.json)` defines selectable ids (aligned with `[frontend/src/lib/supportBuffs.ts](../frontend/src/lib/supportBuffs.ts)` and constants in `[src/data/profile.rs](../src/data/profile.rs)`). Each entry may include `research_levels` (`rid` + `level`) merged in-memory via the same path as synced research, and optional `static_bonuses` (engine keys consumed by `apply_static_buffs_to_combatant` / mitigation). `exclusive_group` + `priority` resolve overlapping picks (e.g. Titan-A fort vs max).
- **API:** Optional `support_buffs: string[]` on simulate, optimize, compare crews, and replay-seed; capped length with unknown-id warnings in JSON responses.
- **Titan-A fort / max (alliance buff):** **Shipped:** Static bonuses from catalog text — Fort → `crit_damage` 1.25; Max → same plus `weapon_damage` 3.5. **Gated catalog combat research** — specific `rid`s whose combat bonuses apply only while the matching support buff is resolved (`titan_a_fortification` / `titan_a_max_fortification`) are **excluded** from normal profile research merge and from summaries without buff context; when the buff is active, those bonuses are converted to the same static shape as `static_bonuses` and merged in scenario via `[augment_static_buffs_with_support_gated_research](../src/data/support_buffs.rs)` so they stack with the Fortification static slice. Max-only lines use a separate `rid` list merged only when max fortification is selected. `[SupportBuffResearchGateState](../src/data/profile.rs)` drives `[research_derived_attack_phase_seats](../src/data/profile.rs)` conditional attack-phase seats to match.
- **Cerritos / Defiant:** **Shipped (gated research):** Same pattern as Titan — curated `rid` lists in `profile.rs` (`CERRITOS_SUPPORT_GATED_RESEARCH_RIDS`, `DEFIANT_REINFORCE_GATED_RESEARCH_RIDS`); merged into static layers when `cerritos_support` / `defiant_reinforce` is in resolved `support_buffs`. **Dual gate:** one Titan+Cerritos node (`TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID`) applies only when both Titan Fortify and Cerritos support are active. **Open:** tune or extend `static_bonuses` / `research_levels` in `support_buffs.json` when authoritative non-catalog values are needed.
- **Defender-side buffs & debuffs:** **Roadmap** — Today support buffs are attacker-scenario inputs. Model **separate defender-side** alliance buffs, debuffs (e.g. Mantis-style), and hostile-applied modifiers with explicit scenario fields and merge order, instead of folding everything into attacker static buffs or omitting defender context.
- **Mantis debuff:** Defender-side; not modeled here; subsumed by the defender-side roadmap item above until implemented.

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

### Roadmap / backlog

- **Cutover:** Research conditional seats use **only** the CombatEffectSpec adapter path (`research_derived_attack_phase_seats` delegates to `research_derived_attack_phase_seats_from_spec`). **Shipped (slice):** officer dynamic effects from `resolve_effect` / `resolve_officer_ability` use `compile_officer_combat_spec` for both `AbilityEffect` and `Ability.condition` (YAML conditions must round-trip through `lcars_condition_to_spec` or the effect is skipped). **Next:** further LCARS surface (e.g. deduplicating [`resolve_lcars_condition`](../src/lcars/resolver.rs) vs adapter-only call sites, or more `effect_type` coverage) only where parity and performance allow.

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
- **Strict validation report** — Report that lists all `buff`_* and unmapped conditions (e.g. strict mode or separate script).
- **Building summary API/UI** — Implemented: `GET /api/profile/buildings-summary` and Roster & Profile → Profile → “Buildings (sync → combat)”. Optional follow-up: editable building levels in the UI (today: sync or manual JSON / tooling such as `building_combat_bonuses`).

---

## Forbidden tech (implemented; ongoing maintenance)

Forbidden tech is implemented for ship combat; remaining items are maintenance/accuracy work (catalog upkeep, optional tier/level scaling).

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

### Partially implemented / gaps (roadmap items)

- **Accuracy** — **Done:** `accuracy` merges into `profile.bonuses` and scales ship `AttackerStats.accuracy` for mitigation/pierce-through; catalog values are treated as fractional bonuses (×(1 + sum)), same convention as `weapon_damage`. Remaining risk: in-game wording/scopes may differ, so validate with additional recorded-fight fixtures if mismatches appear.
- **Other combat stats** — Any future stat keys must be added to `normalize_profile_combat_stat` and wired in `apply_profile_to_attacker` / `apply_static_buffs_to_combatant` (or the mitigation path) before research mappings affect simulation.
- **Apex (shred / barrier)** — **Done:** `apex_shred` and `apex_barrier` are normalized combat keys; research/building merges feed `profile.bonuses`, and `apply_profile_to_attacker` adds them to the player ship combatant (shred on outbound apex math; barrier on counter-attack defense). Import still depends on `import_stfcspace_research.mjs` mapping upstream buffs to those stat names in `research_catalog.json`.
- **Conditional bonuses** — Armada-, class-, PvP-, or faction-scoped lines may be mapped as **global** ship bonuses when descriptions look generic; tightening requires engine/scenario context or buff-level overrides in `data/research/buff_id_to_stat.json`.
- **Catalog refresh** — After upstream drops, re-run `fetch_stfcspace_research.mjs` then `import_stfcspace_research.mjs`; use `--dump-unmapped` to extend `data/research/buff_id_to_stat.json` / `loca_id_to_stat.json` for buff ids that still do not resolve.
