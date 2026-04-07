# Structured audit: `combat_noop` ship abilities

This document expands on [ROADMAP.md](ROADMAP.md) § Ship Abilities — audit `combat_noop`.

**Catalog revision (2026-04-04):** There are **140** upstream ability ids in `data/upstream/data-stfc-space/ship_ability_catalog.json`. **73** map to `effect_type: combat_noop` (inventory-only in combat). **67** are modeled for the sim (timing + effect resolved in `src/data/ship_ability_resolve.rs` and related combat code). Opponent hull-class gates (`condition_opponent_ship_class`) are evaluated against the hostile’s `ship_class` in [`CombatContext::defender_ship_type`](../src/combat/abilities.rs).

Descriptions are keyed by `translations-ship_buffs.json` (`key: ship_ability_desc`, `id` = per-row or ship `loca_id` from `ships/*.json`).

---

## 1. Inventory

All `combat_noop` ability ids (sorted; regen-safe):

`34867572`, `49906243`, `78080222`, `87414807`, `108924704`, `293385368`, `509252162`, `546190599`, `673187302`, `701705952`, `711428193`, `732090900`, `835292335`, `915894112`, `953555085`, `957303751`, `974800413`, `987222969`, `1004533782`, `1027217748`, `1029262994`, `1087128295`, `1090374551`, `1160666017`, `1244824002`, `1307832955`, `1379978713`, `1428543762`, `1439253182`, `1463338054`, `1492898704`, `1535317053`, `1577508895`, `1738424547`, `1784814733`, `1823660918`, `1839370465`, `1878809713`, `1972093910`, `1982797639`, `2004925834`, `2057434885`, `2195955652`, `2254702328`, `2302150828`, `2425475474`, `2441576367`, `2468986074`, `2474117534`, `2520552521`, `2539194335`, `2623051508`, `2686586954`, `2749594341`, `2797581949`, `2802730028`, `2869476908`, `2919480363`, `2942211100`, `2968519195`, `3014221215`, `3046584086`, `3056258007`, `3057038289`, `3261907549`, `3432906971`, `3541570803`, `3602514688`, `3658971555`, `3665388873`, `3694387091`, `4089825668`, `4214885989`

Two ids (`953555085`, `4214885989`) share a `loca_id` with no `ship_ability_desc` text (empty string).

---

## 2. Buckets (why they are `combat_noop` today)

| Bucket | Approx. count | Why noop |
| --- | ---: | --- |
| Economy — mining / materials | ~28 | Mining speed, special nodes, officer-health mining, Latinum/Isogen/Trellium variants; out of combat scope. Generator explicitly sends these to `NOOP`. |
| Economy — loot / progression | ~13 | Extra resources from hostiles, loot multipliers, encrypted intel, splicers, chaos modules, mixed Defiant research strings, post-battle cargo (Amalgam). |
| Economy — hazards / resistances | ~3 | Radiation, ion storm, asteroid field resistance; not combat stats. |
| Economy — officer / alliance meta | ~4 | Captain maneuver effectiveness, Cerritos support duration, Titan fortification counts, station Protector-style multi-ship shield (partially overlaps scope). |
| Stat — max hull / shield from ability text | ~2 | “Maximum Hull/Shield Health … increased”: rolled into ship stats upstream, not a timed combat effect in this catalog. |
| Scope — defending / station / allies | ~5 | “When defending”, round-start buffs to all ships and platforms; not modeled as attacker-centric hull abilities. |
| Scope — takeover / nodes | ~1 | Capture or mining node in Takeover. |
| Scope — armada | ~4 | Armada-only clauses or bundles (including Franklin-A Swarm + Armada, Revenant + Borg armada disable, Stella Eclipse + Armada). |
| Opponent ship class | ~~6~~ **0** (remaining) | Class-gated hull rows use `condition_opponent_ship_class` + [`AbilityCondition::DefenderShipTypeIs`](../src/combat/abilities.rs). Regen catalog after upstream text changes. |
| Opponent tag / special faction | ~5 | `[DQ]`, `[DAL]`, Krenim Invading Entities, Apex Raiders (Solo Wave Defense), Q-Continuum reward lines — not mapped to [`OpponentFactionTag`](../src/combat/types.rs) or unsafe to infer. |
| Hostile debuffs / shield drain | ~3 | Decrease hostile pierce/accuracy; Sanctus-style shield drain over rounds; B’Rel first-round debuff — need hostile-side stat hooks and timings. |
| Proc chains | ~2 | Hull breach + crit + cumulative (e.g. Rotarran, Hegh’ta); intentionally left unmodeled per [DESIGN.md](DESIGN.md) §3.6. |
| Out-of-combat / overworld | ~1 | Borg Cutting Beam HHP outside battle. |
| Weapon / mechanic disable | ~1 | Collective’s Bane vs Borg Type 03 / Polygon armadas. |
| Self defensive stats vs hostiles | ~1 | U.S.S. Intrepid — armor, shield deflection, dodge vs hostiles (not “weapon damage” Gladius pattern). |
| Empty translation | ~2 | No description text for `loca_id`. |

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
