# Roadmap

Planned features and priorities for Kobayashi.

## Codex Speed Demon

Speeding up crew discovery is primarily a search-efficiency problem, not a raw simulator-throughput problem. The simulator is already fast; the roadmap here is about spending Monte Carlo budget on the most promising crews first and learning from prior runs. For the product framing of that pipeline (seed → prune → scout → confirm → learn), see [README.md](../README.md#the-optimizer).

### Below-decks pool heuristics

- **Per-matchup below-decks pool sizing:** Today the pool narrows globally. A hostile with high mitigation may reward different below-decks stat priorities (e.g., pierce officers) than a glass-cannon hostile (e.g., hull HP officers). Compute stat profiles of top historical crews for a match-up and use them to weight below-decks officer scores for future runs, so the pool narrows intelligently rather than uniformly.

### Analytical prefilter improvements

- **Analytical damage floor per hostile tier:** The `prune_analytical_hull_fraction` (drop crews whose expected damage < X% of defender hull) uses a user-supplied fraction. Compute a sensible default per hostile tier: tougher hostiles can tolerate a smaller fraction (0.01), glass-cannon hostiles need a larger fraction (0.10) since the fight is short and every damage point matters. Derive from the hostile's hull-to-attack ratio in the shared scenario data.

## Combat buffs support

- **Support buffs → defender `Combatant` (PvP-shaped toggle):** **Partial** — Catalog field `static_bonus_target: defender_if_player_opponent` (`[data/support_buffs.json](../data/support_buffs.json)`) routes **direct** `static_bonuses` onto the defender [`Combatant`](../src/combat/types.rs) when the API uses `defender_opponent: player` (see [`aggregate_support_static_bonuses_split`](../src/data/support_buffs.rs), [`SharedScenarioData::support_defender_static_buffs`](../src/optimizer/monte_carlo/scenario.rs)). Titan-A Fortify / Max Fortify, Defiant Reinforce, and placeholder `mantis_sting` use this path; Cerritos remains attacker-routed.

- **Optional player-defender LCARS crew (API + scenario):** **Ship-vs-hostile baseline** — `POST /api/simulate` and `POST /api/optimize` accept optional `defender_crew` (same shape as attacker `crew`; non-empty `captain` activates). Officers resolve through the same LCARS path as the attacker; seats merge **after** hostile ship abilities; static keys fold into the defender [`Combatant`](../src/combat/types.rs) (isolytic cascade static stripped at combat-input build, same pattern as other static merges). Wired through [`SharedScenarioData`](../src/optimizer/monte_carlo/scenario.rs) (`player_defender_officer_seats`, `player_defender_static_buffs`) into `build_shared_scenario_data_from_registry` / standalone exhaustive paths. **Not supported:** `strategy: genetic` with a non-empty `defender_crew` (request validation error — genetic MC path does not rebuild shared scenario per that crew).

- **Inbound damage from attacker shots (defender crew prototype):** **Partial** — For outbound player weapon hits, defender [`TimingWindow::DefensePhase`](../src/combat/abilities.rs) effects (filtered with round `CombatContext`) merge into inbound resolution: combined with attacker phase stacks for damage-through; extra shield-mitigation and isolytic-defense bonuses after apex; [`ReceiveDamage`](../src/combat/abilities.rs) hooks for defender burning after hull damage. Experimental SIMD outbound fast path disables when inbound defense or receive-damage lists are non-empty. Semantics, ordering when both sides have round-start crews, and full timing parity vs attacker precompute remain documented assumptions — see **[§4.6 Effect ownership, `CombatContext`, and defender-side crews](DESIGN.md#46-effect-ownership-combatcontext-and-defender-side-crews)** in [`DESIGN.md`](DESIGN.md).

- **Still open / next:** Support-gated **research** from `augment_static_buffs_with_support_gated_research` stays on the **attacker** merge; defender-only profile maps (today [`apply_profile_to_attacker`](../src/data/profile.rs) is attacker-named); hostile-applied modifiers and full Mantis combat stats; **genetic** optimize with `defender_crew`; SPA / `POST /api/compare` if we want defender crew there; single merged inbound stack vs summed accumulators (risk called out in DESIGN); broader golden tests for shield break / self shield break / receive_damage when both sides have crews.

## Research faction gating (`attacker_faction` / `defender_faction`) — polishing

- **Polishing backlog:** Research bonuses now distinguish **player hull** gates (`attacker_faction` / `attacker_factions` → `research_owner_faction_bonuses`) vs **opponent** gates (`defender_faction` → conditional / seat path). Improvements still needed: hand-review ambiguous catalog strings (dual “your faction vs theirs” wording); tighten or replace heuristic `buff_id_to_stat.json` patches where `gen_research_faction_buff_patch.mjs` is wrong; extend engine **`ResearchBonusConditionKey`** when a single modifier must require **both** owner and defender faction; fix lossy merges for gated **`hull_hp` / `shield_hp`** (add vs multiply story); broaden tests, `research_combat_summary` / UI, and docs so gated lines are observable and assumptions are labeled.

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

- **`AbilityModifierSpec::Armor` vs `shield_deflection` (housekeeping):** The compiler maps LCARS `armor` and catalog **`shield_deflection`** (including research seat compile via `research_effect_spec_adapter.rs`) onto `AbilityModifierSpec::Armor`, which actually lowers to **`AbilityEffect::MitigationAdditive`** — a generic additive mitigation fraction for certain seat/counter-fire paths. Flat profile merge still uses **separate** bonus keys but adds both into `Combatant::mitigation`. **Work:** rename or split the modifier spec so labels match STFC semantics (armor ≠ shield deflection); e.g. rename `Armor` → `MitigationAdditive` at the spec layer, or add `ShieldDeflection` that compiles identically; update `effect_spec_compile.rs`, LCARS/research/hostile adapters, and tests; align docs so maintainers are not misled.
- **Future effect types:** Extend the IR/compiler when new `effect_type` values appear in `captain_ability` data (the allow-list test in `tests/lcars_captain_spec_parity_tests.rs` gates this).
- **Remove `resolve_lcars_condition`** after parity confidence and full caller migration.