# Track D — in-game verification checklist (HiggsBozo)

Track D2 ship hull abilities are modeled from **stfc.space catalog + simulator tests**, not from owner in-game fights.

## Status: in-game verify blocked (ship ownership)

These hull abilities sit on ships **not in the maintainer roster** (no B'Rel, Sanctus, Quv'Sompek, or U.S.S. Intrepid). Tooltip checks and live fights are **not available** here until:

- those ships are owned and tiered in-game, or
- someone else supplies a fight log / recorded export with ship id + tier + hostile, or
- community tooltip text is correlated and accepted as sufficient (still label as **assumed**, not verified).

Until then, treat **catalog + `ship_ability_resolve.rs` + integration tests** as the acceptance bar. Do not block merges on empty “Observed” cells below.

**Regression tests:** [`tests/ship_ability_hostile_debuff.rs`](../tests/ship_ability_hostile_debuff.rs), [`src/data/ship_ability_resolve.rs`](../src/data/ship_ability_resolve.rs) unit tests.

**Catalog source:** `data/upstream/data-stfc-space/ship_ability_catalog.json` (merged with `ship_ability_catalog_overrides.json`).

| id | Ship | What to verify in-game (when possible) | Catalog / sim (2026-05-20) | In-game status |
| --- | --- | --- | --- | --- |
| `2441576367` | B'Rel — **Obfuscation** | Opponent **Armor Piercing, Shield Piercing, and Accuracy** reduced **15%** (tier scales in upstream, e.g. 15→17%); **first round only**; vs **Hostiles** | `hostile_counter_stat_debuff`, combat_begin, `duration_rounds: 1`, `value_is_percentage: true` (upstream `ships/2441576367.json` → 0.15 base) | **Tooltip confirmed** (in-game text / UI, ship not owned). Sim: single `reduction` on **counter-fire pierce** only — proxy for all three stats (see § Simulator proxies). Live fight: N/A |
| `1379978713` | Sanctus | Shield drain % of max per round; 5-round cap at tier | `defender_shield_drain_per_round`, round_start, `duration_rounds: 5`, `value_is_percentage: true` | **Blocked** — ship not owned |
| `701705952` | Quv'Sompek | 5-round pierce/accuracy debuff magnitude vs tooltip tier | `hostile_counter_stat_debuff`, combat_begin, `duration_rounds: 5` | **Blocked** — ship not owned |
| `1463338054` | U.S.S. Intrepid | Dodge/deflection/armor stack with officer buffs | `hostile_engagement_defensive`, combat_begin, single fraction → mitigation **and** dodge proxy | **Blocked** — ship not owned |
| `509252162` | (reclassified) | Attack multiplier still applies in optimize | `attack_multiplier` at combat_begin — exercisable in sim without owning hull | Sim-only OK |
| `2425475474` | (reclassified) | Conqueror borg beam suppression vs Conqueror Borg | `conqueror_borg_beam_suppression` — beam disable marker | Sim-only OK |

## Simulator proxies (documented assumptions)

These are **intentional approximations** until in-game data exists:

- **B'Rel Obfuscation (`2441576367`):** Game text debuffs opponent armor pierce, shield pierce, and accuracy for round 1. Catalog value 15% (`0.15`) matches tooltip at low tier. Engine applies `HostileCounterStatDebuff` as `counter_pierce × (1 − reduction)` on the defender counter-attack path for round 1 only — **one multiplier**, not three separate pierce legs. Good enough for “all three down ~15%” on counter fire until we split pierce components in the engine.
- **Quv'Sompek:** Same `HostileCounterStatDebuff` path, 5 rounds (tooltip not recorded here).
- **Sanctus:** `DefenderShieldDrainPerRound` removes `fraction × defender.shield_health` (max SHP at fight start) each round for `duration_rounds`.
- **Intrepid:** One catalog fraction applied to both counter-fire mitigation bonus and dodge bonus sums at combat begin.

## If you later own a ship or get a fight log

1. Fill a row: observed tooltip %, rounds, and whether sim behavior matches.
2. If mismatch: tune `ship_ability_catalog.json` / overrides, then `ship_ability_resolve.rs`; add or tighten a row in `tests/ship_ability_hostile_debuff.rs`.
3. Optional note in `data/officers/officer_modeling_fidelity.yaml` only if officer-adjacent; hull abilities usually stay in this file.

## Alternative evidence (no ship required)

- **Upstream:** `data/upstream/data-stfc-space/ships/<id>.json` ability text + `translations-ship_buffs.json` via `loca_id`.
- **Recorded fight:** drop export under `tests/fixtures/recorded_fights/` and wire a drift or calibration slice (see [combat_log_format.md](combat_log_format.md)).
- **Another player:** fight sample with ship id in metadata is enough; ownership not required for the repo, only for your own client checks.
