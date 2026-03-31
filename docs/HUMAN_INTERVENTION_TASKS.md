# Tasks requiring human intervention

Work the simulator or data pipeline cannot complete automatically: judgment, upstream correlation, or in-game verification.

## Factions and hostiles

1. **Mapping of hostiles to factions using `translations-factions.json`** — **ongoing in code; see `src/data/hostile.rs`**  
   `HostileRecord::opponent_faction_tag()` maps high-volume `faction.id` values from `summary-hostile` and `faction.loca_id` rows that match `translations-factions.json` `faction_name` (e.g. Texas-class → Federation, “Card” → Cardassian, Borg alt loca, V’Ger Clone → Borg). **Intentionally `Unknown`:** Q-Continuum, Exiles, Node, Maverick, Orion, Eclipse, Krenim, Apex Raiders, Transogen, Aggregation (no `OpponentFactionTag` / not modeled for hull faction gates). When the game adds a new `faction.id`, extend the match arms in `hostile.rs` and add a unit test.

## Ship ability catalog (heuristic)

2. **Review `scripts/generate_full_ship_ability_catalog.py` classifications**  
   Ability text that is ambiguous, conditional on mechanics we do not model, or uses non-standard wording may need manual catalog rows or script rule updates after regeneration.

## Recorded fights and CLI

3. **Defender faction for imports / calibration**  
   `simulate_combat` still uses `OpponentFactionTag::Unknown`. If recorded fights or CLI runs should honor faction-gated hull abilities, a human must decide how to supply faction (export metadata, hostile id lookup, or explicit flag) and wire it to `simulate_combat_with_defender_faction`.

## Research / CI environment

4. **Broad research catalog for integration tests**  
   `tests/scenario_research_integration_tests.rs` expects a populated `data/research_catalog.json` (see test message / `scripts/import_stfcspace_research.mjs`). Filling or refreshing that data is an operational/data task, not a code change.

---

*Add new bullets here when you discover work that needs a person, not just a patch.*
