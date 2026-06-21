# Ship ability `combat_noop` audit — shard 5

Track D batch. 10 ability ids.

> **Status note (historical snapshot):** some rows below marked `extend_resolver` are now **modeled** — see [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](../SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) §6.1 (e.g. the B'Rel first-round hostile-debuff ability, modeled in D2). These shards are a frozen Track-D snapshot and are not auto-regenerated.

| id | ships | loca_text (excerpt) | current_bucket | recommendation | engine_touch | evidence | test_plan | in_game_verify |
|---:|---|---|---|---|---|---|---|---|
| 2004925834 | 2004925834 | revenge
when defending the station, the bortas increases the damage of all the other ships and defense platforms by {0:#.#%}.

ship abilitie | Scope — defending / station / allies | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2057434885 | 2057434885 | confiscate evidence
the u.s.s. newton gains {0:#.#%} more resources from hostiles.

ship abilities are always active. | Economy / other (review) | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2195955652 | 2195955652 | bird of prey
as long as the opponent has a hull breach, every time the rotarran deals a critical hit with a weapon attack, it increases the  | Proc chains | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2254702328 | 2254702328 | empire tax
the pilum gains {0:#.#%} more resources from hostiles.

ship abilities are always active. | Economy / other (review) | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2302150828 | 4203458856 | collective's bane
disables the cutting beam weapon on the borg type 03 and borg polygon armadas from firing at the player | Weapon / mechanic disable | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2425475474 | 2251018025 | quantum nullification pulse
disables the resonance beam abilities of conqueror borg suppressors and obliterators.

ship abilities are always | Reclassified | reclassify_catalog | none | Now `conqueror_borg_beam_suppression` — remove from noop §1 inventory. | — | yes |
| 2441576367 | 2441576367 | obfuscation
when fighting hostiles, for the first round of combat, the b'rel decreases the opponent's ship armor piercing, shield piercing a | Hostile debuffs / shield drain | extend_resolver | ship_ability_resolve.rs | Classifier NOOP; timing=combat_begin. | `cargo test -p kobayashi ship_ability_resolve::` | yes |
| 2468986074 | 2468986074 | universal mining laser
when mining 6⇵ and below gas, crystal, and ore, the selkie's mining speed is increased by {0:#####%}.

research adapt | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2474117534 | 2474117534 | ion storm resistance
increases ion storm resistance by {0:#.#%}

ship abilities are always active. | Economy — hazards / resistances | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 2520552521 | 697653604 | black flag
increases the quantity of transogen manipulators looted from suliban stealth cruisers by {0:#.#%}. | Economy — loot / progression | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
