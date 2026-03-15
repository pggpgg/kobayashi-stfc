# Mapping buff IDs to stat names

1. **Resolve loca_id from your data**  
   Buffs are referenced as `buff_<id>` (e.g. `buff_3154267878`). In this repo, processed building files under `data/buildings/` list bonuses with `"stat": "buff_<id>"` and store the translation id in the bonus `notes` as `loca_id=<number>` (e.g. `loca_id=44100`). Parse that number.

2. **Look up the loca_id in translation JSON**  
   Under `data/upstream/data-stfc-space/`, translation files are arrays of `{"id", "key", "text"}`. Search for `id === loca_id`. Building buffs are in `translations-starbase_modules.json`; other buffs may live in `translations-officer_buffs.json`, `translations-ship_buffs.json`, or similar.

3. **Use the right key**  
   For building/starbase buffs, entries with the same `id` can have different `key` values; use the one that holds the name (e.g. `starbase_module_buff_name`) or description (e.g. `starbase_module_buff_description`). The `text` field is the human-readable stat name or description.

**Fallback:** If you only have the numeric buff id (e.g. `3154267878`) and no processed file with `loca_id`, search `summary-building.json` for `"id": <buff_id>` to see which building and level list it; the game uses separate loca tables per context, so you may need to correlate with that building’s loca ids from another source.

**Direct mapping (Kobayashi import):** To normalize combat buffs at import time, add entries to `data/buildings/buff_id_to_stat.json` with format `{ "<buff_id>": "<stat_name>" }`. Stat names must be engine keys: `weapon_damage`, `hull_hp`, `shield_hp`, `crit_chance`, `crit_damage`, `pierce`, `shield_mitigation`, `armor`, `dodge`, `damage_reduction`. The import script merges this file into common combat buff normalization so those bonuses are emitted with the stat name instead of `buff_<id>`.
