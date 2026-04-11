# Galaxy-class damage: sim vs log and additive stacking (findings)

Standalone notes for the Ent-D / V’ger Hurak fight sample and the **layered vs single-pool weapon damage** hypothesis. This document is **not** merged into `DESIGN.md`.

## Source log

- File: `fight samples/uss enterprise d vs vger hurak 59.csv` (tab-separated).
- Parsed fixture: `tests/fixtures/galaxy_ent_d_hurak59_log_outgoing.json`.
- **Outgoing damage (player):** sum of CSV column `Total Damage` for rows with `Type=Attack`, `Attacker Name=HiggsBozo`, and `Attacker Ship` containing `U.S.S. ENTERPRISE-D`.
- **Fight-total outgoing damage (log):** `19_243_298_941` over **23** combat rounds (per log `Round` column).
- **Morale (“To the Journey!” Harry Kim)** procs on rounds: `1,2,3,4,5,6,7,8,10,11,13,14,15,17,19,22` (16 rounds) — parsed from `Ability Name` in the CSV.
- **Hostile match in data:** index id **`518459749`** has `hull_health` **14_836_533_168**, matching the log’s enemy hull row (V’ger Hurak Honorguard). Display names in `data/hostiles/` remain generic (`Hostile 518459749`).

### Crew caveat

The CSV summary row lists captain/bridge as **Annorax, Suder, Seska**; battle events also show **B’Elanna Torres** (“Knock it Down”) and **Harry Kim** below decks. The Kobayashi calibration test uses **Annorax + Suder + Seska + Harry Kim (T4) BD** only (no B’Elanna), so sim is not a line-for-line reproduction of every log line.

## What Kobayashi does today (relevant pieces)

1. **Profile / research / buildings / FT** merge into `PlayerProfile` bonuses (`merge_research_bonuses_into_profile`, etc.). See [`build_shared_scenario_data_from_registry`](src/optimizer/monte_carlo/scenario.rs).
2. **Ship weapons and scalar attack** are scaled by **`apply_profile_to_attacker`**: `attack *= (1 + bonuses["weapon_damage"])` (and the same multiplier on per-weapon rows). See [`apply_profile_to_attacker`](src/data/profile.rs).
3. **LCARS static `weapon_damage`** is merged into `static_buffs` and applied via **`apply_static_buffs_to_combatant`** (`attack *= weapon_mult` with key semantics described in that function — separate from the profile layer).
4. **Per shot (player outbound):** [`engine.rs`](src/combat/engine.rs) combines post-profile `weapon_attack`, `pre_attack_multiplier = 1 + pre_attack_modifier_sum` from [`EffectAccumulator`](src/combat/effect_accumulator.rs), and (when present) Galaxy cumulative `g` with profile `p`: **default** path `weapon_attack * pre_mult * (1 + g/(1+p))`; **experimental additive pool** path `weapon_attack * (p + pre_mult + g)/(1+p)` when `KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL` is set.
5. **Galaxy hull ability (`448699234`)** is `additive_weapon_damage_growth` while **Morale is active**: each round contributes cumulative growth `g = round_index * growth_per_round` (capped) into a separate **`galaxy_additive_weapon_frac`** field, and the engine applies **`×(1 + g/(1+p))`** where `p` is the merged profile `weapon_damage` fraction (`SimulationConfig::profile_weapon_damage_fraction`). Other ships still use `accumulating_attack_multiplier` for classic cumulative pre-attack stacking (see [`ship_ability_effect_from_catalog`](src/data/ship_ability_resolve.rs)).

So the default model is **layered** for profile vs most dynamic pre-attack bonuses: base weapon stats already include `(1 + p_profile)` from research, then officer-style effects multiply by `(1 + sum_dynamic)`. **Enterprise-D Galaxy hull growth is an explicit exception:** it uses `×(1+g/(1+p))` instead of folding `g` into `sum_dynamic`.

If the game instead uses **one additive pool** for all “+% weapon damage” style bonuses, total factor on the pre-bonus weapon base would look like `(1 + p_profile + sum_dynamic)` instead of `(1 + p_profile) * (1 + sum_dynamic)`. For positive `p` and `sum`, the product is **strictly larger** than the single-pool sum — the sim can **overstate** damage when both layers are large (community “+100% does little on top of +3000%” comment refers to that dilution in **one** pool).

## Log vs sim (current build)

Integration test: [`tests/galaxy_ent_d_vs_hurak59_log_calibration.rs`](../tests/galaxy_ent_d_vs_hurak59_log_calibration.rs).

