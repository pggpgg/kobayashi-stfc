# Structured audit: hostile `ability[]` coverage

This document expands on [ROADMAP.md](ROADMAP.md) §6 — hostile-ability coverage audit.

**Catalog revision (2026-07-07):** There are **982** unique upstream hostile ability ids across **2,901** hostiles with non-empty `ability[]`. The regenerated [`hostile_ability_catalog.json`](../data/upstream/data-stfc-space/hostile_ability_catalog.json) classifies all ids: **261** modeled for defender-side counter-fire (`defender_crew`), **721** `combat_noop`. Regenerator: `python3 scripts/generate_full_hostile_ability_catalog.py`.

**Xindi (2026-06-16):** Fixed a PvP classifier false positive on NPC text (`enemy players ship` ≠ PvP `enemy player`). Modeled ability ids:

| Ability id | Hostiles | Primary effect | Notes |
| --- | ---: | --- | --- |
| `1271329828` | 45 | `hostile_crit_damage_reduction` + lethal `extra_seat` | Doomed Species (2R stack) + Xindi Weaponry particle beam (round-end lethal) |
| `1408273502` | 25 | `hostile_crit_damage_reduction` | Be Like Water; Xindi Might text = 9×20B weapon only |
| `141924765` | 14 | `hostile_crit_damage_reduction` + Denticle extra seat | Be Like Water + **Denticle Blade** (combat-start 30% proc gates weapon slot 5); Xindi Might = weapon only |
| `2665723295` | 6 | `hostile_lethal_end_of_round` (`round_interval: 8`) | No Mercy — assimilated prevents 100% |
| `3981152012` | 6 | `hostile_kemocite_weaponry` @ `round_end` | Kemocite — +30%/stack at round end; burning prevents 100% |

See `tests/xindi_hostile_abilities.rs` and [DESIGN.md](DESIGN.md) §3.6 for lethal/crit approximations.

**Be Like Water crit debuff:** upstream value `25` (−2500% UI) subtracts 25 percentage points from the player's outbound crit bonus (typical high-crit builds → ×1.0 before floor). **Critical Damage Floor** then clamps the post-debuff multiplier (`after_mult.max(crit_damage_floor)` in [`crit.rs`](../src/combat/crit.rs)), so outbound crits can still exceed ×1.0 base when floor research is high — e.g. Enterprise-D vs Aquatic Cruiser L51 fight sample crit/non-crit hull ~×1.61 is consistent with BLW collapse plus a floor near that value.

Descriptions are keyed by `translations-ship_buffs.json` (`key: ship_ability_desc`, `id` = per-row `loca_id` from `hostiles/*.json ability[]`).

---

## 1. Three modeling lanes

Hostile combat behavior is **not** a single pipeline. The audit tracks three lanes:

| Lane | Source | Runtime path | Catalog? |
| --- | --- | --- | --- |
| **A — Catalog → defender crew** | `HostileRecord.ability[]` | [`hostile_abilities_to_defender_crew`](../src/data/hostile_ability_resolve.rs) → [`scenario.rs`](../src/optimizer/monte_carlo/scenario.rs) `defender_crew` → counter-fire effect accumulator | Yes — this doc |
| **B — Tag-driven mechanics** | Curated `hostile_tags` on normalized records | [`conqueror_borg_beams.rs`](../src/combat/conqueror_borg_beams.rs), [`evolutionary_assimilation.rs`](../src/combat/evolutionary_assimilation.rs) | No — hardcoded in normalizer + engine |
| **C — Base stats / components** | Normalized hull/shield/weapon fields | [`defender_combatant_from_hostile_record`](../src/optimizer/monte_carlo/scenario.rs) | Out of scope for `ability[]` catalog |

**Trace naming:** `EventSource.hostile_ability_id` in combat traces (e.g. `{defender_id}_mitigation`) labels **mechanics already running** — it is **not** the upstream catalog id. Do not use trace ids for coverage accounting.

**Attacker ship abilities vs hostiles** (Crozier CDR, Track D2 debuffs) live in the **ship** catalog — see [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md).

### Lane B — Conqueror Borg (modeled outside catalog)

