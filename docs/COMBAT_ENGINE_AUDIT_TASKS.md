# Combat Engine Audit — Development Tasks

Tasks identified from a full audit of `src/combat/`, `tests/`, `src/lcars/`, and
`data/`. These are **independent of `ROADMAP.md`** and focus on correctness,
testability, and calibration infrastructure gaps discovered in the current
codebase.

Each task includes: **what is wrong**, **what mechanic is affected**, **what
code to change**, and **uncertainty remaining**.

---

## Phase 1 — Foundation (cleanup and safety net)

- [x] **1. Clean up dead and vestigial combat code**

  **What is wrong:** `src/combat/buffs.rs` contains a single unused struct
  `Buff { multiplier: f32 }`. All real buff resolution goes through
  `StatStacking<K>` in `stacking.rs` and `EffectAccumulator`. The
  `simulate_once()` function in `engine.rs` is a dead stub that always returns
  `FightResult { won: true }`.

  **What code to change:** Remove `buffs.rs` and its `mod buffs;` in
  `mod.rs`. Audit all `use` imports for stale `buffs::Buff` references (none
  expected). Remove or replace the `simulate_once()` stub with a proper
  single-round entry point or delete it if no callers exist.

  **Uncertainty:** Low. Both items are confirmed dead code by grep and the
  explore audit. Removing them is zero-risk.

- [x] **2. Add unit tests for core combat math modules**

  **What is wrong:** `stacking.rs`, `damage.rs`, and `crit.rs` have zero
  `#[test]` blocks. The `compose()` formula (`base × (1 + modifier) + flat`),
  the damage-through pipeline, crit resolution with hull breach, apex damage
  factor (`10000/(10000+barrier)`), and isolytic damage computation are all
  tested only indirectly through 87 integration tests in
  `tests/combat_tests.rs`. This slows down iteration and makes it hard to
  isolate regressions.

  **What code to change:** Add `#[cfg(test)] mod tests` blocks in:
  - `stacking.rs` — test `compose()` with zero/integral/extreme values,
    test category accumulation order independence, test `StatStacking` key
    uniqueness.
  - `damage.rs` — test `compute_damage_through_factor()` with edge
    mitigation/pierce values, test `compute_crit_multiplier()` with and
    without hull breach, test `compute_apex_damage_factor()` with zero/max
    barrier, test `compute_isolytic_taken()` with iso_bonus and defense.
  - `crit.rs` — test `resolve_vehicle_weapon_crit()` at 0% and 100% crit
    chance, verify RNG consumption is deterministic.

  **Uncertainty:** None. These are pure math functions with known
  invariants.

- [x] **3. Add unit tests for the ability evaluation and accumulator pipeline**

  **What is wrong:** `abilities.rs` and `effect_accumulator.rs` (~1700 lines)
  have no `#[test]` blocks. The condition gating (26 `AbilityCondition`
  variants), timing window dispatch (14 `TimingWindow` variants), assimilate
  scaling (`scale_effect()` with 0.75× and its exceptions list), and the
  effect-type-to-stacking-slot routing in `EffectAccumulator` are tested only
  through full-simulation integration tests.

  **What code to change:** Add `#[cfg(test)] mod tests` blocks in:
  - `abilities.rs` — test `filter_effects_by_condition()` with each
    condition variant (StatBelow/Above, MoraleActive, DefenderBurning, etc.),
    test `active_effects_for_timing()` across all 14 timing windows, test
    `scale_effect()` for all 34 `AbilityEffect` variants under assimilate
    (notably: `HostileCritDamageReduction` and
    `CumulativeOpponentShieldMitigationDebuff` are never scaled — verify).
  - `effect_accumulator.rs` — test each effect-type dispatch path,
    test the `PierceBonus` → `AttackPhaseDamage` conversion (with the
    `value * base_attack * 0.5` factor), test `PreAttackDamage` →
    `AttackPhaseDamage` carry-forward, test round-end accumulator reset.

  **Uncertainty:** None. These are combinatorial coverage tests against
  known code paths.

---

## Phase 2 — Correctness fixes

