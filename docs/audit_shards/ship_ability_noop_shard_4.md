# Ship ability `combat_noop` audit — shard 4

Track D batch. 10 ability ids.

| id | ships | loca_text (excerpt) | current_bucket | recommendation | engine_touch | evidence | test_plan | in_game_verify |
|---:|---|---|---|---|---|---|---|---|
| 1492898704 | 1492898704 | combat scavenger
after winning a battle, if the target has more resources than the amalgam's available cargo space, the amalgam fills its ca | Economy — loot / progression | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1535317053 | 1535317053 | ore mining laser
the mining rate of ore is increased by {0:#.#%}.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1577508895 | 1577508895 | gas mining laser
the mining rate of gas is increased by {0:#.#%}. 

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1738424547 | 1738424547 | feels like home

the u.s.s. voyager increases its base damage by +{0:#.#%} against hostiles with the delta quadrant [dq] tag.

ship abilitie | Opponent tag / special faction | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1784814733 | 1784814733 | latinum mining

the mining bonus from the mining laser is increased by  {0:#.#%} when mining raw latinum.

ship abilities are always active. | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1823660918 | 1823660918 | phase discriminating
the borg cutting beam deals {0:#,##0} base hhp damage to non-armada hostiles outside of battle. this value is reduced a | Scope — armada | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1839370465 | 1839370465 | radiation resistance
increases radiation resistance by {0:#.#%}

ship abilities are always active. | Economy — hazards / resistances | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1878809713 | 1878809713 | isogen mining
the meridian's mining bonus from the mining laser is increased by {0:#.#%} when mining isogen.

ship abilities are always acti | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1972093910 | 1328894295 | reliant payoff
the u.s.s. reliant increases hijacked splicers gained from augment exile [wok] hostiles in augment exile space by {0:#.#%}.

 | Economy — loot / progression | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
| 1982797639 | 1982797639 | universal mining laser
when mining 5* crystal, gas and ore, the nova's mining speed is increased by {0:#####%}

ship abilities are always ac | Economy — mining / materials | keep_noop | none | Classifier NOOP; timing=combat_begin. | — | no |
