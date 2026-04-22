# Upstream hostile `ship_type` (reference)

data.stfc.space hostile JSON exposes `ship_type` as a **u32 category enum** that is separate from `hull_type`. Upstream `**hull_type`** maps to Kobayashi `ship_class` via `[hostile_hull_type_raw_to_ship_class](../src/data/hostile.rs)` (player ships: `[player_hull_type_raw_to_ship_class](../src/data/hostile.rs)` / `scripts/normalize_stfcspace_ships.mjs`; split so NPC rules can diverge). That drives hull-line `[ShipType](../src/combat/types.rs)` except where `[upstream_hostile_ship_type_profile](../src/data/upstream_hostile_ship_type.rs)` overrides (e.g. armada). The normalized **category** field is `upstream_ship_type` on `[HostileRecord](../src/data/hostile.rs)`.

Combat override today lives only in `[upstream_hostile_ship_type_profile](../src/data/upstream_hostile_ship_type.rs)`: value **1** sets `is_armada_target` so `[HostileRecord::ship_type_for_combat](../src/data/hostile.rs)` returns `[ShipType::Armada](../src/combat/types.rs)` even when hull-derived class would differ. All other values fall back to hull-derived class until reverse engineering proves otherwise.

## Snapshot and row counts

The table below reflects `**data/hostiles/index.json`** at data version `**stfcspace-hostiles-2026-04-20`** (same distinct set appears in `data/upstream/data-stfc-space/summary-hostile.json` for this checkout).

CI enforces that every per-hostile record’s `upstream_ship_type` is either listed in `KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES` in [`upstream_hostile_ship_type.rs`](../src/data/upstream_hostile_ship_type.rs) or explicitly deferred there with a maintainer reason; run `cargo run --bin validate_data` locally after a hostile refresh.


| `upstream_ship_type` | Hostile rows (index) | Inferred category / label                                            | Evidence                                                                                                                                                                                                                                                | Kobayashi combat effect today                       |
| -------------------- | -------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| 0                    | 3                    | Special / placeholder bucket                                         | Only three rows; sample `loca_id` 12320 → `officer_name` **Antaak Warship** (`translations-officer_names.json`).                                                                                                                                        | Hull-derived `ShipType` only (no profile override). |
| 1                    | 598                  | Armada target                                                        | `translations-navigation.json` key `armada_target_label` → **ARMADA TARGET**; hull `hull_type` 5 → survey in data; in `summary-hostile.json`, many rows with `ship_type` 1 set `is_outpost: true`.                                                      | `**ShipType::Armada`** via `is_armada_target`.      |
| 2                    | 1,025                | System hostiles (hull `3` → **battleship** line in Kobayashi)        | Dominant upstream `hull_type` **3**; common `loca_id` 10003 → **Federation Patrol** (`marauder_name_only`, `translations-navigation.json`).                                                                                                             | Hull-derived only.                                  |
| 3                    | 3                    | Mission boss (Separatist)                                            | Sample `loca_id` 12321 → **BOSS Separatist** (`officer_name`, `translations-officer_names.json`).                                                                                                                                                       | Hull-derived only.                                  |
| 4                    | 115                  | Hunter-style hostiles                                                | Sample `loca_id` 14018 → **Klingon Hunter** (`officer_name`).                                                                                                                                                                                           | Hull-derived only.                                  |
| 5                    | 1,171                | “Moving reds” (includes API scouts; hull `0` → **interceptor** line) | Dominant upstream `hull_type` **0**; 53 rows have `is_scout: true` in `summary-hostile.json`; sample `loca_id` 50150 → **GALOR CLASS** (`ship_name`, `translations-ships.json`).                                                                        | Hull-derived only.                                  |
| 6                    | 1                    | Elite mission (Separatist)                                           | Sample `loca_id` 12322 → **ELITE Separatist** (`officer_name`).                                                                                                                                                                                         | Hull-derived only.                                  |
| 7                    | 1,184                | System hostiles (hull `2` → **explorer** line)                       | Dominant upstream `hull_type` **2**; sample `loca_id` 60057 → **Romulan Velite** (`marauder_name_only`, hazard-tagged UI string).                                                                                                                       | Hull-derived only.                                  |
| 8                    | 436                  | Mixed hull lines (mostly hull `2`/`0`/`3`)                           | Mixed upstream `hull_type` in summary; sample `loca_id` 11003 → **Federation Patrol** (`marauder_name_only`).                                                                                                                                           | Hull-derived only.                                  |
| 10                   | 167                  | Survey-line traders / miners                                         | All `hull_type` 1 (survey); sample `loca_id` 14020 → **Independent Trader** (`marauder_name_only`).                                                                                                                                                     | Hull-derived only.                                  |
| 11                   | 111                  | Survey-line faction traders                                          | All `hull_type` 1; sample `loca_id` 12001 → **Romulan Trader** (`marauder_name_only`).                                                                                                                                                                  | Hull-derived only.                                  |
| 12                   | 84                   | Survey-line transports                                               | All `hull_type` 1; sample `loca_id` 13002 → **Klingon Transport** (`marauder_name_only`).                                                                                                                                                               | Hull-derived only.                                  |
| 13                   | 28                   | War coalition / mixed-line hostiles                                  | Mixed `hull_type`; sample `loca_id` 12005 → **Romulan War Coalition** (`marauder_name_only`).                                                                                                                                                           | Hull-derived only.                                  |
| 14                   | 42                   | Freebooter / ex-Borg-adjacent (hull `0` → **interceptor** line)      | All upstream `hull_type` **0**; sample `loca_id` 52051 → **Freebooter Interceptor** (`ship_name`, `translations-ships.json`).                                                                                                                           | Hull-derived only.                                  |