- [x] **4. Wire the hostile mitigation formula into the in-shot damage loop**

  **What is wrong:** The `mitigation.rs` formulas (component mitigation,
  mystery factor, [0.16, 0.72] hostile clamps) are computed once at
  `Combatant` construction time and baked into `defender.mitigation` as a
  static scalar. The shot loop reads `1.0 - defender.mitigation` directly.
  Real STFC mitigation depends on defender armor/deflection/dodge and
  attacker armor_piercing/shield_piercing/accuracy — all of which officer
  abilities can modify mid-combat (e.g., officer abilities that apply
  `MitigationAdditive`, or debuffs that reduce defender stats).

  **What code to change:** Refactor the per-shot damage path in
  `engine.rs` to call `mitigation_for_hostile()` dynamically rather than
  reading a pre-computed scalar. The function already exists; it needs to be
  called with the current `DefenderStats` and `AttackerStats` at damage
  computation time. This likely means threading the `MitigationAdditive`
  effect accumulator value through to the mitigation call, since officer
  abilities can add to the attacker's mitigation value.

  **Uncertainty:** Medium. The `MitigationAdditive` effect in the current
  engine applies to the **attacker's** mitigation on counter-fire, not to the
  hostile's mitigation. How officer stat debuffs affect hostile mitigation in
  the real game needs verification. The code change is mechanical, but the
  correctness against real STFC behavior should be verified with fight
  export comparison.

- [ ] **5. Wire defender crew `ShotsBonus` in counter-fire**

  **What is wrong:** The counter-fire path in `engine.rs` hardcodes
  `effective_shots_for_weapon(def_base_shots, 0.0)` with the explicit comment:
  "defender crew `ShotsBonus` is not wired here yet." Defender officer
  abilities that grant bonus shots (e.g., SNW Spock, D'Vor Feesha crew
  variants) take no effect during counter-fire.

  **What code to change:** In the counter-fire per-weapon loop
  (`engine.rs`), extract the defender crew's `RoundStart` shots bonus from
  the defender's `EffectAccumulator` (mirroring how the attacker's
  `B_shots(r)` is computed) and pass it to `effective_shots_for_weapon()`.
  This requires the defender's round-start accumulator to track `ShotsBonus`
  contributions.

  **Uncertainty:** Low. The mechanic is well-understood (bonus shots are
  additive to base weapon shots). The only question is whether the STFC game
  applies the same stacking rules to defender shots bonuses as attacker ones,
  which is almost certainly yes.

- [ ] **6. Apply `CombatEnd` timing window effects to combat math**

  **What is wrong:** The `CombatEnd` timing window fires at the end of the
  fight loop, but its effects are only traced via `trace_collector`. No hull
  damage, shield damage, healing, or other effects are applied to the combat
  result. This means officers with CombatEnd triggers (e.g., final-blow
  damage, deathrattle effects, combat-summary bonuses) have their effects
  silently discarded.

  **What code to change:** In the combat-end section of `engine.rs`
  (~line 2547), after firing `CombatEnd` timing window effects, apply the
  accumulated effects to the appropriate side's hull/shield. If the
  CombatEnd damage kills the opponent, the winner should reflect that. This
  may also affect shield/hull regen at CombatEnd.

  **Uncertainty:** Low for the code change. Medium for which specific
  officer abilities use CombatEnd triggers — the LCARS data should be
  searched for `trigger: combat_end` to identify affected officers.

- [ ] **7. Add scaling hooks for burning damage**

  **What is wrong:** Burning is hardcoded at 1% of max hull per round
  (`BURNING_HULL_DAMAGE_PER_ROUND = 0.01`) with the code comment explicitly
  noting "no officer/research scaling of that rate." Real STFC has officers
  that amplify burning damage (e.g., SNW Una, or burning-related research
  nodes).

  **What code to change:** Add a new `AbilityEffect` variant (e.g.,
  `BurningDamageFraction` or `BurningHullDamageScale`) that stacks via the
  existing `StatStacking` system. Wire it into the burning tick code in
  `engine.rs` to replace the constant `0.01` with `0.01 * (1.0 +
  burning_scale)`. Read the stacked value from the appropriate accumulator
  (likely the round-end accumulator since burning ticks at round end).
  Add the modifier to `EffectStatKey` and route it in
  `EffectAccumulator::add_effects()`.

  **Uncertainty:** Low for the stacking mechanics. Medium for which
  specific game sources modify burning intensity — the exact scaling formula
  (additive? multiplicative? separate burn ticks?) should be verified against
  fight exports before claiming full fidelity.

---

## Phase 3 — Feature gaps