32 upstream hostile ids receive tags in [`normalize_hostiles_stfc_space.rs`](../src/bin/normalize_hostiles_stfc_space.rs) (`curated_hostile_tags_for_upstream`):

- `conqueror_borg_suppressor` → Quantum Resonance Beam instant loss vs non–Borg-Sphere attackers
- `conqueror_borg_obliterator` → Hyperthermic Resonance Beam (80% vs Borg Sphere hull)
- `conqueror_borg` + forbidden officers → Evolutionary Assimilation instant loss

Attacker-side **Quantum Nullification Pulse** is a **ship** hull ability (`2425475474`, `conqueror_borg_beam_suppression`), not a hostile catalog row.

Calibration fixtures: `tests/fixtures/recorded_fights/drift_conqueror_borg_*.json`.

---

## 2. Inventory

| Metric | Count |
| --- | ---: |
| Unique upstream ability ids | 982 |
| Modeled (`effect_type` ≠ `combat_noop`) | 261 |
| `combat_noop` (catalogued, inert in sim) | 721 |

**Modeled effect types (261 ids):**

| `effect_type` | Ids |
| --- | ---: |
| `isolytic_damage` | 88 |
| `isolytic_defense` | 82 |
| `apex_barrier` | 54 |
| `attack_multiplier` | 22 |
| `hostile_hyperthermic_decay` | 4 |
| `hostile_crit_damage_reduction` | 3 |
| `hostile_crit_damage_floor` | 2 |
| `shield_mitigation_bypass` | 2 |
| `hostile_isolytic_vulnerability` | 1 |
| `crit_chance` (+ `crit_damage` / `hostile_crit_damage_floor` extra seats) | 1 |
| `hostile_lethal_end_of_round` | 1 |
| `hostile_kemocite_weaponry` | 1 |

**Not yet modeled (high instance count, remain `combat_noop`):**

- PvP player targeting (4 ids, 422 instances): default PvE path is ship vs NPC hostile
- Armada scope (125 ids, 260 instances)
- Outpost scope (56 ids, 163 instances)
- `other_review` (453 ids): burning procs, extra shots, faction gates, etc.

Full regen-safe noop id list: run `python3 scripts/generate_full_hostile_ability_catalog.py` and filter `effect_type == combat_noop` in the catalog JSON.

---

## 3. Buckets (generator heuristics)

| Bucket | Unique ids | Hostile instances | Decision |
| --- | ---: | ---: | --- |
| Isolytic combat-start | 170 | 1,498 | **Modeled** — `combat_begin` + `isolytic_damage` / `isolytic_defense` |
| Apex barrier | 54 | 368 | **Modeled** — `combat_begin` + `apex_barrier` |
| Weapon damage conditional | 19 | 121 | **Partial** — `attack_multiplier` where text matches; hull-breach gates use `condition_defender_hull_breach` |
| Crit multi-stat | 1 | 325 | **Modeled** — Critical Training emits `crit_chance` plus `crit_damage` / `hostile_crit_damage_floor` extra seats |
| Crit damage floor | 2 | 273 | **Modeled** — Diverted Power emits `hostile_crit_damage_floor` |
| PvP enemy player | 4 | 422 | **Keep noop** on default ship-vs-hostile path |
| Armada | 125 | 260 | **Keep noop** — no armada scenario |
| Outpost | 56 | 163 | **Keep noop** — station/outpost scope |
| Defense stat review | 82 | 142 | **Keep noop** pending defender mitigation seat mapping |
| Economy | 1 | 30 | **Keep noop** |
| Other / review | 453 | 1,011 | **Shard triage** — extend generator or overrides per pattern |

---

## 4. Top 20 by hostile count

