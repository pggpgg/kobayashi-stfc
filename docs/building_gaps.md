# Building bonus mapping gaps

Directory: `/Users/pgagnong/Dev/kobayashi-stfc/data/buildings`

## Opaque `buff_*` stats

These keys are not merged into the player combat profile (see `merge_building_bonuses_into_profile` / `normalize_profile_combat_stat` in `src/data/profile.rs`). Descriptions are from stfc.space / game translations (`starbase_module_buff_description`) matched via `loca_id` in each bonus’s `notes` field.

- Distinct opaque: **251**
- Allowlisted: **249**
- Still actionable: **2**

| Stat | Description | Building name(s) |
| --- | --- | --- |
| `buff_1350723279` | Increases the Apex Barrier of all G6+ FKR ships vs Players during Takeovers. | Continuum Consulate |
| `buff_4183662952` | Increases the number of points scored from Capture Nodes during a Takeover. | Continuum Consulate |

## Conditions not in `is_known_building_condition`

None.

