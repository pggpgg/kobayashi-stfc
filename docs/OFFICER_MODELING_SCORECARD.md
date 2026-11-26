# Officer modeling scorecard

This file is **generated**. Do not edit the table by hand. Edit manual notes in `data/officers/officer_modeling_fidelity.yaml`, then run:

```bash
cargo run --bin generate_officer_scorecard
```

## What this measures

- **Auto columns** use the same effect classifier as `/api/mechanics/coverage` ([`lcars_effect_coverage`](../src/lcars/resolver.rs)): Implemented / Partial / Ignored.
- They do **not** detect semantic bugs (wrong target, %-of-SHP modeled as flat HP, missing level caps). Use the **`fidelity`** column for that.

## Combat-intent effects

- Non-`tag` LCARS effects are always combat-intent for this scorecard.
- `type: tag` is combat-intent **unless** the tag string contains `:non_combat` (economy / meta). Tags without that marker (including `:unmapped`) count as **combat gaps**: raw score 0, and they add to **`unmapped_penalty`**.

## Per-effect raw score (0–100)

| Coverage tier | Raw |
|---------------|-----|
| Implemented | 100 |
| Partial | 50 |
| Ignored | 0 |

Combat-intent `tag` lines are always treated as raw **0** for the average (engine skips them in combat).

## Subscores (0–100 integers)

- **`combat_avg`**: arithmetic mean of raw scores over all combat-intent effects. `—` if there are none.
- **`combat_weighted`**: weighted mean — captain ability block **2×**, bridge block **1.5×**, below decks **1×**. `—` if no combat-intent effects.
- **`unmapped_penalty`**: `min(100, 25 × unmapped_combat_tags)` where each combat-intent tag (non-`:non_combat`) counts as one line.
- **`combat_auto`**: `clamp(0, 100, combat_weighted - unmapped_penalty)`; `—` if no combat-intent effects.
- **`grade`**: from `combat_auto` — A≥90, B≥80, C≥65, D≥50, F<50.
- **`nc_ack`**: non-combat tag acknowledgment — **100** if there are no tags or all tags are `:non_combat`; **50** if mixed; **0** if any combat-intent tag (no `:non_combat`).
- **`cap_score` / `br_score` / `bd_score`**: mean raw score within that ability block only (`—` if no combat-intent lines in that block).

## Sort order

Rows with at least one combat-intent effect appear first, sorted by **`combat_auto`** ascending (worst first), then **`unmapped_combat_tags`** descending. Officers with **no** combat-intent lines are listed last (sorted by id).

## Column reference

| Column | Meaning |
|--------|---------|
| `cap_I/P/I` | Implemented / Partial / Ignored counts (captain ability block, combat-intent only) |
| `br_I/P/I` | Same for bridge block |
| `bd_I/P/I` | Same for below decks |
| `drop_trig` | Combat-intent effects the LCARS→IR adapter dropped because their `trigger` is unknown |
| `drop_tag` | Same, dropped because the `tag` has no engine-stat mapping (parallels `unmapped_tags`) |
| `drop_stat` | Same, dropped because `stat_modify.stat` has no engine-modifier mapping |
| `drop_cond` | Same, dropped because the `condition` block can't be represented in the canonical IR |

