# Research conditional routing

How gated research catalog rows reach combat. Companion inventory: [research_conditional_inventory.md](research_conditional_inventory.md) (regenerate with `node scripts/inventory_research_conditional.mjs --markdown`).

**Code:** `src/data/research.rs`, `src/data/research_effect_spec_adapter.rs`, `src/data/profile/mod.rs`, `src/optimizer/monte_carlo/scenario.rs`.

---

## Merge paths (summary)

| Path | When | Applied where |
|------|------|----------------|
| **Flat `profile.bonuses`** | Unconditional combat stats | Scenario attacker base stats |
| **Owner-faction map** | `attacker_faction` / `attacker_factions` only (no defender gate on hull/shield dual rows) | When ship faction matches |
| **Ship-grade base piercing** | `attacker_min_grade` only, stat `accuracy`, `armor_piercing`, or `shield_piercing` | After resolving ship grade, before aggregate and per-weapon hostile mitigation |
| **Attack-phase / round-start seats** | Conditional weapon damage, crit, isolytic, defender-faction mitigation stats, conditional `hull_hp` / `shield_hp` (morale / burning / HB / ship class), etc. | `research_derived_attack_phase_seats` → fight loop |
| **Verified defense and repair seats** | `base_mitigation_stats_bonus` or `officer_shield_regen_max_fraction`, with their explicit status gate | Self-ship `RoundStart`; never flat profile or owner-faction maps |
| **Dual-gate hull/shield** | Owner faction **and** `defender_faction`, stat `hull_hp` or `shield_hp`, **no** extra gates (morale / burning / HB / ship class / grade) | `cumulative_dual_gate_hull_shield_research_fractions` at scenario build |
| **Canonical override** | `data/research_canonical.json` entry for synced `rid` | Replaces catalog compile for that project; KSG incoming SM handled separately |
| **Incoming shield mitigation** | Canonical effect with `incoming_shield_mitigation_rounds` | `SimulationConfig` rounds 1..=N only (counter-fire / incoming damage) |

Catalog records declare `level_value_mode`. Imported upstream records use `"snapshot"`: only the owned rank's complete bonus applies. Authored records that omit the field retain `"cumulative"` behavior: levels **1..=synced level** compose in ascending order. `research_levels_to_apply` provides the same selection for flat, owner-faction, conditional, and dual-faction paths. Level zero contributes nothing; levels above the catalog maximum use the maximum rank. Empty or missing snapshot ranks contribute nothing and do not reuse an earlier rank.

The importer emits ranks through the declared project maximum or project level count. Each nonempty upstream buff array clamps independently to its final entry when shorter than that project bound; empty arrays contribute nothing. Encoded trailing zeroes remain zero. Without a declared project bound, the longest available array supplies the rank count. Short or empty unrelated buffs do not truncate a project's ranks.

Canonical effects retain their independent `snapshot_by_level` flag: `false` (including an omitted field) sums their authored `by_level` entries, while `true` selects one rank. All ten bundled combat effects explicitly use snapshots; the eleventh override intentionally has no effects because its source buff is deployment-only. Changing a catalog record's mode does not change its canonical override.

## Ship-grade base piercing

Research `2999437` (4-star Direct Hit), buff `1994654654`, maps to three separate base-stat fractions with `attacker_min_grade: 4`. Rank 2 contributes `0.24` to each channel; it does not sum ranks 1 and 2 to `0.45`. Ship `grade` is a literal number preserved from upstream through `ExtendedShipRecord` and resolved `ShipRecord`.

`combat_research_ship_grade_bonuses_from_import` admits these grade-only rows when the selected ship has a known grade at least as high as the requirement. Grade 3 and unknown grade contribute nothing. The scenario applies the resulting fractions to base accuracy, armor piercing, and shield piercing before mitigation, including per-weapon stats. These keys do not use the generic damage-through `pierce` channel.

Every grade-gated row is excluded from the ordinary flat, owner-only, and runtime-seat paths. The current grade merge supports only the three stats above with no additional gates; other stats or compound grade conditions remain unapplied. Research summaries use the same restriction and include the resolved `ship_grade` when a scenario is supplied.

---

## Dual-gate hull/shield (faction-only)

Implemented in `cumulative_dual_gate_hull_shield_research_fractions` (`research.rs`):

1. Row must be owner-faction gated **and** have `defender_faction`.
2. Stat is `hull_hp` or `shield_hp`.
3. Row must **not** also require morale, burning, hull breach, defender ship class, or minimum attacker grade (`dual_gate_hull_shield_scenario_apply_condition`).
4. At scenario build, owner slug must match the player ship faction and defender slug must match the hostile/PvP opponent faction tag.

