# Structured audit: `combat_noop` ship abilities

This document expands on [ROADMAP.md](ROADMAP.md) § Ship Abilities — audit `combat_noop`.

**Catalog revision (2026-05-19, Track D + D2):** There are **140** upstream ability ids in `data/upstream/data-stfc-space/ship_ability_catalog.json`. **67** map to `effect_type: combat_noop` (inventory-only in combat). **73** are modeled for the sim (timing + effect resolved in `src/data/ship_ability_resolve.rs` and related combat code). Opponent hull-class gates (`condition_opponent_ship_class`) are evaluated against the hostile’s `ship_class` in [`CombatContext::defender_ship_type`](../src/combat/abilities.rs).

**Track E update (2026-06-07):** The two breach-gated cumulative crit proc chains — Hegh'ta "Open the Wound" (`3432906971`) and Rotarran "Bird of Prey" (`2195955652`) — are now **modeled** (see §6.5), dropping the noop inventory from **67 → 65**. An earlier revision of §6.2 mislabeled these proc chains with the ids `2520552521` / `3014221215`; those two ids are in fact economy abilities (transogen loot / tritanium mining) and remain `combat_noop`.

**Inventory drift vs prior audits:** Six ids left the noop list: `509252162` (`attack_multiplier`), `2425475474` (`conqueror_borg_beam_suppression`), and Track D2 `701705952`, `1379978713`, `2441576367`, `1463338054` (see §6.1). Shard detail: [docs/audit_shards/](audit_shards/).

Descriptions are keyed by `translations-ship_buffs.json` (`key: ship_ability_desc`, `id` = per-row or ship `loca_id` from `ships/*.json`).

---

## 1. Inventory

All `combat_noop` ability ids (sorted; regen-safe; **65** ids):

`34867572`, `49906243`, `78080222`, `87414807`, `108924704`, `293385368`, `546190599`, `673187302`, `711428193`, `732090900`, `835292335`, `915894112`, `953555085`, `957303751`, `974800413`, `987222969`, `1004533782`, `1027217748`, `1029262994`, `1087128295`, `1090374551`, `1160666017`, `1244824002`, `1307832955`, `1428543762`, `1439253182`, `1492898704`, `1535317053`, `1577508895`, `1738424547`, `1784814733`, `1823660918`, `1839370465`, `1878809713`, `1972093910`, `1982797639`, `2004925834`, `2057434885`, `2254702328`, `2302150828`, `2468986074`, `2474117534`, `2520552521`, `2539194335`, `2623051508`, `2686586954`, `2749594341`, `2797581949`, `2802730028`, `2869476908`, `2919480363`, `2942211100`, `2968519195`, `3014221215`, `3046584086`, `3056258007`, `3057038289`, `3261907549`, `3541570803`, `3602514688`, `3658971555`, `3665388873`, `3694387091`, `4089825668`, `4214885989`

