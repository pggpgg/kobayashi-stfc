# Combat Engine Implementation Plan (Logical Order)

This plan turns `COMBAT_FEATURES_FROM_STFC_TOOLBOX.md` into execution phases with concrete deliverables.

## Phase 1 — Deterministic combat math foundation

### 1. Mitigation model (first implementation target)
**Goal:** Ship-accurate mitigation with clear inputs/outputs and test vectors.

- Add core function:
  - `mitigation(defender, attacker, ship_type) -> f64`
- Implement component function:
  - `f(x) = 1 / (1 + 4^(1.1 - x))`, with `x = defense / piercing`
- Implement weighted multiplicative combination:
  - `1 - (1 - cA*fA) * (1 - cS*fS) * (1 - cD*fD)`
- Add ship-type coefficient table:
  - Surveys `[0.3, 0.3, 0.3]`
  - Battleship `[0.55, 0.2, 0.2]`
  - Explorer `[0.2, 0.55, 0.2]`
  - Interceptor `[0.2, 0.2, 0.55]`

**Definition of done**
- Unit tests for known ratios and edge cases (low/high piercing, zero/near-zero defenses).
- Deterministic “golden” test vectors with expected mitigation to <=0.1% tolerance.

### 2. Effect stacking primitives
**Goal:** Build the universal composition rule before adding officer-specific mechanics.

- Implement stack groups by effect kind/stat key.
- Apply canonical combination:
  - `total = A * (1 + B) + C`
- Define strict typing/category for each effect source:
  - base (`A`), modifier (`B`), flat (`C`)

**Definition of done**
- Tests for additive-only, modifier-only, mixed stacks.
- Tests for ordering independence (commutativity inside each category).

## Phase 2 — Ability semantics and timing

### 3. Ability type activation rules
**Goal:** Correctly determine whether each officer effect is active.

- Model ability classes:
  - captain maneuver
  - bridge ability
  - below-deck ability
- Enforce seat/slot gating.
- Enforce scaling source rules (tier/synergy distinctions).

### 4. Boostability and timing gates
**Goal:** Prevent over-applying boosts and mirror observed timing behavior.

- Add boost metadata to effects (boostable/non-boostable).
- Restrict boosts to supported timing windows (combat begin/subround end).
- Include explicit exclusions for non-boostable effects.

**Definition of done for Phase 2**
- Activation matrix tests by seat configuration.
- Cross-tests with stacked boosts and unboostable controls.

## Phase 3 — Simulation and ingestion infrastructure

### 5. Raw combat log ingestion
**Goal:** Parse raw logs into an internal event model usable by simulator validation.

- Create parser for raw-log format.
- Normalize to event timeline + per-round snapshots.

### 6. Monte Carlo simulator runner
**Goal:** Use deterministic engine + RNG wrapper to generate outcome distributions.

- Add simulation API taking `iterations` and scenario payload.
- Emit summary stats + percentile bands.

**Definition of done for Phase 3**
- Replay/parity checks between parsed combat logs and engine outputs for selected fixtures.
- Performance baseline recorded for a standard scenario.

## Phase 4 — Fidelity and diagnostics

### 7. Compatibility toggles and known quirks
- Add optional duplicate-officer bug compatibility mode.
- Add temporary-combat-only state support and end-of-combat rollback.

### 8. Explainability + scenario tooling
- Add mitigation sensitivity table generator (+N defense/piercing deltas).
- Add “why” trace output for mitigation and stack decomposition.

**Definition of done for Phase 4**
- Debug trace snapshots included in fixtures.
- Sensitivity outputs validated against reference calculations.

---

## Immediate next execution slice (Sprint 1)

Focus exclusively on **Phase 1.1 Mitigation model**:

1. Introduce core mitigation module + ship-type coefficient map.
2. Add unit tests with hand-computed vectors.
3. Add a small CLI/dev entrypoint to print mitigation for a supplied stat block.
4. Document assumptions and tolerance thresholds.

This produces a stable mathematical core that subsequent effect and ability work can build on.
