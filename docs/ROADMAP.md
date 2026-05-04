# Roadmap

Planned features and priorities for Kobayashi.

## Codex Speed Demon

Speeding up crew discovery is primarily a search-efficiency problem, not a raw simulator-throughput problem. The simulator is already fast; the roadmap here is about spending Monte Carlo budget on the most promising crews first and learning from prior runs. For the product framing of that pipeline (seed → prune → scout → confirm → learn), see [README.md](../README.md#the-optimizer).

### Heuristics seed quality & learning loop

- **Seed ↔ optimizer feedback loop:** Today heuristics seeds (`data/heuristics/*.txt`) are static community-curated lists. There is no feedback from optimizer results back into seed quality or generation. Close the "learn" step of seed → prune → scout → confirm → **learn**: auto-save top-K confirming crews for a match-up as `optimize_history` entries, then generate fresh seed files from those entries on the next run against the same ship+hostile. A configurable "seed recency" window (keep last N entries per match-up, decaying older ones) would prevent stale seeds from crowding out new discoveries.
- **Seed hit-rate tracking:** Track, per seed line, how often its crews appear in the top-K results. Log a metric (`seed_hit_rate`) so maintainers can prune low-impact seed entries and the optimizer can de-prioritize seeds that consistently underperform for a given hostile faction. Hit-rate below a threshold (e.g., none of its crews reach top-20 in 3 consecutive runs) triggers a deprecation warning in CLI validation.
- **Matchup-aware seed filtering:** Seed lines today are applied uniformly regardless of hostile. Add optional hostile-tag annotations to seed files (e.g., `# @faction:federation @ship:explorer`) so seed crews relevant to one hostile type don't pollute runs against unrelated hostiles. When no tag matches, the seed line is skipped. This keeps seed files clean and prevents Borg-specific seed crews from cluttering Romulan optimize runs.
- **Seed exploration budget:** Heuristics seeds are "community-promising" — allocate them extra scout budget. Seed-derived crews should receive a configurable multiplier on scout trials (e.g., 1.5×) in the adaptive coarse→refine scout pass, so well-known crews get a fairer statistical comparison against the unknown generated pool. Controlled via `OptimizerBudgetHints` (`tiered_scout_seed_mult`).

### Below-decks pool heuristics

- **Tiered below-decks filtering (not just binary):** **Done.** `BelowDecksPoolMode` (`src/data/heuristics.rs`) replaces `only_below_decks_with_ability: bool` with three tiers: **(1) strict** — only officers with a combat-modifier below-decks ability (default), **(2) scored** — all below-decks-ability officers ranked by combat relevance (combat → ambiguous/missing → economy-only) with LCARS Attack+Defense+Health as a power tiebreaker, **(3) relaxed** — all officers ranked by power. API field `below_decks_pool_mode` (`strict|scored|relaxed`) supersedes the legacy `allow_below_decks_without_combat_ability` boolean (still accepted for backward compatibility). Wired through `OfficerPools` build path, `CandidateStrategy`, `GeneticConfig`, and the SPA's `OptimizePanel` 3-way select.
- **Officer power-aware below-decks default:** When below-decks slots are plentiful and the combat-relevant pool is thin, the scorer should surface high-power officers whose below-decks ability modifier is unknown or ambiguous rather than dropping them. Many officers have a below-decks slot but no modifier annotation yet — these should rank above known-economy officers but below known-combat when pools need padding. The `heuristics.rs` `has_combat_below_decks_slot_ability` currently treats missing `modifier` as "ambiguous and kept" — formalize this as a scored tier rather than a binary gate.
- **Per-matchup below-decks pool sizing:** Today the pool narrows globally. A hostile with high mitigation may reward different below-decks stat priorities (e.g., pierce officers) than a glass-cannon hostile (e.g., hull HP officers). Compute stat profiles of top historical crews for a match-up and use them to weight below-decks officer scores for future runs, so the pool narrows intelligently rather than uniformly.

### Bridge & captain synergy heuristics

- **Synergy-strength scoring for bridge pairs:** The current bridge filter keeps officers that share a synergy group with the captain OR have a bridge-slot ability. Extend this to a score: same-group + bridge-ability (highest), same-group only, bridge-ability only, neither (dropped). Use this score as a prior in the analytical prefilter so the optimizer front-loads crews with stronger bridge synergy before spending Monte Carlo budget.
- **Captain+bridge pair co-occurrence learning:** `officer_learning.rs` already tracks per-officer scores. Extend to `(captain, bridge_officer)` pair scores so the optimizer learns which captain+bridge duos consistently perform well together for a match-up, not just individual officers. Feed these pair scores into the analytical prefilter prior and the below-decks generator's epsilon-greedy sampling.

### Scout budget heuristics

- **Heuristic-driven asymmetric scout allocation:** Crews from heuristics seeds, warm-start, and optimize-history reference crews should receive a higher initial scout trial allocation than randomly-generated crews. The priority-queue scout (`tiered_scout_priority_queue`) already supports per-crew trial variance; extend it with a "prior bonus" field so promising crews get head-start trials. This avoids wasting budget on hopeless generated crews before seed crews are fairly evaluated.
- **Budget hints from learning signals:** `learning_signals.rs` computes `top_margin`, `officer_diversity`, and `captain_bridge_stagnation`. When `top_margin` is small (tight race), automatically increase scout trials on the next run to resolve the ambiguity. When `captain_bridge_stagnation` is high (same captain+bridge dominating), temporarily boost the below-decks exploration rate (epsilon) to discover alternative compositions. Wire these signals into `OptimizerBudgetHints` auto-generation so the profile tunes itself over repeated runs.
- **Adaptive scout cap by pool size:** The current adaptive scout cap scales with total candidate count. Extend it to also scale with the ratio of seed-derived candidates to generated candidates — when seeds cover a large fraction of the candidate pool (e.g., >30%), reduce the per-crew scout cap since many crews share structural similarity with known-good seeds.

### Analytical prefilter improvements

- **Static gate pruning as a default (conservative):** `prune_static_gate_max_fraction` (drop crews where ≥95% of conditional abilities fail to match) is opt-in today. Make it a conservative default (e.g., `0.95`) for all non-genetic paths, since a crew whose abilities are 95% gated on a mismatched faction/ship-type is almost certainly worse than alternatives. The SPA can expose a toggle to disable this pruning for edge cases.
- **Analytical damage floor per hostile tier:** The `prune_analytical_hull_fraction` (drop crews whose expected damage < X% of defender hull) uses a user-supplied fraction. Compute a sensible default per hostile tier: tougher hostiles can tolerate a smaller fraction (0.01), glass-cannon hostiles need a larger fraction (0.10) since the fight is short and every damage point matters. Derive from the hostile's hull-to-attack ratio in the shared scenario data.
- **Analytical prefilter keep auto-tuning from history:** When `optimize_history` shows that previous runs found strong crews outside the analytical top-N, automatically increase the `analytical_prefilter_keep_auto` cap for the next run. Conversely, when the analytical top-N perfectly captured the eventual Monte Carlo top-K, the cap can tighten. This closes the loop between the cheap analytical sort and the expensive Monte Carlo pass.

### Warm-start enrichment

- **Blurred warm-start for adjacent match-ups:** When a player has optimize history for hostile A (e.g., a specific Romulan hostile), extend warm-start to "blur" those crews into the candidate pool for hostile B (another Romulan hostile with similar stats and the same faction tag). The blur replaces one below-decks officer with the next-best-scored alternative, generating a few neighboring candidates. This transfers learning across similar hostile profiles without requiring explicit runs against every hostile.
- **Warm-start freshness weighting:** Stale warm-start crews (from history older than N days) should receive reduced weight in the analytical prefilter prior compared to fresh entries. This prevents meta shifts from being locked in by old history, while still giving veteran crews a small edge over completely random candidates.

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

Current state: officers are **authored** in LCARS; dynamic (non-static) effects already adapt to `**CombatEffectSpec`** and compile into engine types (`lcars_effect_to_combat_effect_spec` → `compile_officer_combat_spec` in `src/lcars/resolver.rs`). Static passive-permanent `stat_modify` / mapped combat tags still use the static-buff merge path. Research remains **catalog stat rows** (`research_catalog.json`): unconditional combat keys merge into `profile.bonuses`, while conditional `crit_chance` / `crit_damage` / `weapon_damage` rows become attack-phase seats via `**research_derived_attack_phase_seats_from_spec`** — i.e. the same IR + compiler, not a separate ad-hoc Rust routing layer (`src/data/research_effect_spec_adapter.rs`). Other keys (e.g. morale-gated isolytic, isolytic cascade) still use scenario/profile wiring documented in `src/data/profile.rs`. Residual divergence between LCARS YAML, row-shaped research ingest, and paths that bypass the spec keeps some drift and maintenance surface until more surfaces normalize through one story.

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

- **Battlelogs:** Mod `**battlelogs`** batches are persisted to `profiles/{id}/battlelogs.imported.json` (rolling **last 50** objects; see [SYNC.md](SYNC.md)). **Next:** wire stored logs into calibration, recorded-fight fixtures, replay, or analysis tooling (consumption path and schema interpretation still TBD).
- **Non-priority / deferred:** the mod also sends payload types that are accepted (200) but not stored. This is intentionally **not** a combat-accuracy priority right now, so it is tracked only as a “maybe later” note: traits, slots, resources, missions, inventory, jobs, and any additional raw tech-tree payloads if the mod exposes shapes beyond research project levels (already covered by `research` sync). **Note:** stfc-mod’s JSON `type: "tech"` is forbidden/chaos tech (same as `ft`) and is already persisted to `forbidden_tech.imported.json`.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.

---

## Forbidden tech (open maintenance)

Forbidden tech ship-combat support is in place; the open roadmap here is maintenance and fidelity work (catalog upkeep, optional tier/level scaling).

### Maintenance / gaps

- **Catalog `fid` (maintenance):** CI requires every committed catalog item to have a unique `fid` (`forbidden_chaos::sync_readiness_tests`) so stfc-mod sync can match bonuses by `fid`. Rows without a `fid` never apply for synced players. Workflow: upstream `summary-forbidden_tech.json` + `translations-forbidden_tech.json`, manual CSV `fid`, or `scripts/build_chaos_tech_csv_rows.mjs` (chaos rows from live `data.stfc.space/forbidden_tech/{id}.json`). See [data/README.md](../data/README.md) § Forbidden tech.
- **Level/tier:** `ForbiddenTechEntry` includes `level` and `tier`. The merge can optionally scale catalog bonuses by `tier`/`level` when `KOBAYASHI_FT_LEVEL_TIER_SCALING=1` is set (linear scaling within a tier; conservative behavior when catalog tier disagrees with synced tier). `build_shared_scenario_data_standalone` and the registry path both use the same merge helper and env flag. The exact in-game scaling is still uncertain, so scaling remains opt-in until confirmed.
- **Combat timing:** DESIGN documents the intentional approximation: forbidden/chaos bonuses are applied at **profile merge**, not as a separate per-sub-round phase. A per-sub-round FT phase would require new evidence and engine work. See [DESIGN.md](DESIGN.md) §3.6 Notes.
- **Chaos data fidelity:** Bulk chaos rows are generated with heuristics (PvP-only / armada / proc lines approximated or skipped). Review `data/import/forbidden_chaos_tech.csv` when balancing matters; re-run `node scripts/build_chaos_tech_csv_rows.mjs` after adjusting the script.