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
