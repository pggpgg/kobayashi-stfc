## Potential tasks tracker (20)

This file tracks a curated set of candidate tasks (features, improvements, refactors) grounded in the current codebase. It is not intended to replace the roadmap; it exists as an execution-friendly checklist.

### Combat engine mechanics (correctness)

- [x] 1. Add a first-class `accuracy`/evasion stat path end-to-end (data → stacking → engine usage), so ship/officer effects that mention accuracy can be modeled.
- [x] 2. Implement hostile-side effects on defender return fire (defender “crew” / abilities / debuffs), applied during counter-attack resolution.
- [x] 3. Expand LCARS condition support to include faction gating and morale/burning/hull-breach state predicates as explicit condition nodes.
- [x] 4. Replace the LCARS `crit_chance` / `crit_damage` placeholder mapping with typed crit modifiers applied in the crit step (not an attack-multiplier approximation).
- [x] 5. Add an “after-shot / subround-end” timing window to model effects that modify the next shot(s) within the same round.
- [x] 6. Add per-weapon stat overrides (pierce/crit/proc/shots) in `WeaponStats` and thread through scenario building to better match multi-weapon logs.
- [x] 7. Improve trace explainability: emit optional per-effect contribution breakdown for key stacks in trace mode.

### Optimizer / Monte Carlo (performance + output quality)

- [x] 8. Add confidence intervals/error bars to optimize outputs (win rate, hull remaining, R1 kill rate) and surface them in the UI.
- [x] 9. Implement an analytical pre-filter stage used by optimize strategies to prune obviously bad crews before Monte Carlo (explicitly labeled approximate).
- [x] 10. Add optimize constraints (must-include, exclude, group count, seating rules) exposed in API and UI.
- [x] 11. Make below-decks slot count a first-class scenario parameter (ship-aware defaults) and update candidate generation accordingly.
- [x] 12. Add a deterministic “replay one seed” endpoint that returns a compact trace + summary for a chosen seed from an optimize result.
- [ ] 13. Improve async optimize job UX: richer SSE status payload (phase, throughput, ETA, top-N preview) and show it live in Workspace.

### Data persistence + provenance

- [ ] 14. Results library schema migration + provenance (store data version, migrate old saved results/presets, show provenance in UI).
- [ ] 15. Add a mechanics coverage report endpoint (implemented/partial/ignored) derived from LCARS + ability catalogs and expose via `/api/mechanics/coverage`.
- [ ] 17. Add a profile diff/attribution inspector showing exactly which sources contributed which effective stats.
- [ ] 18. Improve roster import diagnostics (unknown officer mapping, alias suggestions, tier/level bounds) with actionable messages.

### UI / workflow

- [ ] 16. Add a “compare crews” view showing side-by-side distributions (rounds-to-kill, hull remaining, proc rates), not just means.

### Testing / regression protection

- [x] 19. Add more recorded-fight calibration fixtures and a drift summary harness (what got closer/farther) to spot regressions.
- [ ] 20. Refactor `simulate_combat_with_defender_faction`/combat loop into smaller testable units while preserving deterministic RNG consumption order.

### Suggested execution order

The order below is tuned for “correctness first, then explainability, then performance/UX,” while keeping refactors late to reduce churn:

1. 1 (accuracy/evasion stat path) — unlocks many currently-unmodelable ability texts.
2. 4 (typed crit modifiers) — removes a known approximation in LCARS resolution.
3. 3 (LCARS conditions: faction + state predicates) — makes modeling more declarative, less hard-coded.
4. 5 (after-shot timing window) — needed for a class of effects that currently can’t be represented correctly.
5. 6 (per-weapon overrides) — improves parity with multi-weapon traces/logs.
6. 7 (trace contribution breakdown) — makes future mechanic work debuggable and reviewable.
7. 12 (replay-one-seed endpoint) — accelerates debugging and UI explainability.
8. 19 (more fixtures + drift harness) — adds guardrails as correctness surface grows.
9. 9 (analytical pre-filter) — reduces optimizer cost without changing the engine.
10. 8 (confidence intervals) — improves ranking interpretability.
11. 11 (below-decks slot count) — realism + search space correctness.
12. 10 (constraints) — user-facing power feature.
13. 13 (richer progress SSE) — UX polish for long runs.
14. 16 (compare crews) — UX + explainability.
15. 15 (coverage endpoint) — documentation + guardrails for “graceful degradation.”
16. 14 (results schema migration/provenance) — long-term stability.
17. 17 (profile attribution) — explains “why” outcomes differ.
18. 18 (import diagnostics) — improves onboarding and reduces data issues.
19. 20 (combat loop refactor) — do last, after mechanics stabilize.
