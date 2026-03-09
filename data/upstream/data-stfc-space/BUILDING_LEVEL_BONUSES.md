# Building level bonuses (Building 70 / Subspace relay)

## Incremental bonuses (gain per level, levels 1–60)

**No file in the repo explicitly describes incremental stat gains from level 1 to 60 for building 70.**

- **`data/buildings/building_70.json`** (processed) has levels **25–80** only (56 levels), with one `value` per stat per level. Building 70 has `unlock_level: 25`, so levels 1–24 are not present in that file.
- **`data/upstream/data-stfc-space/summary-building.json`** has building 70 with `max_level: 60`, `unlock_level: 25`, and per-buff **cumulative** values (see below). Incremental for level N is **derived** as: `cumulative[N] - cumulative[N-1]` (with cumulative[0] = 0 or the value at “level 0” if defined).

So: **incremental from level 1 to 60 is not stored as such**; it can only be obtained by differentiating the cumulative series from `summary-building.json` (and, for levels 25+, you can cross-check with `data/buildings/building_70.json` if you know whether that file’s `value` is cumulative or incremental).

---

## Cumulative bonuses (total at each level)

**File:** `data/upstream/data-stfc-space/summary-building.json`

- Structure: array of buildings. Each building has `id`, `max_level`, `unlock_level`, and `buffs`.
- Each buff has `id` and `values`: array of `{ "value": number, "chance": 1 }`.
- **`values[i]` = cumulative stat bonus at level (i+1)** (index 0 = level 1, index 1 = level 2, …). It is **not** the incremental gain for that level.

Example: building 70, buff `3154267878` — `values[0]=0.25`, `values[1]=0.5`, … means total at level 1 is 0.25, total at level 2 is 0.5, etc. Incremental at level 2 = 0.5 − 0.25 = 0.25.
