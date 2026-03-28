# Tasks requiring human intervention

Work the simulator or data pipeline cannot complete automatically: judgment, upstream correlation, or in-game verification.

## Factions and hostiles

1. **Mapping of hostiles to factions using `translations-factions.json`**  
   Correlate upstream hostile `faction.id` / `faction.loca_id` with faction display names and game semantics in `data/upstream/data-stfc-space/translations-factions.json`, then extend `HostileRecord::opponent_faction_tag()` (in `src/data/hostile.rs`) so unmapped ids (e.g. Q-Continuum, Exiles, Card, Node, Texas-class) resolve to the correct [`OpponentFactionTag`](src/combat/types.rs) or stay explicitly `Unknown` with a documented reason.

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
