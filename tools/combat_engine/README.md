# Combat Engine (Phase 1.1)

## Mitigation model

This module implements the first combat-engine task from `IMPLEMENTATION_PLAN_COMBAT_ENGINE.md`.

### Formula

- Component function:
  - `f(x) = 1 / (1 + 4^(1.1 - x))`
  - `x = defense / piercing`
- Total mitigation:
  - `1 - (1 - cA*fA) * (1 - cS*fS) * (1 - cD*fD)`

### Ship-type coefficients

- Survey: `[0.3, 0.3, 0.3]`
- Battleship: `[0.55, 0.2, 0.2]`
- Explorer: `[0.2, 0.55, 0.2]`
- Interceptor: `[0.2, 0.2, 0.55]`

### Assumptions

- Defense and piercing values are treated as non-negative inputs.
- Non-positive piercing is clamped to `EPSILON=1e-9` to keep deterministic finite math.
- Final mitigation is clamped to `[0.0, 1.0]`.

### Tolerance thresholds

- Golden vectors are asserted with <= `0.1%` relative tolerance (`pytest.approx(..., rel=1e-3)`).

### Dev CLI

```bash
python tools/combat_engine/mitigation_cli.py \
  --ship-type battleship \
  --armor 250 --shield-deflection 120 --dodge 50 \
  --armor-piercing 100 --shield-piercing 60 --accuracy 200
```
