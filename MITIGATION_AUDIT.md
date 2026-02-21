# Mitigation logic audit (optimizer + combat engine)

Reference description checked:

- `mitigation = 1 - (1 - cA * f(dA / pA)) * (1 - cS * f(dS / pS)) * (1 - cD * f(dD / pD))`
- `f(x) = 1 / (1 + 4^(1.1 - x))`
- `pA`, `pS`, `pD` are attacker piercing stats clamped at a minimum of `0`
- coefficients: surveys/armadas `(0.3, 0.3, 0.3)`, battleships `(0.55, 0.2, 0.2)`, explorers `(0.2, 0.55, 0.2)`, interceptors `(0.2, 0.2, 0.55)`

## What matches

- The combat engine implementation uses exactly the same `f(x)` formula and multiplicative composition for armor/shield/dodge mitigation.
- Ship-type coefficients for survey, battleship, explorer, and interceptor match the described values.

## Gaps and differences

1. **Optimizer path does not currently compute mitigation from defense/piercing stats.**
   - In optimizer Monte Carlo setup, defender mitigation is assigned from a synthetic hash-based scalar (`0.25..0.59`) instead of the formula above.

2. **No explicit `Armada` ship type in engine enum.**
   - The described behavior says armadas share survey coefficients. Current enum includes survey/battleship/explorer/interceptor only.

3. **Piercing clamp behavior uses epsilon, not exact zero.**
   - Current `component_mitigation` clamps piercing to `max(EPSILON, piercing)`, where `EPSILON = 1e-9`, instead of `max(0, piercing)`.
   - This avoids division by zero and is numerically very close to the implied limit behavior (`p -> 0+`) for nonzero defense.

## Practical impact

- **Core combat mitigation function:** very close to description, with one numerical-stability deviation (epsilon clamp).
- **Optimizer results:** currently not aligned with the described mitigation model because they use a pre-filled mitigation scalar rather than deriving mitigation from armor/deflection/dodge versus piercing/accuracy.