These rows are **skipped** from:

- Flat `profile.bonuses` merge
- `research_owner_faction_bonuses` (would incorrectly apply vs all hostiles)

**Not covered by scenario dual-gate:** rows that also need morale, burning, hull breach, or defender ship class compile to **conditional max-HP seats** (`HullHpMultiplier` / `ShieldHpMultiplier` in `src/combat/abilities.rs`), applied once per round in the fight loop when gates pass. A minimum-grade gate also excludes this path; grade-gated hull/shield rows are currently unsupported and do not become seats.

**Catalog audit (2026-06-14):** upstream has **no** `hull_hp`/`shield_hp` projects with owner+defender faction only. Owner-faction hull/shield lines (Graviton Shields, Resolve, etc.) correctly map with `attacker_faction` only. Cross-faction **weapon_damage** rows were corrected (`982655355` Romulan vs Federation, `2982312380` Federation vs Klingon, `4009387266` Klingon vs Romulan). Dual-gate hull/shield scenario path is covered by unit tests; add `buff_id_to_stat.json` rows when upstream ships them.

---

## Canonical overrides (flagship trees)

Eleven manual overrides are stored in `data/research_canonical.json`. The table describes their current modeled effects after the selected client-data verification below.

| rid | Tree | Notes |
|----:|------|-------|
| 365419690 | NS Burning weapon damage | Burning gate; catalog duplicates but canonical wins |
| 2580836593 | NS Burning isolytic | |
| 2047743532 | NS Burning mitigation | Base armor/deflection/dodge bonus; opponent Burning; round start |
| 4133019450 | NS Morale isolytic | `round_start` trigger |
| 535909811 | NS Morale mitigation | Base armor/deflection/dodge bonus; self Morale; round start |
| 1995496344 | NS HB isolytic | |
| 1233598019 | NS HB mitigation | Base armor/deflection/dodge bonus; opponent Hull Breach; round start |
| 3288570685 | NS Morale SHP | Repair fraction of maximum shield HP; self Morale; round start |
| 3407808029 | NS HB crit damage reduction | Hostile crit DR, HB gate |
| 851540444 | Forge Reward Claim Unlock | Empty combat override; source is launch/deployment CargoProtection |
| 2392190200 | KSG Shield mitigation | `snapshot_by_level` + incoming rounds 1–2 |

Canonical RIDs are excluded from the catalog fallback seat builder; effects compile from `by_level` (sum or snapshot per flag).

### Canonical source audit (2026-10-20)

At the initial rank audit, every bundled override's `source_ref.rid`, `buff_id`, and `loca_id` resolved to its checked-in `data/upstream/data-stfc-space/research/<rid>.json` entry. Seven `by_level` arrays matched raw ranked values and four matched every raw rank multiplied by `0.01`. This established snapshot representation, but did not validate those unit conversions or the modeled modifiers. Burning Isolytic rank 2 was corrected to select `0.06`, rather than `0.05 + 0.06`.

That initial audit exposed these mismatches with `research_project_description` in `translations-research.json`. The “previous effect” column is historical; the client-verified replacements are described below.

| RID | Buff ID | Loca ID | Previous canonical effect | Source description |
|---:|---:|---:|---|---|
| 2047743532 | 679782642 | 70103 | Isolytic damage with Burning + Hull Breach | “Increases all base Mitigation stats while your opponent has Burning.” |
| 535909811 | 1142346453 | 70104 | Weapon damage with Hull Breach | “Increases all base Mitigation stats while your ship has Morale.” |
| 1233598019 | 4033066544 | 70105 | Isolytic cascade with Hull Breach | “Increases all base Mitigation stats while your opponent has Hull Breach.” |
| 3288570685 | 2286920377 | 70107 | Unconditional critical damage | “Increases SHP repair for every round while your ship has Morale.” |
| 851540444 | 3573127943 | 80007 | Shield mitigation with Morale | “Unlocks One Time Rewards Claims when the Transogen Forge Building is the required level.” |

### Client-verified corrections

The selective fixture [client_verified_research_2026-10-20.json](../data/research/client_verified_research_2026-10-20.json) records only those five static projects, their linked buff definitions, target/trigger records, raw rank values, byte offsets and hashes. It contains no player-state payloads. A fresh cache captured after login at `2026-10-20T18:04:30.399Z`, while client build `272` was observed, contains byte-identical payloads for all five projects, five complete buffs, and three target/trigger records. Both cache envelopes omit server version and timestamp: the fresh capture establishes local co-observation, not an encoded server/build binding.

