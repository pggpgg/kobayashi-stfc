# Combat Feature Backlog (derived from stfc-toolbox.vercel.app)

Source pages reviewed:

- `/mitigation`
- `/game-mechanics`
- `/simulator`
- `/combatlog`
- `/ship-comparison`

## High-priority features (core combat accuracy)

1. **Separate raw combat pipeline from CSV combat log parser**
  - Add parser/import model for raw combat logs as a first-class input format.
  - Preserve subround-level events and intermediate stat state snapshots to support mechanics reverse-engineering.
  - Encode canonical round/sub-round ordering from observed client event identifiers:
    - `START_ROUND` → `HULL_REPAIR_START/END` (once per round, before first sub-round)
    - Per sub-round: officer/ship abilities apply, then forbidden tech + chaos tech buffs, then attacks for that sub-round weapon index
    - `END_ROUND`: burning tick (1% of target max hull per round while burning active), temporary-effect cleanup, then next round (up to 100 rounds)
  - Persist full ordered event stream (including repeated per-ship applications) even when the UI collapses duplicate ability/FT log lines.
2. **Add Monte Carlo combat simulator mode**
  - Build simulation runner taking combat snapshot input + iteration count.
  - Return confidence intervals / distributions (not just mean outcomes) for damage and survival.

## Medium-priority features (mechanics completeness)

1. **Implement ability-boost interaction rules**
  - Add boost logic for effects that modify maneuver/ability potency.
  - Respect “boostable at combat begin or subround end” timing restriction.
  - Keep a per-effect boostability flag so unsupported effects are not amplified.
2. **Model temporary-combat-only effects**
  - Add transient state for combat-only gains that are removed after battle (e.g., temporary hull restoration behavior like Leslie).
  - Ensure post-combat state rollback for those effects.
3. **Add duplicate-officer bug compatibility toggle**
  - Introduce optional simulation mode reproducing known duplicate-officer bug behavior for log parity.
4. **Improve stat nomenclature and baseline definitions**
  - Standardize HHP/SHP and component stat naming.
  - Define “base” values consistently (component bonuses + tier-max level assumptions, excluding research unless toggled on).

## Validation and tooling features

1. **Mitigation scenario analyzer**
  - Add tool endpoint that computes sensitivity deltas (“+1000 armor”, “+1000 all defenses”, etc.) and reports mitigation and damage-taken delta.
2. **Mechanics regression corpus from raw logs**
  - Create a fixture suite from representative raw logs.
    - Add snapshot tests for mitigation%, per-round damage, and effect-stack outcomes.
3. **Engine explainability output (mitigation decomposition)**
  - Extend optional debug trace for mitigation with per-step calculations not covered by today’s scalar `mitigation_calc` events:
    - defense/piercing ratios per component
    - each `f(x)` value
    - weighted component contributions (`cA`, `cS`, `cD`)
    - final multiplicative combination

## Future / optional (sub-round and weapons)

1. **Per-weapon pierce/crit/proc from upstream data**
  - The engine already supports optional per-weapon overrides on `WeaponStats` (fallback to combatant-level when unset).
  - When upstream or STFC data differs by weapon, extend normalizers / importers so those fields are populated consistently.

## Suggested implementation order

1. Raw-log parser and simulator integration (client-fidelity stream and snapshots)
2. Monte Carlo snapshot mode + damage/survival distributions
3. Ability boost rules + temporary combat-only state
4. Compatibility toggles + regression suite
5. Mitigation analyzer endpoint + trace decomposition for mitigation