Modeled out of the noop list in Track E (2026-06-07): `2195955652` (Rotarran), `3432906971` (Hegh'ta) — see §6.5.

Two ids (`953555085`, `4214885989`) share a `loca_id` with no `ship_ability_desc` text (empty string).

---

## 2. Buckets (why they are `combat_noop` today)

| Bucket | Approx. count | Why noop |
| --- | ---: | --- |
| Economy — mining / materials | 26 | Mining speed, special nodes, officer-health mining, Latinum/Isogen/Trellium variants; out of combat scope. Generator explicitly sends these to `NOOP`. |
| Economy — loot / progression | 13 | Extra resources from hostiles, loot multipliers, encrypted intel, splicers, chaos modules, mixed Defiant research strings, post-battle cargo (Amalgam). |
| Economy — hazards / resistances | 3 | Radiation, ion storm, asteroid field resistance; not combat stats. |
| Economy — officer / alliance meta | 3 | Captain maneuver effectiveness, Cerritos support duration, Titan fortification counts, station Protector-style multi-ship shield (partially overlaps scope). |
| Economy / other (review) | 4 | Residual text the classifier did not bucket; all `keep_noop` pending only if new patterns warrant generator rules. |
| Stat — max hull / shield from ability text | 1 | “Maximum Hull/Shield Health … increased”: rolled into ship stats upstream, not a timed combat effect in this catalog. |
| Scope — defending / station / allies | 5 | “When defending”, round-start buffs to all ships and platforms; not modeled as attacker-centric hull abilities. |
| Scope — armada | 1 | Armada / non-armada overworld clauses (e.g. Borg Cutting Beam HHP outside battle). |
| Opponent ship class | **0** (remaining) | Class-gated hull rows use `condition_opponent_ship_class` + [`AbilityCondition::DefenderShipTypeIs`](../src/combat/abilities.rs). |
| Opponent tag / special faction | 5 | `[DQ]`, `[DAL]`, Krenim Invading Entities, Apex Raiders (Solo Wave Defense), etc. — keep noop until stable hostile metadata slugs. |
| Hostile debuffs / shield drain | 0 | Modeled in D2: Quv’Sompek, Sanctus, B’Rel (§6.1). |
| Proc chains | **0** (remaining) | Rotarran `2195955652` and Hegh’ta `3432906971` are now modeled with an approved per-hit/per-crit proxy (§6.5). (`2520552521` / `3014221215` were never proc chains — they are economy loot/mining ids and stay noop.) |
| Out-of-combat / overworld | 0 | (Borg cutting beam counted under Scope — armada.) |
| Weapon / mechanic disable | 1 | Collective’s Bane vs Borg Type 03 / Polygon armadas. |
| Self defensive stats vs hostiles | 0 | Intrepid `1463338054` modeled in D2 (§6.1). |
| Empty translation | 3 | `953555085`, `4214885989`, and one additional row with missing `ship_ability_desc`. |

Counts are approximate because a few lines span multiple themes (e.g. Trellium mining + Mirror hazard immunity).

---

## 3. Decisions per bucket

| Bucket | Decision |
| --- | --- |
| Economy / progression / hazards / empty | **Keep `combat_noop`.** Document only; no combat engine work. |
| Max hull / shield from ability | **Keep `combat_noop`** for timed combat; stats live on the normalized ship record. |
| Defending / station / multi-ship / takeover / armada | **Keep `combat_noop`** until scenario supports defender roles, nodes, or armada. Optional future: scenario flags + resolver conditions. |
| Opponent class (Explorer / Battleship / Interceptor) | **Modeled:** catalog `condition_opponent_ship_class`; engine uses hostile [`HostileRecord::ship_type`](../src/data/hostile.rs). |
| Special tags (DQ, DAL, Krenim, Apex raid, QC loot) | **Keep noop** or add narrow `OpponentFactionTag` / hostile metadata when upstream provides stable slugs. |
| Hostile debuffs / shield drain / round-1 debuff | **Extend resolver + combat** (hostile stat modifiers, durations). |
| Proc chains (hull breach + crit cumulative) | **Keep noop** unless a deliberate simplified proxy is accepted and documented in §3.6. |
| Intrepid-style defensive buffs | **Extend engine** (dodge / mitigation / armor as ship ability effects) or **noop** if deemed low value. |

### Classifier improvements shipped with this audit

`scripts/generate_full_ship_ability_catalog.py` now maps (and `ship_ability_catalog.json` matches):

- **True Aim** — combat start accuracy vs hostiles → `combat_begin` + `accuracy` (summed in scenario; not a crew seat).
- **Mirror / Borg / Swarm / Actian / Xindi-Aquatic** — “damage increased … against …” without the literal phrase “weapon damage” → `attack_multiplier` + `condition_opponent_faction` where slugs exist.
- **Opponent hull class** — text ties bonuses to Explorer / Battleship / Interceptor (e.g. “if the opponent’s ship is…”, “against interceptors”) → appropriate `effect_type` + `condition_opponent_ship_class`.
- **Round cap** — phrases like “first 5 rounds of combat” set catalog `round_cap` (and `AbilityCondition::RoundRange` in the resolver). Omitted when `duration_rounds` is set on the row (e.g. Crozier hostile crit reduction uses effect duration only).

Affected ids: `1800726742`, `2529591723`, `3087961933`, `3803001941`, `644714972`, `1851148569`, `2289346504`.

---

## 4. Drift control (regeneration)

1. Run from repo root: `python3 scripts/generate_full_ship_ability_catalog.py` (requires Python 3).
2. **Overrides:** After heuristics, the script merges `data/upstream/data-stfc-space/ship_ability_catalog_overrides.json` (`entries` object: ability id → full catalog row). Use this for hand-tuned rows that must survive regeneration.
3. **Diff:** Compare the regenerated `ship_ability_catalog.json` to the previous commit; port any intentional deltas either into the Python classifier or into `ship_ability_catalog_overrides.json`.

Approximations for modeled rows remain summarized in [DESIGN.md](DESIGN.md) §3.6 (Ship hull abilities).

---

## 5. Maintenance

When adding new ships from upstream:

- If a new ability stays `combat_noop`, add a row to the bucket table above (or extend the generator comment) if the reason is non-obvious.
- Prefer extending `classify_single_ability` for repeatable text patterns; use `ship_ability_catalog_overrides.json` for one-offs.

---

## 6. Track D shard audit (2026-05-19)

Eight parallel shards reviewed all noop ids against `ships/*.json` `ability[]`, `translations-ship_buffs.json`, and the live catalog. Per-id tables: [`docs/audit_shards/ship_ability_noop_shard_1.md`](audit_shards/ship_ability_noop_shard_1.md) … `_8.md`.

**Summary:** 64 `keep_noop`, 4 modeled in Track D2 (§6.1), 2 `reclassify_catalog` (earlier), 2 proc chains review-only.

### 6.1 Track D2 — implemented (2026-05-19; **data activated 2026-06-07**)

| id | Ship | Catalog `effect_type` | Engine effect | Assumption | Value |
| --- | --- | --- | --- | --- | --- |
| `701705952` | Quv’Sompek | `hostile_counter_stat_debuff` | [`HostileCounterStatDebuff`](../src/combat/abilities.rs) — 5 rounds | Uniform pierce multiplier on counter-fire (proxy for armor/shield pierce + accuracy debuff). | 0.12 |
| `1379978713` | Sanctus | `defender_shield_drain_per_round` | [`DefenderShieldDrainPerRound`](../src/combat/abilities.rs) — `round_start`, 5 rounds | Drains `fraction × max_shield` at round start. | 0.10 |
| `2441576367` | B’Rel | `hostile_counter_stat_debuff` | Same — `duration_rounds: 1` | First-round-only pierce debuff (same proxy as Quv’Sompek). | 0.15 |
| `1463338054` | U.S.S. Intrepid | `hostile_engagement_defensive` | [`HostileEngagementDefensiveBonus`](../src/combat/abilities.rs) | Same % added to counter-fire mitigation + dodge sums. | 0.40 |

> **Data activation fix (2026-06-07).** Track D2 mapped the catalog/overrides in 2026-05-19 but the
> abilities were **dormant in the simulator** until now, for two reasons: (1) `data/ships_extended`
> was never regenerated (the four rows stayed `combat_noop` / `value 0.0`), and (2) the catalog rows
> set `value_is_percentage: true` while the upstream values are already fractional (e.g. `0.15` =
> 15%), so a regeneration alone would have baked them **100× too small** (`0.0015`). Both are fixed:
> the rows now carry `value_is_percentage: false` + `ignore_upstream_value_is_percentage: true` (the
> documented "upstream marks small decimals as %" case — see [`normalize_data_stfc_space.rs`](../src/bin/normalize_data_stfc_space.rs)),
> and `ships_extended` is regenerated. The **Value** column shows the resolved per-ability fraction
> (first-tier `values[]` entry; these rows are not level-scaled). Re-run after catalog edits:
> `cargo run --bin normalize_data_stfc_space`.

Tests: [`tests/ship_ability_hostile_debuff.rs`](../tests/ship_ability_hostile_debuff.rs). Regenerate `ships_extended` after catalog change: `cargo run --bin normalize_data_stfc_space` (or full data refresh).

### 6.2 Proc chains (modeled in Track E — see §6.5)

The proc chains are the two breach-gated cumulative crit abilities below. An earlier revision of
this section listed them under the ids `2520552521` / `3014221215`; that was wrong — those ids are
economy abilities (transogen loot / tritanium mining) and remain `combat_noop`. The real proc-chain
ids and ships:

| id | Ship | Ability | Behaviour |
| --- | --- | --- | --- |
| `2195955652` | Rotarran | Bird of Prey | While opponent **Hull Breached**, every **critical** hit adds cumulative crit **damage**. |
| `3432906971` | Hegh’ta | Open the Wound | While opponent **Hull Breached**, every weapon **hit** adds cumulative crit **chance**. |

### 6.3 In-game verification (HiggsBozo)

- Confirm B’Rel `2441576367` debuff stat (pierce vs accuracy vs damage) and round-1-only scope.
- Confirm Sanctus `1379978713` shield drain is % of max shield per round and round cap from ability tier.
- Intrepid `1463338054`: whether dodge/deflection stack with officer buffs in hostile fights.
- Reclassified ids `509252162`, `2425475474`: spot-check optimizer scenarios that use those ships still pick up hull abilities.

### 6.4 Reclassified (remove from noop inventory)

| id | Current `effect_type` |
| --- | --- |
| `509252162` | `attack_multiplier` |
| `2425475474` | `conqueror_borg_beam_suppression` |

### 6.5 Track E — breach-gated cumulative crit proc chains (2026-06-07)

The two proc chains from §6.2 are now modeled. Both are passive ship hull abilities ("always
active") that only do anything **while the opponent has Hull Breach**:

| id | Ship | Catalog `effect_type` | Engine effect | Per-tier value |
| --- | --- | --- | --- | --- |
| `3432906971` | Hegh'ta — Open the Wound | `cumulative_breach_crit_chance` | [`AbilityEffect::BreachCumulativeCritChancePerHit`](../src/combat/abilities.rs) | +2% → +20% crit chance per hit |
| `2195955652` | Rotarran — Bird of Prey | `cumulative_breach_crit_damage` | [`AbilityEffect::BreachCumulativeCritDamagePerCrit`](../src/combat/abilities.rs) | +10% → +20% crit damage per crit |

**Model.** The game text is "every hit / every crit, cumulative." The engine applies the per-hit
(Hegh'ta) and per-crit (Rotarran) increments **per shot** at the crit-resolution site
([`src/combat/engine.rs`](../src/combat/engine.rs)) — i.e. true per-event, not a round-granular
approximation — counting only events while the defender is hull breached so mid-round breach onset
is honored. The bonus on a given shot reflects all *prior* qualifying events (it benefits subsequent
shots, not the one that triggered it):

- Hegh'ta: `crit_chance += per_hit × (breached hits so far)`. **Uncapped**; the crit-chance roll
  itself clamps to `[0, 1]`, so this saturates to near-guaranteed crits within a round or two.
- Rotarran: `crit_mult = weapon_crit × officer_crit + per_crit × (breached crits so far)`.
  **Additive percentage points** on the crit multiplier (a `+X%` crit-damage stat bonus), threaded
  through [`resolve_vehicle_weapon_crit`](../src/combat/crit.rs) as a dedicated additive term rather
  than a multiplicative chain factor — so a +11% increment is +0.11 on the multiplier, not ×1.11.
  **Uncapped** (a deliberate snowball); only grows on rounds where crits land while breached.

The per-tier value is taken from the upstream `values[]` curve and level-scaled at ship resolution
([`ship_ability_value_for_level`](../src/data/ship.rs)). Catalog mapping lives in
[`ship_ability_resolve.rs`](../src/data/ship_ability_resolve.rs); durable overrides in
`ship_ability_catalog_overrides.json`. Tests:
[`tests/ship_ability_hostile_debuff.rs`](../tests/ship_ability_hostile_debuff.rs) (engine, incl. an
inert-without-breach gate), [`src/combat/crit.rs`](../src/combat/crit.rs) (additive crit-damage
unit tests), and [`tests/ship_ability_breach_crit_data.rs`](../tests/ship_ability_breach_crit_data.rs)
(data-driven resolution). Regenerate `ships_extended` after catalog edits: `cargo run --bin
normalize_data_stfc_space`.

**Open questions for in-game verification (HiggsBozo)** — empirical, not modeling choices:
1. "Every hit/crit" granularity — per shot (current) vs per weapon-volley.
2. Whether the accumulated buff **persists** when the breach lapses (current: persists, only grows
   while breached) or resets.
3. Confirm the crit-damage bonus is additive to the crit-damage stat (current model) and not
   multiplicative — this is the one place STFC's convention is debated.

### 6.6 U.S.S. Athena pass + apex-barrier defensive rewiring (2026-07-15)

Official sources (all quote consistent values): the
[Update 91 feature highlight](https://startrekfleetcommand.com/news/update-91-feature-highlight-the-u-s-s-athena/),
the [support FAQ](https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/8799-the-uss-athena/),
and the [Critical Mitigation guide](https://startrekfleetcommand.com/news/starfleet-academy-remote-campus-critical-mitigation/).
Both officially quoted scaling values sit at upstream `values[24]` (level 25): Fury 110,000
renders "11,000,000%" under the `{0:#.#%}` ×100 convention, Revenge 310,000 renders flat via
`{0:#}` — which ground-truths the value conventions for all four rows.

| id | Ability | Catalog `effect_type` | Change (2026-07-15) |
| --- | --- | --- | --- |
| `2357321655` | Athena's Fury | `attack_multiplier` (VENRA-gated) | None — scale validated (raw values are bonus fractions; 85,000 at L1 → 20,000,000 at L75 is intentional hard-counter design). Endpoints now pinned in tests. |
| `1913694321` | Athena's Revenge | `crit_mitigation_rating` (VENRA-gated) | Was a mis-classified **ungated** `apex_barrier`. Now modeled: flat Critical Mitigation rating → `HostileCritDamageReduction` with `reduction = CM / (CM + 50,000)` (formula pinned by the official worked example: 83,000 ⇒ 62.41% of the full crit damage). Always active; the resolver's 0.95 clamp binds above rating 950,000 (Revenge exceeds it from ~mid levels). |
| `2506949026` | Athena's Valor | `apex_barrier` (VENRA-gated) | Was ungated — a spurious 10M barrier in every fight. Gate added. |
| `39689355` | Athena's Wrath | `combat_noop` | Wave Defense vs Academy Drones is outside simulated scenarios (and no Academy Drone faction tag exists); was an ungated 15M barrier in every fight. |

Athena's Solace (Programmable Matter immunity, flavor inside loca 91001's combined text) has no
separate upstream row; it becomes relevant only if Programmable Matter itself is modeled
(see COMBAT_FIDELITY_BACKLOG.md item 13).

**Apex-barrier seat rewiring (engine).** Attacker-crew `ApexBarrierBonus` seats (officer / ship
hull / research) were previously composed into the **defender's** barrier on the player's own
outbound fire — i.e. a player ship's barrier ability *reduced its own damage* and provided no
defense (pre-fix, the Athena's ungated 25M of barrier seats crushed her outbound damage ~2500×
against every hostile). Apex Barrier is a defensive stat: seats now feed the **counter-fire**
apex factor (`compute_apex_damage_factor(defender_shred, attacker_barrier)`) via the
condition-aware [`attacker_apex_barrier_bonus_active`](../src/combat/abilities.rs), and outbound
fire faces only the defender's own barrier. Flat profile/component sources already used the
counter-fire path and are unchanged. Known approximation: PvP **defender** barrier seats are
folded in unconditionally at scenario build (`hostile_apex_barrier_bonus_from_defender_crew`
ignores seat conditions), so a faction-gated barrier on a PvP defender applies vs all attackers.
Tests: `tests/combat_tests.rs` (officer + tag-gated barrier, both directions),
`tests/ship_ability_athena_venari_ral.rs`.