The three mitigation buffs have modifier code `73` (`ModAllDefenses`), operation `3` (`MultiplyAdd`), self-ship target `1`, and round-start trigger `18`. Their single condition codes are `38` (opponent Burning), `33` (self Morale), and `37` (opponent Hull Breach), respectively. The retained client `BuffCondition..cctor` at `0x181b71610` confirms those condition assignments. All three specify one-round effects and raw rank fractions `0.5, 0.75, 1.15, 1.6, 2.05, 2.6, 3.15, 3.75, 4.35, 5`. They now use `base_mitigation_stats_bonus`, with no division by 100 and no direct addition to final mitigation probability.

Native `StatDataUtilities.GetStatTypes` at `0x181e9b9e0` maps modifier `73` to Dodge, Plating, and Absorption. `ApplySingleBuffToStatContainer` at `0x1817fe0c0` adds the unchanged ranked double to the MULT bucket for operation `3`; `ApplyBuffModifiersToValue` at `0x1817c2450` uses `(input + AdditionVal) * (1 + MultVal)`. This supports multiplying the base defense components by the raw fractional bonus, rather than interpreting the display flag as a percentage conversion.

The runtime uses the raw-stat mitigation curve for incoming fire whenever nonzero raw defense context is present, whether or not a research status is active. A qualifying research effect adds its fraction to each base defense multiplier for the round; it does not switch the baseline mitigation model. Missing or all-zero raw defense context retains the legacy fallback.

The shield-repair buff has modifier `16` (`ModShieldHpRepair`), the same self-ship round-start target/trigger and operation, condition `33`, and rank fractions `0.01` through `0.1`. It now uses the simulator's existing `officer_shield_regen_max_fraction` channel with Morale. The inspected `GetStatTypes` method does not map modifier `16`; the authoritative server repair denominator and ordering remain unverified. The implemented channel restores a fraction of effective maximum shield HP. The runtime applies these status predicates at round start; an effect is not activated retroactively by a status acquired later in that round.

The remaining buff is modifier `67` (`ModCargoProtection`), operation `1` (`Add`), with trigger `24` (`trig_self_launch`) and schema `2` (`FleetDeploymentSchema`). It contributes no battle shield mitigation. Its empty canonical override blocks stale catalog fallback, and an explicit importer exclusion prevents reintroducing it through prose inference.

The four combat mappings use `value_unit: "fraction"` to bypass magnitude-based percentage heuristics. Both canonical effects and regenerated catalog fallback produce the same raw values, status gates, and round-start timing. Generic authored override defaults remain unchanged.

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
- **`base_mitigation_stats_bonus` / `officer_shield_regen_max_fraction`** → `RoundStart`, preserving their explicit conditions.

---

## Importer: percentage-point values

The importer currently interprets upstream buffs with `value_is_percentage: true`, `show_percentage: true`, and values in `[1, 100]` as **percentage points** (1 = +1%). Values below 1, including Direct Hit's `0.21` and `0.24`, retain their fractional units. These normalization rules are separate from snapshot rank selection.

Normalization lives in `scripts/lib/research_normalize_bonus_value.mjs` (used by `import_stfcspace_research.mjs`):

- `show_percentage && value ∈ [1, 100]` → divide by 100
- Otherwise legacy rules unchanged
- Explicit client-verified `value_unit: "fraction"` mappings bypass those heuristics and preserve finite raw values.

---

## Tests

- `tests/research_profile_merge_tests.rs` — supplied rank selection, duplicate-rid maximum, canonical exclusion and independent override modes, canonical/fallback defense and repair parity, noncombat exclusion, KSG incoming SM
- `src/data/research.rs` unit tests — zero/first/second/maximum/above-maximum ranks, empty/missing snapshots, legacy cumulative defaults, faction and grade routing
- `src/optimizer/monte_carlo/scenario.rs` tests — Direct Hit grade 3/4/5/unknown and aggregate/per-weapon base piercing
- `scripts/test/research_catalog_snapshots.test.mjs` — source-backed Direct Hit mapping, retained empty ranks, invalid grade mapping rejection, mixed buff lengths, and all eleven canonical arrays against source IDs and rank values
- `scripts/test/research_normalize_bonus_value.test.mjs` — percentage-point normalization
