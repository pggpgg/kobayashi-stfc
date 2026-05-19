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
| 718-0-2509d7 | 718.0 | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ahvix-f90184 | Ahvix | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| airiam-9265fc | Airiam | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| alok-sahar-4d1370 | Alok Sahar | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| andy-billups-c27ba7 | Andy Billups | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| annorax-830d35 | Annorax | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| arix-b3d602 | Arix | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| arkady-94c81b | Arkady | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| azetbur-7eff22 | Azetbur | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| b-elanna-torres-75cf02 | B'Elanna Torres | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ba-el-91d122 | Ba'el | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| badgey-13df33 | Badgey | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| barot-2d7be0 | Barot | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| beckett-mariner-d93865 | Beckett Mariner | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| benjamin-sisko-5c51f2 | Benjamin Sisko | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| beverly-crusher-74b2d7 | Beverly Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| black-ops-chapel-814fe8 | Black Ops Chapel | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| black-ops-m-benga-e23e3a | Black Ops M'Benga | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| bones-ddc8a9 | Bones | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| borg-queen-c8e67d | Borg Queen | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| brad-boimler-ee5262 | Brad Boimler | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| byr-ch-kelrer-090eff | Byr Ch'Kelrer | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| cadet-kirk-a80563 | Cadet Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| cadet-mccoy-13d460 | Cadet McCoy | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| cadet-scotty-b342ae | Cadet Scotty | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| cadet-sulu-784421 | Cadet Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| cadet-uhura-3ef15c | Cadet Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| cap-tilly-070cc9 | Cap. Tilly | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| carol-755a05 | Carol | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| carol-freeman-a46be4 | Carol Freeman | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| cath-f0e149 | Cath | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| chakotay-a1f5df | Chakotay | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| chang-ecc238 | Chang | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Bridge enemy_delay on crit (on_critical) when defender hull breach active; DefenderFireDelay skips counter-fire. Captain weapon bonus uses hull_hp <60% gate. |
| changeling-kira-666ebe | Changeling Kira | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| charvanek-0f1b5c | Charvanek | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| chen-cdb1ca | Chen | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| culber-b3e4a0 | Culber | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| d-jaoki-baa18f | D'Jaoki | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| d-vana-tendi-9fabf0 | D'Vana Tendi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| dajash-tolra-1e809f | Dajash Tolra | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| damar-796eb0 | Damar | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| darwin-b8dc0a | Darwin | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| data-d20ef8 | Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| deanna-troi-57341d | Deanna Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| decius-8fce68 | Decius | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| demarco-7f2d86 | Demarco | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| dezoc-381416 | Dezoc | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| dixon-hill-b7ea10 | Dixon Hill | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| doctor-t-ana-b98f82 | Doctor T'Ana | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | AddRandomState vs player defenders only (PvP); same weighted states and rank proc as Zeph; inactive in default hostile optimize. |
| eight-of-eleven-cb5f3c | Eight Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| emp-georgiou-1564b5 | Emp. Georgiou | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ent-e-data-871245 | Ent-E Data | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Isolytic cascade line OK for combat start; missing non-Armada hostile filter in LCARS. |
| ent-e-picard-556227 | Ent-E Picard | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| ent-e-riker-516b8d | Ent-E Riker | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| ent-e-troi-46cdc3 | Ent-E Troi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| eurydice-18b643 | Eurydice | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ezri-dax-bb0892 | Ezri Dax | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| fcm-data-3b9e4d | FCM Data | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| five-of-eleven-d9aa11 | Five Of Eleven | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| french-marshal-q-1f0a28 | French Marshal Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| gaila-3d387a | Gaila | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| garak-771862 | Garak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| geordi-la-forge-914cec | Geordi La Forge | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| georgiou-d2bdef | Georgiou | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| gonzales-de640c | Gonzales | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| goon-891942 | Goon | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| gorkon-b8d2e7 | Gorkon | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain crit window OK; bridge hull breach proc is correctly modeled as hull_breach effect with chance scaling. |
| gossa-dafefb | Gossa | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| gowron-27ac30 | Gowron | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| gul-dukat-70e9d7 | Gul Dukat | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| harrison-56cc6c | Harrison | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain: Explorer hostile gate modeled in LCARS; level-70 unlock gated via defender_level_at_most condition; bridge Sabotage uses ShieldMitigationBypassFraction on defender mitigation for round 1 only (duration→RoundRange). |
| harry-kim-a79fdf | Harry Kim | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| harry-mudd-374d5f | Harry Mudd | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| hendorff-549c65 | Hendorff | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| honorguard-worf-8ac58c | Honorguard Worf | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| hugh-9fc348 | Hugh | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| icheb-fb3bd7 | Icheb | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ikatika-4fa1ba | Ikat’Ika | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| infiltrator-tuvok-5d3048 | Infiltrator Tuvok | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| instr-spock-bba5d7 | Instr. Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| jack-ransom-dfdb38 | Jack Ransom | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| jadzia-dax-736698 | Jadzia Dax | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| javaid-fdff59 | Javaid | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| jaylah-857412 | Jaylah | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| jean-luc-picard-e0515c | Jean-Luc Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain off-ability boost is tag/non_combat path (ignored in sim); bridge crit damage roughly OK. Not an officer-stat-scaling gap. |
| joachim-03587e | Joachim | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| jonathan-archer-4e9cd0 | Jonathan Archer | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| julian-bashir-0551fa | Julian Bashir | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| k-bisch-b6c84c | K'Bisch | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| kang-55e67a | Kang | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| kathryn-janeway-bd4a19 | Kathryn Janeway | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| kati-01ab4d | Kati | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| keenser-3a5ad4 | Keenser | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| kerla-6cdf45 | Kerla | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| khan-3f1d1e | Khan | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| kira-nerys-a5253a | Kira Nerys | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| kirk-1323b6 | Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| klaa-acbd92 | Klaa | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| komal-357fb2 | Komal | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| koth-c70d1c | Koth | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| kras-a47042 | Kras | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | CptManeuverEffect + enemy OfficerStatAll are PvP-gated (EnemyPlayer); inactive vs NPC hostiles. |
| krell-ef559b | Krell | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| kumak-c5b0db | Kumak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| kuron-15cda2 | Kuron | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | Captain self_recharge at combat start modeled as +100% shots round 1 (ShotsBonus proxy). ModuleKinetic gate is lenient (all weapons) — no weapon-type dimension in engine. |
| l-nar-ae14f4 | L'Nar | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| laliari-87e81a | Laliari | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | Omega-13 cooldown reduction on ship active ability — not modeled in combat sim (:non_combat). |
| leslie-975ce0 | Leslie | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| lieutenant-picard-6303e7 | Lieutenant Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| linkasa-01a1b3 | Linkasa | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| livis-43235e | Livis | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| lorca-d32ec8 | Lorca | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| lursa-57a544 | Lursa | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Cloaking cooldown on ship active ability — not modeled in combat sim (:non_combat). |
| m-benga-53446d | M'Benga | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| m-ral-986e7a | M'Ral | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mara-bd3ca6 | Mara | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| mara-dalen-6827 | Mara Dalen | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| marcus-073931 | Marcus | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| mariachi-q-c8cf6e | Mariachi Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| marla-9732c7 | Marla | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| martok-89ef39 | Martok | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| masriad-vael-50cf64 | Masriad Vael | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| michael-burnham-6c711c | Michael Burnham | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| miles-o-brien-f0c92f | Miles O'Brien | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| mirek-953093 | Mirek | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-data-e13d66 | Mirror Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-ezri-2f6326 | Mirror Ezri | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-kira-bc42ce | Mirror Kira | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-picard-7c1a17 | Mirror Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-troi-d237df | Mirror Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mirror-uhura-0a7168 | Mirror Uhura | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| miss-q-cc911e | Miss Q | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mitchell-0217f7 | Mitchell | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| moreau-23ddb4 | Moreau | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| mudd-32546a | Mudd | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| navi-0b328a | Navi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| neelix-c8a380 | Neelix | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | Voyager active-ability cost efficiency — not modeled in combat sim (:non_combat). |
| nero-1aeca9 | Nero | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| nesmith-8d3e34 | Nesmith | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| next-gen-crusher-0de02d | Next Gen Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| next-gen-data-5e7215 | Next Gen Data | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| next-gen-la-forge-ee6d76 | Next Gen La Forge | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| next-gen-riker-44ccee | Next Gen Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| next-gen-troi-ccf26b | Next Gen Troi | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| nine-of-eleven-8c475b | Nine Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| nog-0c9672 | Nog | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| odo-04a97d | Odo | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| one-of-eleven-ee0ee9 | One Of Eleven | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| origins-burnham-e854d6 | Origins Burnham | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| origins-saru-753b24 | Origins Saru | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| origins-stamets-b0decf | Origins Stamets | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| otto-cb0fb6 | Otto | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pan-13e04e | Pan | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| paul-stamets-4aa6ab | Paul Stamets | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| phlox-72c07b | Phlox | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | Cutting Beam ability cost on below decks — not modeled in combat sim (:non_combat). |
| pic-admiral-picard-5f2936 | PIC Admiral Picard | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pic-beverly-crusher-26f56a | PIC Beverly Crusher | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| pic-hugh-75d78e | PIC Hugh | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| pic-riker-61815c | PIC Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pic-worf-0f1290 | PIC Worf | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| pike-1e7d0d | Pike | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 50 | mixed | Captain OffAbilityEffect scales bridge combat abilities by min(1, base×(1+X)) vs hostiles ≤70; SelfBridge only (not captain-seat bridge row). |
| pon-a2ddd4 | Pon | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain enemy_delay 3R at combat begin; gated SelfExplorer + EnemyPlayer + SelfDefending — inactive in default hostile optimize. |
| qa-ug-a165bf | Qa'Ug | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| quasi-bf8173 | Quasi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| rachel-garrett-4f15c8 | Rachel Garrett | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| reginald-barclay-111169 | Reginald Barclay | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| rima-26c9f4 | Rima | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| ro-mudd-21a89a | Ro Mudd | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| rom-621ae3 | Rom | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain enemy_delay 1R at combat begin vs station (EnemySentinel + SelfAttacking); inactive vs default NPC hostiles. |
| romi-270f19 | Romi | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| rukor-9d7beb | Rukor | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| s31-georgiou-91b91d | S31 Georgiou | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| sam-rutherford-927c93 | Sam Rutherford | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| saru-68cf4f | Saru | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| scotty-a83cb5 | Scotty | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| sela-bd6e1b | Sela | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| sesha-631428 | Sesha | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| seska-848b5b | Seska | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | On-hit crit buff now limited to rounds 1-4 via round_range condition; once-per-weapon gating requires engine refinement. |
| seven-of-eleven-e45727 | Seven Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| seven-of-nine-d18a5e | Seven Of Nine | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| severus-93daaf | Severus | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| shaxs-11a808 | Shaxs | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| shev-1799cb | Shev | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| six-of-eleven-20cbe8 | Six Of Eleven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-d-chok-34f7ba | SNW D'Chok | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-hemmer-330aae | SNW Hemmer | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| snw-james-kirk-6f6300 | SNW James Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| snw-mbenga-fe38e5 | Snw M’Benga | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-nurse-chapel-d80ed9 | SNW Nurse Chapel | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-ortegas-7c79fe | SNW Ortegas | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-pelia-9d33f1 | SNW Pelia | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-pike-c94ac4 | SNW Pike | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| snw-sam-kirk-0a77f9 | SNW Sam Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | Captain enemy SHP drain modeled as negative shield_regen on attacker crew — requires engine changes for enemy-targeted shield drain. |
| snw-scotty-1dd4c3 | Snw Scotty | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-spock-40de97 | SNW Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-t-pring-16cf8d | SNW T'Pring | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| snw-uhura-ff7333 | SNW Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| snw-una-2b6e15 | SNW Una | 1 | 1/0/0 | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | — | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| soran-24a024 | Soran | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| specialist-seven-7b554a | Specialist Seven | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| spock-c04738 | Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| starfleet-q-3c61cb | Starfleet Q | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| strike-team-la-an-84e2a9 | Strike Team La'an | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| strike-team-ortegas-d9df30 | Strike Team Ortegas | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Bridge enemy_delay at round start vs player defender on BB; upstream trigger is RoundStart (text says on your shot — verify in-game). |
| strike-team-una-5ec6f6 | Strike Team Una | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| suder-d348a9 | Suder | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| sulu-fe562d | Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| t-laan-c4627b | T'Laan | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tal-c3e4eb | Tal | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tasha-yar-b9300c | Tasha Yar | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| the-doctor-327fc3 | The Doctor | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| the-hierarch-e1f430 | The Hierarch | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| tilly-6bd08f | Tilly | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tiza-9d38f9 | Tiza | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| tmp-hikaru-sulu-c73326 | TMP Hikaru Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tmp-nyota-uhura-802f0c | TMP Nyota Uhura | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| toli-2d704a | Toli | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tom-paris-3640cc | Tom Paris | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tomalak-0ff09c | Tomalak | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-chekov-0f158d | TOS Chekov | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tos-kirk-bc6d1b | TOS Kirk | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-mccoy-fff2e0 | TOS McCoy | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| tos-spock-86f176 | TOS Spock | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-sulu-0d02c3 | TOS Sulu | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| tos-uhura-44419b | TOS Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| trip-tucker-75d4f9 | Trip Tucker | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| tyler-1dcc4d | Tyler | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| uhura-ea117c | Uhura | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain shield-break enemy_delay: DefenderFireDelay skips defender counter-fire for N rounds; chance from rank scaling. |
| vartoq-9109e7 | Vartoq | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vel-f335b3 | Vel | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| vella-7ab77e | Vella | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| vemet-00a218 | Vemet | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vil-gul-dukat-c46cb8 | Vil Gul Dukat | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| vil-winn-adami-6c42f3 | Vil Winn Adami | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| vixis-9eec06 | Vixis | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | Captain enemy_delay 2R at round start; chance rank-scaled. |
| wesley-crusher-834fce | Wesley Crusher | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| weyoun-e042c4 | Weyoun | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| william-t-riker-ddebb5 | William T. Riker | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-carol-52a350 | Wok Carol | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| wok-joachim-858f5a | WOK Joachim | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-mccoy-f09b41 | Wok Mccoy | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-saavik-65f1bb | WOK Saavik | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| wok-scotty-57ed85 | Wok Scotty | 1 | 0/0/0 | 0/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | — | 100 | 100 | 100 | 0 | 100 | A | 50 | mixed | — |
| worf-c55d58 | Worf | 1 | 0/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | — | 100 | 100 | 0 | 100 | A | 100 | economy_only | — |
| woteln-c67650 | Woteln | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | — |
| yan-agh-dd8637 | Yan'Agh | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| yuki-1ab97a | Yuki | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zahra-e3f002 | Zahra | 2 | 1/0/0 | 1/0/0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 100 | 100 | — | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zefram-cochrane-f8a1c2 | Zefram Cochrane | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 100 | none | — |
| zeph-21ee5c | Zeph | 2 | 0/0/0 | 1/0/0 | 1/0/0 | 0 | 0 | 0 | 0 | 0 | — | 100 | 100 | 100 | 100 | 0 | 100 | A | 0 | combat_tag_gaps | AddRandomState vs NPC hostiles at round start: rank-scaled proc chance; weighted 8:4:2 Morale / Hull Breach / Burning on defender; defender Morale boosts counter-fire primary pierce (BB/INT). |
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