| Ability id | Hostiles | Bucket | Catalog `effect_type` | Text (plain snippet) |
| --- | ---: | --- | --- | --- |
| `2291206649` | 325 | crit_multi_stat_modeled | `crit_chance` + `crit_damage` / `hostile_crit_damage_floor` extra seats | Critical Training — crit chance + damage + floor at combat start |
| `849650945` | 194 | pvp_player_target | `combat_noop` | Deadlock — hull breach enemy player |
| `910140799` | 194 | pvp_player_target | `combat_noop` | Dismantlement — weapon damage if enemy player hull breached |
| `2486538514` | 162 | crit_floor_modeled | `hostile_crit_damage_floor` | Diverted Power — crit damage floor |
| `788454016` | 111 | crit_floor_modeled | `hostile_crit_damage_floor` | Diverted Power — crit damage floor |
| `3172395625` | 90 | isolytic_combat | `isolytic_damage` | Elite Assassin Training — isolytic at combat start |
| `2747222231` | 82 | outpost_scope | `combat_noop` | Diverted Power (outpost) |
| `1782396999` | 69 | apex_combat | `apex_barrier` | Not So Wounded — apex barrier |
| `3257135627` | 69 | isolytic_combat | `isolytic_damage` | Augmented Force — isolytic at combat start |
| `390948510` | 53 | other_review | `combat_noop` | Ruthless Pursuit — crit chance first N rounds |
| `658066283` | 53 | isolytic_combat | `isolytic_damage` | Isolytic Vulnerability |
| `986116981` | 53 | other_review | `combat_noop` | Persistence Hunter — burning at combat start |
| `1745201100` | 53 | isolytic_combat | `isolytic_damage` | Isolytic Maul |
| `1271329828` | 45 | xindi_crit_debuff | `hostile_crit_damage_reduction` + lethal extra seat | Doomed Species + Xindi Weaponry particle beam |
| `141924765` | 14 | xindi_crit_debuff | `hostile_crit_damage_reduction` + Denticle extra seat | Be Like Water + Denticle Blade (30% proc gates weapon slot 5) |
| `3445799437` | 45 | hostile_shield_bypass | `shield_mitigation_bypass` | Blade's Tip — 100% bypass of player shield mitigation on counter |
| `2936293636` | 44 | isolytic_combat | `isolytic_defense` | Programmable Matter — reduces final damage (review mapping) |
| `3196612078` | 39 | hostile_shield_bypass | `shield_mitigation_bypass` | Strength of the Ibix — 100% bypass (10 shots are weapon components, not this seat) |
| `1088929105` | 30 | other_review | `combat_noop` | S31 Elite — faction ship gate |
| `1539285779` | 30 | armada_scope | `combat_noop` | Armada isolytic defense |
| `1651219904` | 30 | other_review | `combat_noop` | Mo'Kai Elite — faction ship gate |

---

## 5. Drift control (regeneration)

1. Run from repo root: `python3 scripts/generate_full_hostile_ability_catalog.py`
2. **Overrides:** After heuristics, merges [`hostile_ability_catalog_overrides.json`](../data/upstream/data-stfc-space/hostile_ability_catalog_overrides.json) (`entries`: ability id → full catalog row).
3. **Audit metadata:** `hostile_ability_audit_meta.json` (bucket + hostile counts per id; not consumed at runtime).
4. **Diff:** Compare regenerated catalog to previous commit; port intentional deltas into the Python classifier or overrides file. The nine hand-maintained aggregation/offense/isolytic rows currently live in the overrides file so regeneration is idempotent.
5. **Parity test:** `cargo test --test hostile_ability_catalog_parity` — catalog keys must cover every upstream ability id.

---

## 6. Maintenance

When upstream hostiles refresh:

- Re-run the generator after `fetch_stfcspace_hostiles.mjs` / normalize.
- New combat-relevant patterns → extend `classify_hostile_ability` in the generator; one-offs → overrides JSON.
- Extend [`hostile_ability_effect_from_catalog`](../src/data/hostile_ability_resolve.rs) when adding new `effect_type` values (prefer delegating to ship resolver for shared effects).
- Label approximations in [DESIGN.md](DESIGN.md) §3.6 (Hostile hull abilities).

**Uncertainty:** Only the first `values[]` entry is used (same as ships). Per-level ability curves are not modeled. Percentage semantics follow catalog `value_is_percentage` + `ignore_upstream_value_is_percentage` — verify with overrides when upstream marks fractional values as percentage (Track D2 lesson).
