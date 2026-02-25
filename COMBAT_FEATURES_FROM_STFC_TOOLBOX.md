# Combat Feature Backlog (derived from stfc-toolbox.vercel.app)

Source pages reviewed:
- `/mitigation`
- `/game-mechanics`
- `/simulator`
- `/combatlog`
- `/ship-comparison`

## High-priority features (core combat accuracy)

1. **Implement mitigation math (explicit model + coefficients)**
   - Add component mitigation for armor, shield deflection, and dodge using defense/piercing ratio.
   - Use fitted curve: `f(x) = 1 / (1 + 4^(1.1 - x))` where `x = defense / piercing`.
   - Combine components multiplicatively:
     - `mitigation = 1 - (1 - cA * f(dA/pA)) * (1 - cS * f(dS/pS)) * (1 - cD * f(dD/pD))`
   - Apply ship-type coefficient vectors:
     - Surveys: `[0.3, 0.3, 0.3]`
     - Battleship: `[0.55, 0.2, 0.2]`
     - Explorer: `[0.2, 0.55, 0.2]`
     - Interceptor: `[0.2, 0.2, 0.55]`

2. **Separate raw combat pipeline from CSV combat log parser**
   - Add parser/import model for raw combat logs as a first-class input format.
   - Preserve subround-level events and intermediate stat state snapshots to support mechanics reverse-engineering.
   - Encode canonical round/sub-round ordering from observed client event identifiers:
     - `START_ROUND` → `HULL_REPAIR_START/END` (once per round, before first sub-round)
     - Per sub-round: officer/ship abilities apply, then forbidden tech + chaos tech buffs, then attacks for that sub-round weapon index
     - `END_ROUND`: burning tick (1% initial hull), temporary-effect cleanup, then next round (up to 100 rounds)
   - Persist full ordered event stream (including repeated per-ship applications) even when the UI collapses duplicate ability/FT log lines.

3. **Add Monte Carlo combat simulator mode**
   - Build simulation runner taking combat snapshot input + iteration count.
   - Return confidence intervals / distributions (not just mean outcomes) for damage and survival.

4. **Implement canonical effect stacking model**
   - For same effect kind, support stacking:
     - `total = A * (1 + B) + C`
   - Track each buff/debuff as one of three categories:
     - base contribution (`A`)
     - multiplicative modifier (`B`)
     - flat additive (`C`)

5. **Model ability activation semantics by ability type**
   - Distinguish captain maneuver, bridge ability, and below-deck ability.
   - Enforce activation gates (captain seat vs any bridge seat vs unlocked below deck slots).
   - Apply tier/synergy scaling constraints per type (maneuver vs ability behavior differs).

## Medium-priority features (mechanics completeness)

6. **Implement ability-boost interaction rules**
   - Add boost logic for effects that modify maneuver/ability potency.
   - Respect “boostable at combat begin or subround end” timing restriction.
   - Keep a per-effect boostability flag so unsupported effects are not amplified.

7. **Model temporary-combat-only effects**
   - Add transient state for combat-only gains that are removed after battle (e.g., temporary hull restoration behavior like Leslie).
   - Ensure post-combat state rollback for those effects.

8. **Add duplicate-officer bug compatibility toggle**
   - Introduce optional simulation mode reproducing known duplicate-officer bug behavior for log parity.

9. **Improve stat nomenclature and baseline definitions**
   - Standardize HHP/SHP and component stat naming.
   - Define “base” values consistently (component bonuses + tier-max level assumptions, excluding research unless toggled on).

## Validation and tooling features

10. **Mitigation scenario analyzer**
    - Add tool endpoint that computes sensitivity deltas (“+1000 armor”, “+1000 all defenses”, etc.) and reports mitigation and damage-taken delta.

11. **Mechanics regression corpus from raw logs**
    - Create a fixture suite from representative raw logs.
    - Add snapshot tests for mitigation%, per-round damage, and effect-stack outcomes.

12. **Engine explainability output**
    - Add optional debug trace showing per-step calculations:
      - defense/piercing ratios
      - each `f(x)` value
      - weighted component contributions
      - final multiplicative combination
      - stack decomposition (`A`, `B`, `C`)

## Future / optional (sub-round and weapons)

13. **Per-weapon pierce/crit/proc in data and engine when STFC differentiates them**
    - Today only attack is per-weapon; pierce, crit_chance, crit_multiplier, proc_chance, proc_multiplier are combatant-level for all weapons.
    - When upstream or STFC data differs by weapon, add optional per-weapon fields to `WeaponStats` / `WeaponRecord`, normalizer output, and engine resolution (fallback to combatant-level when absent).

14. **Officer effects that trigger after a shot and affect the next shot(s) in the same round**
    - Some officers grant e.g. extra crit chance or crit damage to the *next* shot(s) of that round (effects that trigger “after a shot” and apply to subsequent sub-rounds).
    - Requires a timing such as SubroundEnd (or “after shot”) and carrying buff state (e.g. “+X% crit chance for next shot”) into the next sub-round within the same round; apply when resolving the following weapon(s).

## Suggested implementation order

1. Mitigation model + tests
2. Effect stacking + typed buff/debuff system
3. Ability activation and scaling rules
4. Raw-log parser and simulator integration
5. Compatibility toggles + regression suite
