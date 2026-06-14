# Research conditional routing

How gated research catalog rows reach combat. Companion inventory: [research_conditional_inventory.md](research_conditional_inventory.md) (regenerate with `node scripts/inventory_research_conditional.mjs --markdown`).

**Code:** `src/data/research.rs`, `src/data/research_effect_spec_adapter.rs`, `src/data/profile.rs`, `src/optimizer/monte_carlo/scenario.rs`.

---

## Merge paths (summary)

| Path | When | Applied where |
|------|------|----------------|
| **Flat `profile.bonuses`** | Unconditional combat stats | Scenario attacker base stats |
| **Owner-faction map** | `attacker_faction` / `attacker_factions` only (no defender gate on hull/shield dual rows) | When ship faction matches |
| **Attack-phase / round-start seats** | Conditional weapon damage, crit, isolytic, defender-faction mitigation stats, conditional `hull_hp` / `shield_hp` (morale / burning / HB / ship class), etc. | `research_derived_attack_phase_seats` → fight loop |
| **Dual-gate hull/shield** | Owner faction **and** `defender_faction`, stat `hull_hp` or `shield_hp`, **no** extra gates (morale / burning / HB / ship class) | `cumulative_dual_gate_hull_shield_research_fractions` at scenario build |
| **Canonical override** | `data/research_canonical.json` entry for synced `rid` | Replaces catalog compile for that project; KSG incoming SM handled separately |
| **Incoming shield mitigation** | Canonical effect with `incoming_shield_mitigation_rounds` | `SimulationConfig` rounds 1..=N only (counter-fire / incoming damage) |

Levels **1..=synced level** are cumulative unless `snapshot_by_level: true` on a canonical effect (tier total only — used today for KSG rid `2392190200`).

---

## Dual-gate hull/shield (faction-only)

Implemented in `cumulative_dual_gate_hull_shield_research_fractions` (`research.rs`):

1. Row must be owner-faction gated **and** have `defender_faction`.
2. Stat is `hull_hp` or `shield_hp`.
3. Row must **not** also require morale, burning, hull breach, or defender ship class (`dual_gate_hull_shield_scenario_apply_condition`).
4. At scenario build, owner slug must match the player ship faction and defender slug must match the hostile/PvP opponent faction tag.

These rows are **skipped** from:

- Flat `profile.bonuses` merge
- `research_owner_faction_bonuses` (would incorrectly apply vs all hostiles)

**Not covered by scenario dual-gate:** rows that also need morale, burning, hull breach, or defender ship class compile to **conditional max-HP seats** (`HullHpMultiplier` / `ShieldHpMultiplier` in `src/combat/abilities.rs`), applied once per round in the fight loop when gates pass.

**Catalog audit (2026-06-14):** upstream has **no** `hull_hp`/`shield_hp` projects with owner+defender faction only. Owner-faction hull/shield lines (Graviton Shields, Resolve, etc.) correctly map with `attacker_faction` only. Cross-faction **weapon_damage** rows were corrected (`982655355` Romulan vs Federation, `2982312380` Federation vs Klingon, `4009387266` Klingon vs Romulan). Dual-gate hull/shield scenario path is covered by unit tests; add `buff_id_to_stat.json` rows when upstream ships them.

---

## Canonical overrides (flagship trees)

Eleven manual overrides in `data/research_canonical.json` cover high-investment NS / KSG trees:

| rid | Tree | Notes |
|----:|------|-------|
| 365419690 | NS Burning weapon damage | Burning gate; catalog duplicates but canonical wins |
| 2580836593 | NS Burning isolytic | |
| 2047743532 | NS Burning + HB isolytic | Dual gate (burning ∧ HB) |
| 4133019450 | NS Morale isolytic | `round_start` trigger |
| 535909811 | NS HB weapon damage | |
| 1995496344 | NS HB isolytic | |
| 1233598019 | NS HB isolytic cascade | |
| 3288570685 | NS Crit damage | Unconditional in canonical (flat merge OK) |
| 3407808029 | NS HB crit damage reduction | Hostile crit DR, HB gate |
| 851540444 | NS Morale shield mitigation | Morale gate |
| 2392190200 | KSG Shield mitigation | `snapshot_by_level` + incoming rounds 1–2 |

Canonical RIDs are excluded from the catalog fallback seat builder; effects compile from `by_level` (sum or snapshot per flag).

---

## Catalog seat timing quirks

From `research_effect_spec_adapter.rs`:

- **`isolytic_damage` + `requires_morale`** → `RoundStart` (morale must be active before isolytic leg).
- **`apex_barrier` + `requires_morale`** → `RoundStart` (matches officer ApexBarrier + Morale seats).
- **Other conditional isolytic** → `AttackPhase`.
- **`isolytic_cascade_damage`** → always `AttackPhase`.
- **Conditional weapon damage / crit** → `AttackPhase` seats; not flat `profile.bonuses`.
- **`hull_hp` / `shield_hp` + `requires_morale`** → `RoundStart` (`HullHpMultiplier` / `ShieldHpMultiplier`).
- **Other conditional `hull_hp` / `shield_hp`** (burning, HB, defender faction, ship class) → `AttackPhase`.

---

## Importer: percentage-point values

Upstream buffs with `value_is_percentage: true` and `show_percentage: true` store **percentage points** (1 = +1%). The generic branch kept values in `(0, 1.5]` as literal fractions (+100% for `1.0`).

Normalization lives in `scripts/lib/research_normalize_bonus_value.mjs` (used by `import_stfcspace_research.mjs`):

- `show_percentage && value ∈ [1, 100]` → divide by 100
- Otherwise legacy rules unchanged

---

## Tests

- `tests/research_profile_merge_tests.rs` — crit/burning/morale isolytic, burning+HB isolytic, canonical priority, KSG incoming SM
- `src/data/research.rs` unit tests — dual-gate hull/shield fractions, flat-merge skips
- `scripts/test/research_normalize_bonus_value.test.mjs` — percentage-point normalization
