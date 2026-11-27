# Native critical damage units

In live STFC server combat, native `Attack.CritModifier` / `CriticalDamage` is
already the **final critical-hit damage factor**, not a bonus added on top of
`1.0`. A native value of `1.5` therefore produces a `1.5×` critical hit, and
additive critical-damage buffs (`mod=10, op=1 Add`) or reductions (`op=2 Sub`)
add or subtract percentage points directly from that total factor down to `0.0`
before `crit_damage_floor` clamps upward.

## Live combat evidence (2026-11-26)

An earlier version of this document inferred `1 + native_value` solely from the
client ThreatConfig UI `dpr` display equation
(`base + base * CriticalChance * CriticalDamage`, see
[`client_threat_formulas_2026-10-20.json`](../data/client_threat_formulas_2026-10-20.json)),
which omits `-1` in the UI strength preview. Live battle logs across 63 hostile
critical hits in 26 fights and four `CritModifier` regimes establish that server
combat evaluates `CritModifier` directly as the final multiplier:

| Enemy `CritModifier` | Player CDR (`add`, `mult`) | Final-factor formula `max(0, eCD - add) * (1 - mult)` | Observed live server |
| --- | --- | --- | --- |
| `1.50` + Hull Breach (`NS HB CDR -2.0`) | `-2.60`, `×0.95` | `0.000×` | `0.000×` (17/17 zero-damage crits) |
| `1.50` (Hijacked D7 / D3) | `-0.60`, `×0.95` | `0.855×` | `0.856×` (13/13 Reliant crits) |
| `1.80` (Scouting Bandit / Takret) | `-0.60`, `×0.95` | `1.140×` | `1.153×` (9/9 Vi'dar & Realta crits) |
| `2.00` (L40 Exchange Transport, repo fixtures) | `-0.60`, `×0.95` | `1.330×` | `1.329×` (24/24 crits in `realta_exchange_*_observed.json`) |

The same final-factor convention matches live player outgoing critical ratios
when combined with `Court of Q` (`bid 1768821190`, `+1.90` vs non-Armada
hostiles) and `Section 31 Critical Hit Damage` (`bid 2900801564`, `+0.25` R1–2):
Reliant Stonn solo `8.28×` vs `8.15×`, Reliant Morale R1–2 `9.08×` vs `9.16×`,
and Reliant Morale R3–6 `8.83×` vs `8.95×`. It is also corroborated by
`Critical Damage Floor` research levels 1–4 (`0.20, 0.40, 0.60, 0.80 < 1.0`).

## Compatibility

Ship, extended-ship, and hostile catalogs retain the `crit_damage_is_bonus`
metadata field and
[`native_critical_damage_sources.json`](../data/native_critical_damage_sources.json)
manifest for provenance and backwards compatibility, while
`critical_damage_to_multiplier` returns the final factor directly without adding
`+1.0`.
