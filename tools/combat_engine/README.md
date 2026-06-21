# Combat Engine (Python reference)

Small **Python** package for the same core combat math as the Rust simulator, useful for quick experiments, notebooks, and **CI parity checks**. The production engine is Rust (`src/combat/`); this tree stays aligned with it.

**Repository entrypoint:** see the root [README.md](../../README.md) for build, verify, and where this package fits for contributors.

## Rust parity (keep in sync)

When changing formulas here, update the matching Rust sources and vice versa. Golden tests cross-reference both sides:

| Python | Rust |
| --- | --- |
| `component_mitigation`, `mitigation`, `mitigation_with_mystery`, `mitigation_for_hostile` | [`src/combat/mitigation.rs`](../../src/combat/mitigation.rs) |
| `pierce_damage_through_bonus`, `PIERCE_CAP` | [`pierce_damage_through_bonus`](../../src/combat/mitigation.rs), `PIERCE_CAP` |
| `apex_barrier_damage_factor` | [`compute_apex_damage_factor`](../../src/combat/damage.rs) |
| `isolytic_mitigation` | isolytic defense term used with [`isolytic_damage`](../../src/combat/mitigation.rs) in the engine |

**Locked vectors:** `tests/test_mitigation.py::test_mitigation_matches_rust_golden_reference_vectors` uses the same stats and expected floats as `golden_values_match_python_reference_for_each_ship_type` in [`tests/combat_tests.rs`](../../tests/combat_tests.rs). If you change either, update the other.

## Mitigation model

This module implements the mitigation model documented in [DESIGN.md](../../docs/DESIGN.md) (combat math), mirrored by the Rust engine in [`src/combat/mitigation.rs`](../../src/combat/mitigation.rs).

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

- Golden vectors are asserted with <= `0.1%` relative tolerance (`pytest.approx(..., rel=1e-3)`) where noted; Rust-reference vectors use `rel=1e-12`.

### Tests

From the repository root (after `pip install -r tools/combat_engine/requirements-test.txt`):

```bash
python -m pytest tools/combat_engine/tests/ -v
```

These tests also run on every push/PR in GitHub Actions (`combat_engine_python` job in `.github/workflows/ci.yml`). Root `npm run verify` includes the same pytest invocation.

### Dev CLI

```bash
python tools/combat_engine/mitigation_cli.py \
  --ship-type battleship \
  --armor 250 --shield-deflection 120 --dodge 50 \
  --armor-piercing 100 --shield-piercing 60 --accuracy 200
```
