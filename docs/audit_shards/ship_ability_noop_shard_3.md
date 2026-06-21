# Ship ability `combat_noop` audit — shard 3

Track D batch. 10 ability ids.

> **Status note (historical snapshot):** some rows below marked `extend_resolver` are now **modeled** — see [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](../SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) §6.1 (e.g. the Sanctus hostile-debuff ability, modeled in D2). These shards are a frozen Track-D snapshot and are not auto-regenerated.

| id | ships | loca_text (excerpt) | current_bucket | recommendation | engine_touch | evidence | test_plan | in_game_verify |
|---:|---|---|---|---|---|---|---|---|
| 1029262994 | 1029262994 | healthy mining

the mining rate of 3★ gas, ore and crystal is increased by {0:#.#%} per total officer health point of all officers on the sh | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1087128295 | 1087128295 | data mining

the botany bay's mining bonus from the mining laser is increased by  {0:#.#%} when mining corrupted data and decoded data.

shi | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1090374551 | 1773930736 | marked for disposal
increases base damage against apex raiders (solo wave defense) hostiles by {0:#.#%}. | Opponent tag / special faction | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1160666017 | 442815157 | relative growth
increases base encrypted intelligence gained by {0:#,#%} when fighting krenim invading entities or armadas | Economy — loot / progression | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1244824002 | 1244824002 | offense is the best defense
when defending,at the start of each round,  the k't'inga increases the damage of all ships and defense platforms | Scope — defending / station / allies | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1307832955 | 1307832955 | mycelium harvesting
the u.s.s. discovery's mycelium harvesting speed is increased by {0:#.#%}.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1379978713 | 1379978713 | drain of the empire
when fighting hostiles, the sanctus decreases hostile shield health by {0:#.#%} at the beginning of each round for the f | Hostile debuffs / shield drain | extend_resolver | ship_ability_resolve.rs | Classifier NOOP; timing=combat_begin. | `cargo test -p kobayashi ship_ability_resolve::` | yes |
| 1428543762 | 1428543762 | ore mining laser
the mining rate of ore is increased by {0:#.#%}. 

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1439253182 | 1439253182 | all mine!

+{0:0#%} crysal, gas, and ore mining speed.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1463338054 | 1463338054 | frontline defender
when fighting hostiles, the u.s.s. intrepid increases its  armor, shield deflection and dodge by {0:#.#%}.

ship abilitie | Self defensive stats vs hostiles | extend_resolver | ship_ability_resolve.rs | Classifier NOOP; timing=combat_begin. | `cargo test -p kobayashi ship_ability_resolve::` | yes |
