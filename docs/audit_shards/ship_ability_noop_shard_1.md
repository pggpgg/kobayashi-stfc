# Ship ability `combat_noop` audit — shard 1

Track D batch. 10 ability ids.

> **Status note (historical snapshot):** some rows below marked `extend_resolver` are now **modeled** — see [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](../SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) §6.1 (e.g. the Quv'Sompek hostile-debuff ability, modeled in D2). These shards are a frozen Track-D snapshot and are not auto-regenerated.

| id | ships | loca_text (excerpt) | current_bucket | recommendation | engine_touch | evidence | test_plan | in_game_verify |
|---:|---|---|---|---|---|---|---|---|
| 34867572 | 34867572 | enhanced hull
the maximum hull health of the orion corvette is increased by {0}.

ship abilities are always active. | Stat — max hull / shield | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 49906243 | 49906243 | modernity is overrated 

the monaveen increases its base damage by +{0:#.#%} against hostiles with the texas-class [dal] tag.

ship abilitie | Opponent tag / special faction | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 78080222 | 442815157 | anti-tachyon weaponry
increases base damage against krenim invading entities by {0:#.#%} | Opponent tag / special faction | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 87414807 | 87414807 | gas mining laser
the mining rate of gas is increased by {0:#.#%}.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 108924704 | 108924704 | gas mining laser
the mining rate of gas is increased by {0:#.#%}.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 293385368 | 293385368 | master thief

the stella increases the reward you get from eclipse hostiles and armada targets by {0:#.#%}

ship abilities are always active | Economy / other (review) | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 509252162 | 2251018025 | omicron particle charge
increases base damage against conqueror borg suppressors and obliterators by {0:#.#%}.

ship abilities are always ac | Reclassified | reclassify_catalog | none | Now `attack_multiplier` — remove from noop §1 inventory. | — | yes |
| 546190599 | 546190599 | ore mining laser
the mining rate of ore is increased by {0:#.#%}. 

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 673187302 | 673187302 | territorial
when defending , the centurion increases the armor, shield deflection and dodge of all ships and defense platforms by {0:#.#%}.
 | Scope — defending / station / allies | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 701705952 | 701705952 | intimidating presence
when fighting hostiles, the quv'sompek decreases hostile armor piercing, shield piercing, and accuracy by {0:#.#%} for | Hostile debuffs / shield drain | extend_resolver | ship_ability_resolve.rs | Classifier NOOP; timing=combat_begin. | `cargo test -p kobayashi ship_ability_resolve::` | yes |
