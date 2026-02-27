# Combat data (ships, hostiles, buildings)

Ship and hostile stats are loaded from `ships/index.json` and `hostiles/index.json` plus per-id JSON files in the same directories.
Building bonuses are loaded from `buildings/index.json` plus per-building JSON files.

## Provenance

- **Index files** include optional `data_version` and `source_note` (e.g. `"data_version": "stfccommunity-main"`, `"source_note": "STFCcommunity baseline (outdated ~3y)"`). These document where the data came from and help detect drift.
- **Source:** Data is typically produced by the normalizer from STFCcommunity or other community sources. See the normalizer and `data_version` / `source_note` in each index for the current baseline.
- **Validation:** Run the test suite; `data_provenance_and_validation` (or similar) tests that indexes load and optional version fields are present. For manual checks, compare a subset of ship/hostile stats to a known source (e.g. toolbox or game).

## Schema

- **Ships:** See `src/data/ship.rs` (`ShipRecord`). Fields include attack, hull_health, shield_health, shield_mitigation, apex_shred, isolytic_damage, etc.
- **Hostiles:** See `src/data/hostile.rs` (`HostileRecord`). Fields include armor, shield_deflection, dodge, hull_health, shield_health, shield_mitigation, apex_barrier, isolytic_defense, etc.
- **Buildings:** See `src/data/building.rs` (`BuildingRecord`). Each building has `levels` with `bonuses` (`stat`, `value`, `operator`, optional `conditions`/`notes`). Index is `data/buildings/index.json` (`BuildingIndex`).