- [ ] **8. Implement weapon range mechanics**

  **What is wrong:** STFC ships have short/medium/long-range weapons with
  range-dependent damage modifiers (e.g., explorers deal reduced damage at
  close range, interceptors at long range). The Kobayashi engine has no range
  concept at all — `WeaponStats` has no `range` field and the shot loop does
  not apply range-vs-class damage modifiers.

  **What code to change:**
  1. Investigate data.stfc.space ship JSON for per-weapon range data
     (likely a `range` or `weapon_range` field on each weapon entry).
  2. Add `WeaponStats.range: Option<WeaponRange>` enum
     (`Short | Medium | Long`).
  3. Add a range-vs-class damage multiplier table (likely
     short→explorer bonus, long→explorer penalty, etc. — exact values TBD).
  4. Apply the range multiplier in the per-hit damage loop after computing
     `weapon_attack` but before the damage-through calculation.

  **Uncertainty:** High. The exact range-vs-class damage modifier values
  for STFC are not publicly documented. This task starts with research: find
  the formula from community sources, data.stfc.space, or from parsing
  game client data. Treat the formula as an explicitly labeled uncertain
  mechanic (`// UNCERTAIN: range formula from community testing, not
  confirmed`) until verified with fight exports.

---

## Phase 4 — Calibration infrastructure

- [ ] **9. Build per-event trace alignment for fight replay parity**

  **What is wrong:** Current fight replay parity (`log_ingest.rs` →
  `parity_within_tolerance()`) only compares summary aggregates: total_damage,
  hull_remaining, rounds. When a fight doesn't match, the only signal is
  "drifted by N damage" with no indication of where or why. The
  `ingested_events_to_combat_events()` function exists but is only tested for
  structural conversion, not for actual event-by-event alignment.

  **What code to change:** Build an alignment function that:
  1. Takes a simulator `TraceCollector` output and an ingested
     `IngestedCombatLog`.
  2. Aligns events by `(round_index, weapon_index, hit_index, event_type)`.
  3. For `Damage` events, computes `sim_damage - log_damage` and reports the
     contributing components: attack delta, mitigation delta, pierce delta,
     crit multiplier delta, proc multiplier delta, apex delta, isolytic
     delta.
  4. Reports the first point of divergence with the full component breakdown.
  5. Optionally produces a markdown table of all aligned rounds for visual
     inspection.

  Add tests using the existing `sample_combat_log.json` and
  `multi_weapon_round_log.json` fixtures to verify alignment produces correct
  structural pairing.

  **Uncertainty:** Low for the alignment mechanics (deterministic pairing
  by structured keys). The usefulness depends on whether real fight exports
  contain enough per-event detail — currently `fight_export_weapon_index.tsv`
  has a Weapon Index column, but per-hit-level detail from game exports may
  be limited.

- [ ] **10. Expand drift fixture coverage for under-tested scenarios**

  **What is wrong:** Only 5 drift fixtures exist, covering basic soak,
  dual-weapon ordering, stall margin, and research weapon damage pooling.
  Missing scenarios that exercise known correctness gaps:
  - Hostile-applied status effects on the player (e.g., a defender crew that
    applies Burning or Hull Breach to the attacker).
  - Multi-weapon with simultaneous Hull Breach → crit damage interaction.
  - Morale + Burning + Hull Breach all active simultaneously with stacking
    interactions.
  - Extreme stat values: 90% pierce on a low-mitigation defender, 85%
    mitigation vs high pierce, 5× crit damage.
  - Conqueror Borg beam suppression with officer abilities active (verify
    the suppression doesn't accidentally suppress unrelated effects).
  - On-kill hull regen chaining across multi-kill combat (N > 1 chain).

  **What code to change:** Add 6–8 new JSON fixture files in
  `tests/fixtures/recorded_fights/` following the existing drift fixture
  format (attacker/defender Combatant JSON, `SimulationConfig`, and
  `reference_bands` with min/max for total_damage, rounds, hull_remaining,
  shield_remaining). Each fixture must include:
  - Reference bands derived from verified game behavior or mathematical
    invariants (e.g., "Hull Breach adds exactly 50% crit damage" →
    test that crit damage is within 1e-6 of expected).
  - A comment block explaining what mechanic is being calibrated.
  - A `source` field noting "mathematical invariant" or "community-verified"
    vs "estimated from fight exports."

  **Uncertainty:** Low for the fixure format and harness. The main
  uncertainty is whether game-accurate reference values exist for the
  edge-case scenarios — some may need to be marked as "invariant only"
  (verifying mathematical consistency, not game parity) until real fight
  exports become available.