- **Ship:** `uss_enterprise_d`, tier **5**, level **18** (from fixture / log ship level).
- **Hostile:** `518459749`.
- **Profile:** `demo` (merges `profiles/demo/research.imported.json` and other imports — **not** an empty-bonus profile in practice).
- **Officer source:** `KOBAYASHI_OFFICER_SOURCE=lcars` for Harry Kim morale from YAML.

Example output from one run:

- `total_damage` (sim, trace-aggregated fight total) ≈ **0.63×** the log’s summed outgoing `Total Damage` for the same row filter — same order of magnitude, **not** bit-for-bit match (different crew line-up vs log, different mitigation RNG, demo profile vs HiggsBozo live account).

The test only asserts a **wide sanity bracket** on `sim/log` until a full profile + roster import matches the fight.

## Experimental: single additive pool for profile `weapon_damage`

**Env:** `KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL=1` (or `true`).

**Behavior:** [`weapon_damage_profile_additive_pool_from_env`](src/optimizer/monte_carlo/scenario.rs) sets `CombatSimulationInput::weapon_damage_profile_additive_pool` to `Some(profile weapon_damage bonus)` when the env flag is on. The combat engine applies:

```text
effective_attack = weapon_attack * (p + pre_attack_multiplier + g_galaxy) / (1 + p)
```

where `p` is the merged profile `weapon_damage` bonus (same units as `apply_profile_to_attacker`: additive fraction, e.g. `0.65` = +65%) and `g_galaxy` is the round’s cumulative Galaxy hull growth (same units). Algebraically this matches **one pool** `(1 + p + sum + g)` on the pre-profile base together with additive-pooled officers.

When the env flag is **off**, Galaxy still uses the dilution factor: `effective_attack = weapon_attack * pre_attack_multiplier * (1 + g/(1+p))` (default path).

**Counter-attack (hostile) path** is unchanged.

**Uncertainty:** Only **profile** `weapon_damage` is folded into this experimental pool; **static LCARS `weapon_damage`** still uses `apply_static_buffs_to_combatant` as today. Fully matching an unknown client ordering would require more evidence.

## LCARS vs ship catalog: `AttackMultiplier` payload

- **Ship catalog / hull resolver:** `attack_multiplier` → `AbilityEffect::AttackMultiplier(value)` where `value` is a **delta** added to `pre_attack_modifier_sum` (see [`ship_ability_effect_from_catalog`](src/data/ship_ability_resolve.rs)).
- **LCARS `stat_modify` default branch** (`weapon_damage` / `attack`, non-decay, non-accumulate): [`resolve_effect`](src/lcars/resolver.rs) sets `mult = 1.0 + value` for the default `add`-style operator and stores `AttackMultiplier(mult)`.

The engine’s `trace_add_pre_mod` **adds the numeric payload verbatim** to `pre_attack_modifier_sum`. So **ship rows** (delta) and **LCARS rows** (often `1+v` factor) are **not** guaranteed to use the same convention. This is a **known inconsistency** to resolve separately (tests in `resolver.rs` intentionally use operators like `sub` that produce factors such as `0.8`). Any future fix should normalize LCARS dynamic weapon_damage to **deltas** before touching `pre_attack_modifier_sum`, and add regression tests.

## Open questions / next steps

1. Import **HiggsBozo** (or fight-day) profile + roster into a reproducible fixture and tighten `sim/log` tolerance.
2. Decide whether **static** `weapon_damage` should enter the same additive pool as profile + dynamic pre-attack.
3. Validate Galaxy **growth** and **morale gating** against more fight samples (round indexing is 1-based in engine and trace; see [`tests/galaxy_ent_d_round_damage_sanity.rs`](../tests/galaxy_ent_d_round_damage_sanity.rs) header comment).
4. Resolve LCARS `AttackMultiplier` vs ship-catalog **delta** convention with a single documented rule in code + tests.

## Related code pointers

| Topic | Location |
|--------|-----------|
| Layered profile attack | [`apply_profile_to_attacker`](src/data/profile.rs) |
| Scenario merge + env flag | [`weapon_damage_profile_additive_pool_from_env`](src/optimizer/monte_carlo/scenario.rs) |
| Outbound effective attack | [`engine.rs`](src/combat/engine.rs) (`weapon_damage_profile_additive_pool` branch) |
| `pre_attack_modifier_sum` | [`effect_accumulator.rs`](src/combat/effect_accumulator.rs) |
| Galaxy catalog / resolver | [`ship_ability_catalog.json`](../data/upstream/data-stfc-space/ship_ability_catalog.json), [`ship_ability_resolve.rs`](src/data/ship_ability_resolve.rs) |