---
| id | name | combat_n | cap_I/P/I | br_I/P/I | bd_I/P/I | unmapped_tags | drop_trig | drop_tag | drop_stat | drop_cond | cap_score | br_score | bd_score | combat_avg | combat_wtd | unmap_pen | combat_auto | grade | nc_ack | nc_label | fidelity |
|---:|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---|---|
| barot-2d7be0 | Barot | 1 | 0/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 0 | — | 0 | 0 | 0 | 0 | F | 100 | economy_only | — |
| damar-796eb0 | Damar | 2 | 0/0/1 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — | 0 | 0 | 0 | 0 | F | 100 | none | Captain and bridge effects require a qualifying critical hit and a weapon-scoped grant. Both are unsupported and diagnosed. The bridge effect reduces opponent critical damage for three rounds. |
| gaila-3d387a | Gaila | 1 | 0/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 0 | — | 0 | 0 | 0 | 0 | F | 100 | economy_only | Bridge opponent critical damage reduction against hostiles through level 70 is unsupported and diagnosed pending evidence for its damage pool. |
| leslie-975ce0 | Leslie | 1 | 0/0/1 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | — | — | 0 | 0 | 0 | 0 | F | 100 | economy_only | — |
| origins-stamets-b0decf | Origins Stamets | 1 | 0/0/1 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | — | — | 0 | 0 | 0 | 0 | F | 100 | economy_only | — |
| toli-2d704a | Toli | 1 | 0/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 0 | — | 0 | 0 | 0 | 0 | F | 100 | economy_only | Bridge RoundStart enemy AllDefenses MultiplyBaseSub (1664195138) remains quarantined as unsupported with diagnostics pending enemy-target op 6 AllDefenses verification. |
| gossa-dafefb | Gossa | 2 | 0/0/0 | 0/0/1 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 0 | 100 | 50 | 40 | 0 | 40 | F | 100 | none | Bridge opponent critical damage reduction for Borg Solo Armadas is unsupported and diagnosed pending evidence for its round-start path; below-decks cumulative raw AllDefenses vs Assimilated players is modeled. |
| arix-b3d602 | Arix | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Powered Armor adds 15–50% to raw base Armor in non-Eclipse hostile ship fights when remaining hull at round start is below 70% of the hull carried into that fight. Plasma Backlash remains unsupported with diagnostics pending evidence of its energy-hit timing within a multi-shot attack. |
| charvanek-0f1b5c | Charvanek | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| d-jaoki-baa18f | D'Jaoki | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 1 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Captain battle-won Health remains unsupported and diagnosed; bridge RoundStart AllDefenses (1R, Health, while target Burning) is modeled via unified op 5 PRE_ALL defense. |
| emp-georgiou-1564b5 | Emp. Georgiou | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| georgiou-d2bdef | Georgiou | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| harry-mudd-374d5f | Harry Mudd | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Cap ShieldMitigation 2R vs hostiles ≤70; bridge on_combat_start shots bonus same gate. |
| honorguard-worf-8ac58c | Honorguard Worf | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Captain crew-Attack-scaled ArmorPiercing on crit vs players (2884241557) is quarantined as unsupported with diagnostics rather than mapped to damage-through shield_pierce. |
| joachim-03587e | Joachim | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| khan-3f1d1e | Khan | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| linkasa-01a1b3 | Linkasa | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| moreau-23ddb4 | Moreau | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 1 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| origins-burnham-e854d6 | Origins Burnham | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| origins-saru-753b24 | Origins Saru | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| qa-ug-a165bf | Qa'Ug | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Captain AllDefenses MultiplySub remains unsupported and diagnosed (tooltip vs modifier mismatch); bridge Accuracy when attacker hull breach. |
| severus-93daaf | Severus | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| tyler-1dcc4d | Tyler | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | — |
| vemet-00a218 | Vemet | 2 | 0/0/1 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 100 | — | 50 | 43 | 0 | 43 | F | 100 | none | Captain post-defeat ship survival remains unsupported and diagnosed; bridge RoundStart enemy ShieldHPRepair MultiplyBaseSub (2633638696, Health, while target Burning) is modeled via unified op 6 flat enemy shield drain. |
| ahvix-f90184 | Ahvix | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | — |
| bones-ddc8a9 | Bones | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | Cap OffAbilityEffect at combat start vs hostiles ≤70; bridge on_hit armor same gate. Inactive in default optimize vs high-level hostiles. |
| changeling-kira-666ebe | Changeling Kira | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | Bridge EnemyTakesHit enemy AllDefenses MultiplyBaseSub (2108234991) remains quarantined as unsupported with diagnostics pending enemy-target op 6 AllDefenses verification. |
| eurydice-18b643 | Eurydice | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | Bridge shield restoration is unsupported and diagnosed: the current shield-break handler cannot faithfully apply its chance and percentage of maximum shields. |
| tal-c3e4eb | Tal | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | — |
| tilly-6bd08f | Tilly | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | — |
| tiza-9d38f9 | Tiza | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | Cap enemy ShieldMitigation 3R (faction gate); bridge chance-gated CombatStart AllDefenses remains unsupported and diagnosed. |
| tos-spock-86f176 | TOS Spock | 2 | 1/0/0 | 0/0/1 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 0 | — | 50 | 57 | 0 | 57 | D | 100 | none | Captain RoundStart ShieldHPRepair (MultiplyBaseAdd, Defense) is modeled as direct flat shield repair while defending (SelfDefending). Bridge opponent critical damage reduction while the ship has Morale in PvP remains unsupported and diagnosed. |
| vil-winn-adami-6c42f3 | Vil Winn Adami | 2 | 0/0/0 | 1/0/0 | 0/0/1 | 0 | 0 | 0 | 0 | 0 | — | 100 | 0 | 50 | 60 | 0 | 60 | D | 100 | none | Bridge enemy ShieldMitigation lasts three rounds. Below-deck opponent critical damage reduction in PvP is unsupported and diagnosed. |
| kirk-1323b6 | Kirk | 2 | 0/1/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 50 | 100 | — | 75 | 71 | 0 | 71 | C | 100 | none | Cap OfficerStatAll on_round_start when Morale active (1R) — Phase 4d 3-axis per-round path (attack/defense/health). Bridge on_round_start Morale proc separate. |
| 718-0-2509d7 | 718.0 | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Computer Precision adds 5–15% to the Defense pool of every crewed officer in non-Eclipse hostile ship fights. Station Knowledge (+CritChance) activates when attacker_at_station is enabled or on the defending crew in pvp_station raids (SelfAtStation). |
| academy-doctor-bc8e69 | Academy Doctor | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge isolytic_cascade_damage at combat start vs Explorer hostiles is gated on Wave Defense (SelfAtWaveDefenseChallenge → engagement_includes: waves): active when wave_defense is enabled, inert on the default path. BD Interceptor crit_chance (8R) is EnemyPlayer-gated: inert vs NPC hostiles, active in PvP. |
| airiam-9265fc | Airiam | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| alok-sahar-4d1370 | Alok Sahar | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ambassador-spock-e7945f | Ambassador Spock | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge isolytic_defense at combat start is OGN-faction-gated. BD My Mine to Your Mine is node-defense-only (SelfMining → literal_false): inert in all sim paths. |
| andy-billups-c27ba7 | Andy Billups | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| annorax-830d35 | Annorax | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge Time is Patient (2281926825, HitTaken, num_rounds=2, Tal-not-on-bridge) is modeled as reactive flat Apex Barrier (+2,000..+3,200 once per incoming weapon volley, active from the next sub-round; verified against 2026-11-26 Fight D Vi'Dar capture). |
| arkady-94c81b | Arkady | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| azetbur-7eff22 | Azetbur | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain kinetic-hit Hull Breach gate remains unmodeled; bridge EnemyTakesHit ArmorPiercing MultiplyAdd (3301945997) is modeled as cumulative post-volley single-axis Armor Piercing pool addition. |
| b-elanna-torres-75cf02 | B'Elanna Torres | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge HitTaken AllPiercing (MultiplyBaseAdd, 1R, Attack) is modeled as reactive PRE_ALL piercing (verified against 2026-11-26 Vi'Dar live capture); below-deck hull breach retains its source-declared one-round state duration. |
| ba-el-91d122 | Ba'el | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| badgey-13df33 | Badgey | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| beckett-mariner-d93865 | Beckett Mariner | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Get It Together! adds 5–30% to every crewed officer's Attack pool before ship breakpoints. The below-deck ability separately increases weapon damage. |
| benjamin-sisko-5c51f2 | Benjamin Sisko | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| beverly-crusher-74b2d7 | Beverly Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain RoundStart ShieldHPRepair (MultiplyBaseAdd, Health, EnemyHostile) is modeled as direct flat shield repair (verified against 2026-11-26 Fight 1 Vi'Dar capture: 193,144 SHP/round); bridge CombatStart AllDefenses (MultiplyBaseAdd, Defense, EnemyHostile) is modeled via unified op 5 PRE_ALL defense. |
| black-ops-chapel-814fe8 | Black Ops Chapel | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| black-ops-m-benga-e23e3a | Black Ops M'Benga | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| borg-queen-c8e67d | Borg Queen | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| brad-boimler-ee5262 | Brad Boimler | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| byr-ch-kelrer-090eff | Byr Ch'Kelrer | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| cadet-kirk-a80563 | Cadet Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge officer-stat pool and bridge passive weapon damage are both represented. |
| cadet-mccoy-13d460 | Cadet McCoy | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge Health pool is represented; bridge captain-maneuver effect uses the existing PvP proxy. |
| cadet-scotty-b342ae | Cadet Scotty | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| cadet-sulu-784421 | Cadet Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| cadet-uhura-3ef15c | Cadet Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| caleb-mir-3588ed | Caleb Mir | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge shield_regen_max_fraction (per-round SHP restore, MultiplyAdd) is gated on EnemyNonArmadaHostileOrOutpost (active vs non-Armada NPC hostiles and Outposts). BD Top Marks weapon_damage is gated on EnemyOutpostOrRetaliator (active vs Outpost Armadas and Outpost Retaliation Attackers, inert vs generic hostiles). |
| cap-tilly-070cc9 | Cap. Tilly | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| carol-755a05 | Carol | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| carol-freeman-a46be4 | Carol Freeman | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| cat-spock-61edec | Cat Spock | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge base crit_damage at combat start is OGN-faction-gated (up to 25,000% at rank 5 — matches Update 92 notes). BD apex_barrier vs non-Armada hostiles is unconditional on the default hostile path. |
| cath-f0e149 | Cath | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| chakotay-a1f5df | Chakotay | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| chancellor-ake-225a78 | Chancellor Ake | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge isolytic_cascade_damage vs Battleship hostiles is Wave-Defense-gated (engagement_includes: waves): active when wave_defense is enabled, inert on the default path. BD Explorer crit_chance (8R) is EnemyPlayer-gated: inert vs NPC hostiles. |
| chang-ecc238 | Chang | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge enemy_delay on crit (on_critical) when defender hull breach active; DefenderFireDelay skips counter-fire. Captain weapon bonus uses hull_hp <60% gate. |
| chen-cdb1ca | Chen | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| culber-b3e4a0 | Culber | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| d-vana-tendi-9fabf0 | D'Vana Tendi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge passive OfficerStatHealth; BD passive hull_hp. |
| dajash-tolra-1e809f | Dajash Tolra | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| darwin-b8dc0a | Darwin | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| data-d20ef8 | Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| deanna-troi-57341d | Deanna Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| decius-8fce68 | Decius | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| deidamia-50f1da | Deidamia | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge isolytic_cascade_damage vs Interceptor hostiles is Wave-Defense-gated (engagement_includes: waves): active when wave_defense is enabled, inert on the default path. BD Battleship crit_chance (8R) is EnemyPlayer-gated: inert vs NPC hostiles. |
| demarco-7f2d86 | Demarco | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| dezoc-381416 | Dezoc | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge OfficerStatAll at combat start (AND: solo_armadas + defender hull faction) via pending; BD assimilated on_round_start when defender hull <95%. |
| dixon-hill-b7ea10 | Dixon Hill | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| doctor-t-ana-b98f82 | Doctor T'Ana | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | AddRandomState vs player defenders only (PvP); same weighted states and rank proc as Zeph; inactive in default hostile optimize. |
| eight-of-eleven-cb5f3c | Eight Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cybernetic Enhancements adds 45–85% to every crewed officer's Attack pool before ship breakpoints. |
| ent-e-data-871245 | Ent-E Data | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge isolytic_cascade_damage at combat start vs NPC hostiles only, excluding Armada (EnemyHostile + TargetNotArmada → defender_is_npc_hostile AND NOT defender_ship_type_is armada). |
| ent-e-picard-556227 | Ent-E Picard | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| ent-e-riker-516b8d | Ent-E Riker | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| ent-e-troi-46cdc3 | Ent-E Troi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge RoundStart enemy ShieldHPRepair MultiplyBaseSub vs non-Armada hostiles (2298455942, Attack) is modeled via unified op 6 flat enemy shield drain (verified against 2026-11-26 Fight C + 1R reset captures: 370,176 = 8 × effective_crew_attack SHP/round). |
| ezri-dax-bb0892 | Ezri Dax | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| fcm-data-3b9e4d | FCM Data | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Captain Plexurus of Borg (1610205778) is in its verified captain seat (+ShotsPerAttack for 2 rounds vs Group Armadas); bridge Through Space and Time (ArmadaLoot) is economy → :non_combat. |
| female-spock-c21a12 | Female Spock | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Captain Enterprising Mind is Isogenite loot (economy → :non_combat; upstream rank 4-5 values unpublished, stored as 0). Bridge isolytic_damage at combat start is OGN-faction-gated: active only vs Origin Sector hostiles/armadas. |
| five-of-eleven-d9aa11 | Five Of Eleven | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| french-marshal-q-1f0a28 | French Marshal Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| garak-771862 | Garak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| genesis-lythe-82d460 | Genesis Lythe | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | No captain (Chain of Command). Bridge shield_regen_max_fraction restore is Wave-Defense-gated (SelfAtWaveDefenseChallenge → engagement_includes: waves): active when wave_defense is enabled, inert on the default path. BD Admiral's Daughter (Outpost retaliation medals/plunder) is economy → :non_combat tag. |
| geordi-la-forge-914cec | Geordi La Forge | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| gonzales-de640c | Gonzales | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| goon-891942 | Goon | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| gorkon-b8d2e7 | Gorkon | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain crit window OK; bridge hull breach proc is correctly modeled as hull_breach effect with chance scaling. |
| gowron-27ac30 | Gowron | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge CombatStart Shields (MultiplyBaseAdd, Health) is modeled as PRE_ALL single-axis base Shield Deflection rather than shield regeneration. |
| gul-dukat-70e9d7 | Gul Dukat | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| harrison-56cc6c | Harrison | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain: Explorer hostile gate modeled in LCARS; level-70 unlock gated via defender_level_at_most condition; bridge Sabotage uses ShieldMitigationBypassFraction on defender mitigation for round 1 only (duration→RoundRange). |
| harry-kim-a79fdf | Harry Kim | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | Native English identity binds source 497650232 to Harry Kim: below-decks Ashes to Ashes applies Burning (Tal-not-on-bridge gate); Voyager active-ability cost efficiency is non-combat. The prior catalog swapped this officer with Neelix. |
| hendorff-549c65 | Hendorff | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| hugh-9fc348 | Hugh | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| icheb-fb3bd7 | Icheb | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ikatika-4fa1ba | Ikat’Ika | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| infiltrator-tuvok-5d3048 | Infiltrator Tuvok | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| instr-spock-bba5d7 | Instr. Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| jack-ransom-dfdb38 | Jack Ransom | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| jadzia-dax-736698 | Jadzia Dax | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| javaid-fdff59 | Javaid | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap passive shield_piercing tag; bridge passive armor. |
| jaylah-857412 | Jaylah | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| jean-luc-picard-e0515c | Jean-Luc Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain off-ability boost is tag/non_combat path (ignored in sim); bridge crit damage roughly OK. Not an officer-stat-scaling gap. |
| jonathan-archer-4e9cd0 | Jonathan Archer | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| julian-bashir-0551fa | Julian Bashir | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| k-bisch-b6c84c | K'Bisch | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| kang-55e67a | Kang | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap on_combat_start hull_breach; bridge CombatStart Accuracy (MultiplyBaseAdd, Attack) is modeled via unified op 5 PRE_ALL single-axis Accuracy. |
| kathryn-janeway-bd4a19 | Kathryn Janeway | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap reactive HullDamageTaken AttackerShieldMitigationBonus 1R vs NPC hostiles; bridge isolytic_cascade_damage at combat start. |
| kati-01ab4d | Kati | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| keenser-3a5ad4 | Keenser | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| kerla-6cdf45 | Kerla | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap ShipDodge on_shield_break; bridge crit_damage at combat start. |
| kira-nerys-a5253a | Kira Nerys | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge enemy ShieldMitigation is gated on SelfAtAssault2 (engagement_includes: assaults; inert on default ship-vs-hostile path); BD crit_chance at combat start. |
| klaa-acbd92 | Klaa | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| komal-357fb2 | Komal | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| koth-c70d1c | Koth | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| kras-a47042 | Kras | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | PvP-only: captain Art of War debuffs opponent captain maneuver; bridge Know Your Enemy debuffs enemy bridge officer stats (no-op vs NPC hostiles). |
| krell-ef559b | Krell | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| kumak-c5b0db | Kumak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap OfficerStatAll on_round_start via Phase 4d per-round path; bridge BaseMitigationStatsBonus on_round_start when defender burning (raw mitigation curve). |
| kuron-15cda2 | Kuron | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | Captain self_recharge at combat start modeled as +100% shots round 1 (ShotsBonus proxy; recharge≈double-shots hypothesis). ModuleKinetic gate honored: bonus applies to kinetic weapons only (weapon_type from ships_extended/hostile components; untyped weapons match leniently). |
| l-nar-ae14f4 | L'Nar | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge Health pool and bridge CombatStart AllDamage (MultiplyBaseAdd, Attack, vs Romulan) POST_ALL pre-crit flat weapon damage are both modeled. |
| laliari-87e81a | Laliari | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | Omega-13 cooldown reduction on ship active ability — not modeled in combat sim (:non_combat). |
| lieutenant-picard-6303e7 | Lieutenant Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| livis-43235e | Livis | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| lorca-d32ec8 | Lorca | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| lursa-57a544 | Lursa | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Cloaking cooldown on ship active ability — not modeled in combat sim (:non_combat). |
| m-benga-53446d | M'Benga | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| m-ral-986e7a | M'Ral | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain ShipLaunched ArmorPiercing MultiplyAdd (508362235) is modeled via single-axis base Armor Piercing pool addition (verified against 2026-11-26 Fight E Vi'Dar capture: +35% × 13,089 = +4,581.15 AP without touching Shield Piercing or Accuracy). |
| mara-bd3ca6 | Mara | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap weapon_damage at combat start; bridge enemy ShipDodge on_round_end. |
| mara-dalen-6827 | Mara Dalen | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| marcus-073931 | Marcus | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain morale timing remains approximate; bridge CombatStart ShieldPiercing (MultiplyBaseAdd, Defense) is modeled via unified op 5 PRE_ALL single-axis Shield Piercing. |
| mariachi-q-c8cf6e | Mariachi Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| marla-9732c7 | Marla | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| martok-89ef39 | Martok | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap weapon_damage at combat start; bridge CombatStart ShipDodge (MultiplyBaseAdd, Defense) is modeled via unified op 5 PRE_ALL single-axis Dodge. |
| masriad-vael-50cf64 | Masriad Vael | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| michael-burnham-6c711c | Michael Burnham | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge HullDamageTaken enemy ShieldHPRepair MultiplyBaseSub vs players (2958606151, Attack) is modeled via unified op 6 reactive flat enemy shield drain. |
| miles-o-brien-f0c92f | Miles O'Brien | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| mirek-953093 | Mirek | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge CombatStart AllDamage vs Federation (MultiplyBaseAdd, Defense) is modeled as POST_ALL pre-crit flat weapon damage (verified against 2026-11-26 Fight 3 Vi'Dar, Fight 5 Realta, and Fight B Realta Pike×Mirek captures). |
| mirror-data-e13d66 | Mirror Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-ezri-2f6326 | Mirror Ezri | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-kira-bc42ce | Mirror Kira | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-picard-7c1a17 | Mirror Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-troi-d237df | Mirror Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-uhura-0a7168 | Mirror Uhura | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| miss-q-cc911e | Miss Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mitchell-0217f7 | Mitchell | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge stat pool is gated on SelfAtStation (active when attacker_at_station is set or on the defending crew in pvp_station raids); bridge reactive CriticalShotTaken 1R BaseWeaponDamageBonus is modeled. |
| mudd-32546a | Mudd | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| navi-0b328a | Navi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| neelix-c8a380 | Neelix | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | Native English identity binds source 1458469333 to Neelix: below-decks Morale, with non-combat reputation officer ability. The prior catalog swapped this officer with Harry Kim. |
| nero-1aeca9 | Nero | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge burning state retains its source-declared two-round duration; the weapon-hit ability remains eligible in subsequent rounds. |
| nesmith-8d3e34 | Nesmith | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap enemy ShieldMitigation (literal_false); bridge apex_barrier at combat start. |
| next-gen-crusher-0de02d | Next Gen Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge Health pool is represented; bridge captain-maneuver effect uses the existing proxy. |
| next-gen-data-5e7215 | Next Gen Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfBridge stat pool and bridge passive weapon damage are both represented. |
| next-gen-la-forge-ee6d76 | Next Gen La Forge | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| next-gen-riker-44ccee | Next Gen Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| next-gen-troi-ccf26b | Next Gen Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| nine-of-eleven-8c475b | Nine Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| nog-0c9672 | Nog | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| odo-04a97d | Odo | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| one-of-eleven-ee0ee9 | One Of Eleven | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| otto-cb0fb6 | Otto | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pan-13e04e | Pan | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| paul-stamets-4aa6ab | Paul Stamets | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge CombatStart enemy ArmorPiercing MultiplySub vs players while defending (862911683) is modeled as single-axis opponent base Armor Piercing pool reduction. |
| phlox-72c07b | Phlox | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge reduces enemy critical chance at combat start for eight rounds, excluding armadas and stations (symmetric on attacker and PvP defender crews). Below-deck Cutting Beam ability cost is non-combat. |
| pic-admiral-picard-5f2936 | PIC Admiral Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pic-beverly-crusher-26f56a | PIC Beverly Crusher | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| pic-hugh-75d78e | PIC Hugh | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| pic-riker-61815c | PIC Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pic-worf-0f1290 | PIC Worf | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pike-1e7d0d | Pike | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | Captain OffAbilityEffect scales bridge combat abilities by min(1, base×(1+X)) vs hostiles ≤70; SelfBridge only (not captain-seat bridge row). |
| planetfall-kirk-0def07 | Planetfall Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain Defensive Focus (+20% existing Polaron Mitigation on FKR/Vengeance vs FKR Hostiles) uses base 20% until native build synergy binding includes Update 96 group 55 (+10%/+40% synergy bonuses). Bridge Destructive Capacity (+2,000..+150,000 Infusion Capacitors) is modeled with full FKR/Vengeance + FKR Hostile gates. |
| planetfall-ortegas-408465 | Planetfall Ortegas | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge Exploit Target (+5,000..+500,000 Polaron Mitigation on FKR/Vengeance vs FKR Hostiles) and BD Infusion Leverage (+2,500..+25,000 Infusion Capacitors on FKR/Vengeance vs FKR Armadas) are both modeled. |
| planetfall-scotty-5a60c7 | Planetfall Scotty | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | No captain (Chain of Command). Bridge Infusion Capacitor Amplification (+10%..+100% Infusion Capacitor Spike on FKR/Vengeance vs FKR Hostiles) and BD Infusion Optimization (+2,500..+25,000 Infusion Capacitors on FKR/Vengeance vs Solo Outposts and Outpost Retaliation Attackers) are both modeled. |
| planetfall-uhura-f1ba9e | Planetfall Uhura | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | No captain (Chain of Command). Bridge Copious Capacitors (+750..+2,000 Infusion Capacitors on FKR/Vengeance vs FKR Hostiles) is modeled. BD Now We're Talking! (Polaron Ions loot) is economy → :non_combat. |
| pon-a2ddd4 | Pon | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain enemy_delay 3R at combat begin vs player defender on Explorer is gated on SelfDefending (inert on default attack path; active when attacker_is_defending is set or defending in PvP); bridge TimedHostileCritChanceReduction 3R at round start on Explorer with Morale is modeled. |
| quasi-bf8173 | Quasi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| rachel-garrett-4f15c8 | Rachel Garrett | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| reginald-barclay-111169 | Reginald Barclay | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| rima-26c9f4 | Rima | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ro-mudd-21a89a | Ro Mudd | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain SelfCaptain stat pool and bridge CombatStart AllDamage (MultiplyBaseAdd, Attack, rank-scaled chance) POST_ALL pre-crit flat weapon damage are both modeled. |
| rom-621ae3 | Rom | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain enemy_delay 1R at combat begin vs station (EnemySentinel + SelfAttacking); inactive vs default NPC hostiles. |
| romi-270f19 | Romi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| rose-harper-3b4b1d | Rose Harper | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | No captain (Chain of Command). Bridge Dilithium Reaver (Ra'Ul Reaver Sigma-Dilithium raid proportion) is economy → :non_combat. BD The Apparition (+400%..+1,000% Isolytic Defense) is gated on FKR ships or U.S.S. Vengeance vs players (EnemyPlayer). |
| rukor-9d7beb | Rukor | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| s31-georgiou-91b91d | S31 Georgiou | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| sam-rutherford-927c93 | Sam Rutherford | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| saru-68cf4f | Saru | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| scotty-a83cb5 | Scotty | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| sela-bd6e1b | Sela | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| sesha-631428 | Sesha | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| seska-848b5b | Seska | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | On-hit crit buff modeled as OnHitCritDamageStack: at most one 4-round stack per weapon, refreshed when that weapon hits again (guaranteed-hit engine → each firing weapon holds one stack); stacks multiply as (1+X)^active. Refresh-on-hit reading chosen over once-per-fight; both agree for fights ≤ 4 rounds. |
| seven-of-eleven-e45727 | Seven Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap shots_per_attack at combat start; bridge passive OfficerStatHealth. |
| seven-of-nine-d18a5e | Seven Of Nine | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| shaxs-11a808 | Shaxs | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| shev-1799cb | Shev | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| six-of-eleven-20cbe8 | Six Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge EnemyTakesHit AllPiercing vs Armada (3353791415, MultiplyBaseAdd, Attack) is modeled as cumulative post-volley PRE_ALL piercing; captain EnemyTakesHit enemy ShieldHPRepair MultiplyBaseSub (115416486, Defense) is modeled via unified op 6 post-volley flat enemy shield drain. |
| snw-d-chok-34f7ba | SNW D'Chok | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-hemmer-330aae | SNW Hemmer | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge CombatStart AllDefenses vs Survey hostiles (MultiplyBaseAdd, Health) is modeled via unified op 5 PRE_ALL defense; below-deck crew Health is represented. |
| snw-james-kirk-6f6300 | SNW James Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap enemy ShieldMitigation at combat start; bridge weapon_damage at combat start. |
| snw-mbenga-fe38e5 | Snw M’Benga | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-nurse-chapel-d80ed9 | SNW Nurse Chapel | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-ortegas-7c79fe | SNW Ortegas | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-pelia-9d33f1 | SNW Pelia | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-pike-c94ac4 | SNW Pike | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap self + bridge enemy ShieldMitigation at combat start (AND gates). |
| snw-sam-kirk-0a77f9 | SNW Sam Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain ShieldHPRepair MultiplySub vs non-Armada hostiles: LCARS enemy shield_regen sub at round_start → DefenderShieldDrainPerRound (fraction of defender max SHP each round). |
| snw-scotty-1dd4c3 | Snw Scotty | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-spock-40de97 | SNW Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-t-pring-16cf8d | SNW T'Pring | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-uhura-ff7333 | SNW Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-una-2b6e15 | SNW Una | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| soran-24a024 | Soran | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| specialist-seven-7b554a | Specialist Seven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| spock-c04738 | Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge RoundStart ShieldHPRepair while Morale active (MultiplyBaseAdd, Defense) is modeled as direct flat shield repair (zero ship shield-HP pool multiplication). |
| starfleet-q-3c61cb | Starfleet Q | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| strike-team-la-an-84e2a9 | Strike Team La'an | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| strike-team-ortegas-d9df30 | Strike Team Ortegas | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Bridge AllReloadSpeed vs player on BB: canonical trigger RoundStart (ability_id 1842949267); tooltip 'weapons shot by them' is flavor. Sim uses on_round_start enemy_delay 1R — matches upstream. |
| strike-team-una-5ec6f6 | Strike Team Una | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap OfficerStatAll on_round_start (PvP + defender Explorer) via Phase 4d; bridge enemy ShieldMitigation on_round_start (PvP + attacker BB). |
| suder-d348a9 | Suder | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Bridge HullRepair (2041170794, 12–24% of prior-round hull damage vs non-Armada hostiles) is modeled at round boundary before next-round shots (verified against 2026-11-26 Fight C Vi'Dar capture: +87,546.42 = 26% of 336,717 R1 hull damage); below-deck hostile critical-damage bonus retains its five-round combat-start duration. |
| sulu-fe562d | Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| t-laan-c4627b | T'Laan | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tasha-yar-b9300c | Tasha Yar | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| the-doctor-327fc3 | The Doctor | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Bridge HullDamageTaken AllDefenses (MultiplyBaseAdd, 2R, Health, EnemyHostile) is modeled as reactive PRE_ALL defense stacking per damaging volley. |
| the-hierarch-e1f430 | The Hierarch | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap shots_per_attack on_round_start; bridge AddRandomState vs player ships (PvP-only). |
| tmp-hikaru-sulu-c73326 | TMP Hikaru Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tmp-nyota-uhura-802f0c | TMP Nyota Uhura | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tom-paris-3640cc | Tom Paris | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | Below-decks CombatStart AllDefenses (3192273655, MultiplyBaseAdd, Defense, EnemyHostile) is modeled via the unified op 5 PRE_ALL crew-Defense formula: (ship_base_defense + coeff * effective_crew_defense) * (1 + launch_defense_pool + combat_start_defense_pool), verified against both the 2026-10-26 Realta captain swap (0.0000 pp gap) and 2026-11-26 Vi'Dar vs L57 capture (0.706674 vs 0.706690 observed). Requires saved officer levels; stays inert with diagnostics when crew levels are unknown. |
| tomalak-0ff09c | Tomalak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| toryn-818f4f | Toryn | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | No captain (Chain of Command). Bridge Tritanium Reaver (Ra'Ul Reaver Sigma-Tritanium raid proportion) is economy → :non_combat. BD Andorian Resolve (+2,500..+50,000 Critical Mitigation rating, diminishing returns R/(R+50,000)) is gated on EnemyPlayer. |
| tos-chekov-0f158d | TOS Chekov | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tos-kirk-bc6d1b | TOS Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-mccoy-fff2e0 | TOS McCoy | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain CombatStart AllDefenses while defending (MultiplyBaseAdd, Health, SelfDefending) is modeled via unified op 5 PRE_ALL defense; bridge crew stats retain the ship-type gate. |
| tos-sulu-0d02c3 | TOS Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-uhura-44419b | TOS Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain RoundStart TimedHostileCritChanceReduction (1R while Morale active vs players) and bridge enemy shield mitigation are both represented. |
| trelane-96b872 | Trelane | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain General of Gothos (+10% base shield mitigation, no synergy) and bridge Child's Play (+5,000..+50,000 Apex Barrier, disabled when Tal is on bridge) are both gated on Wave Defense (SelfAtWaveDefenseChallenge → engagement_includes: waves; inert on default ship-vs-hostile path). |
| trip-tucker-75d4f9 | Trip Tucker | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| uhura-ea117c | Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain shield-break enemy_delay: DefenderFireDelay skips defender counter-fire for N rounds; chance from rank scaling. |
| v-ger-ilia-25aa9f | V'Ger Ilia | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vartoq-9109e7 | Vartoq | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vel-f335b3 | Vel | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| vella-7ab77e | Vella | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap weapon_damage at combat start; bridge passive OfficerStatHealth. |
| vil-gul-dukat-c46cb8 | Vil Gul Dukat | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vixis-9eec06 | Vixis | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain enemy_delay 2R at round start; chance rank-scaled. |
| wesley-crusher-834fce | Wesley Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain CombatStart AllDamage vs hostiles (MultiplyBaseAdd, Attack) is modeled as POST_ALL pre-crit flat weapon damage; bridge SelfOfficer stat pool folds into effective crew Attack at combat start. |
| weyoun-e042c4 | Weyoun | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| william-t-riker-ddebb5 | William T. Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-carol-52a350 | Wok Carol | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| wok-joachim-858f5a | WOK Joachim | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-mccoy-f09b41 | Wok Mccoy | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-saavik-65f1bb | WOK Saavik | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-scotty-57ed85 | Wok Scotty | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| worf-c55d58 | Worf | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| woteln-c67650 | Woteln | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Cap weapon_damage at combat start; bridge passive ShipDodge (ship-type gate). |
| yan-agh-dd8637 | Yan'Agh | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| yuki-1ab97a | Yuki | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zahra-e3f002 | Zahra | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zefram-cochrane-f8a1c2 | Zefram Cochrane | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zeph-21ee5c | Zeph | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | AddRandomState vs NPC hostiles at round start: rank-scaled proc chance; weighted 8:4:2 Morale / Hull Breach / Burning on defender; defender Morale boosts counter-fire primary pierce (BB/INT). |
| zhou-5d4d4b | Zhou | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| alonzo-freeman-ef0f9b | Alonzo Freeman | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| arrock-0791b2 | Arrock | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| b-etor-8fc426 | B'Etor | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| dane-4f7fc1 | Dane | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| domitia-2e4a05 | Domitia | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| dupont-5deb80 | Dupont | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| eleven-of-eleven-ac168c | Eleven Of Eleven | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| fess-7afd43 | Fess | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| four-of-eleven-15084c | Four Of Eleven | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| ghalenar-69ddad | Ghalenar | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| ghrush-6e635b | Ghrush | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| hadley-ae14a1 | Hadley | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| helvia-70a338 | Helvia | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| hoshi-sato-3bc529 | Hoshi Sato | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| jirali-551c28 | Jirali | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | Entirely out-of-combat: captain Beam Augmenter (extra Borg Cube cutting-beam charge), bridge Tech Expert (cutting-beam loot), and BD Black Market Mechanic (repair-speed cascade) all map to :non_combat tags (no in-fight damage/hull/shield effect). |
| joaquin-697b4c | Joaquin | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| makinen-a124a0 | Mäkinen | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| mavery-60052a | Mavery | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| naga-delvos-aa4e10 | Naga Delvos | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | Ouroboros buffs Borg Cube Cutting Beam active ability — not modeled in combat sim (:non_combat). |
| phellun-c3188c | Phellun | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| quark-2fd57b | Quark | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| shran-f9ae44 | Shran | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| snw-la-an-abc92f | SNW La'an | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| stonn-95a1f6 | Stonn | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| t-pol-3164c1 | T'Pol | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| t-pring-9bade6 | T'Pring | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| ten-of-eleven-51fe63 | Ten Of Eleven | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| three-of-eleven-d5568f | Three Of Eleven | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| tos-scotty-c747f6 | TOS Scotty | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| tuvok-5ceab0 | Tuvok | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
| two-of-eleven-c4f4d6 | Two Of Eleven | 0 | 0/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | — | — | — | 0 | — | — | 100 | economy_only | — |