**Absent in this snapshot:** numeric **9** does not occur in the index or `summary-hostile.json` for this data version. If it appears after a refresh, re-run the commands below and extend this table.

Related UI strings that **do not** yet have a confirmed one-to-one `ship_type` mapping in this doc (keep triaging with in-game / client bundles): `mta_node_target_label` (**FORMATION ARMADA TARGET**), `solo_armada_target_label` (**SOLO ARMADA**), `solo_boss_armada_target_label` (**INVADING ENTITY**), `combat_triangle_outpost_target` (**OUTPOST TARGET**), `cross_alliance_armada_target_label` (**OPEN ARMADA TARGET**).

## Methodology

1. **Distinct values:** Enumerate `upstream_ship_type` from the normalized hostile index and cross-check `ship_type` in `data/upstream/data-stfc-space/summary-hostile.json`.
2. **Row counts:** Count index rows per value (maintainer-facing volume signal).
3. **Labels:** Prefer `translations-navigation.json` (`marauder_name_only`, `marauder_name`), `translations-officer_names.json` (`officer_name`), and `translations-ships.json` (`ship_name`) keyed by representative `loca_id` values from the index; correlate with `hull_type`, `is_scout`, and `is_outpost` from upstream summary rows.
4. **Combat:** Only promote a value to a dedicated `match` arm with non-default profile behavior when LCARS / mitigation / observed combat tests require it (today: **1** only).

## Regenerating the distinct-value list

From the repository root, after refreshing hostile data:

```bash
jq '[.hostiles[].upstream_ship_type] | unique | sort' data/hostiles/index.json
jq '[.[].ship_type] | unique | sort' data/upstream/data-stfc-space/summary-hostile.json
```

Row counts per value:

```bash
jq -r '.hostiles[].upstream_ship_type' data/hostiles/index.json | sort | uniq -c | sort -k2 -n
```

Strict validation (fails CI on undocumented ids unless defer-listed with a reason in code):

```bash
cargo run --bin validate_data
```

Human-readable triage report (canonical conditions + hostile `upstream_ship_type` table and undocumented subsection):

```bash
cargo run --bin report_unknown_mappings -- --output /tmp/unknown_mappings.md
```

## See also

- [ROADMAP.md](ROADMAP.md) — section *Hostile upstream `ship_type`*
- [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md) — hostile model / import overview
- [DESIGN.md](DESIGN.md) — defender `ship_type` / LCARS note

