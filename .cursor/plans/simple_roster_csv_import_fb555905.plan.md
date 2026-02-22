---
name: Simple roster .txt import
overview: Add a novice-friendly officer roster import using a simple tab-separated text format (name, tier, level per line) so names can contain commas without quoting; wire it into the existing import command.
todos: []
isProject: false
---

# Simple tab-separated officer roster import

Use a **one-line-per-officer** text format so players can edit in Notepad or Excel without writing JSON. **Tab-separated** (not comma) so officer names that contain commas never need quoting.

---

## User-facing format

**File:** A `.txt` file (e.g. `my_roster.txt`).

**Contents:** One officer per line with three fields separated by **Tab**: **name**, **tier**, **level**.

- **Separator:** **Tab** (not comma). Names can contain commas, spaces, apostrophes—no quoting needed.
- **Header (optional):** First line may be `name	tier	level` (tabs between words); it is ignored.
- **Columns:** Order: name, then tier, then level. Tier and level are optional; flexible input and defaults (see below).

**Example:** (below, each gap between columns is a single Tab character)

```text
name	tier	level
Kirk	3	45
Spock	2	
Kirk	T3	lvl 50
Spock	Tier 2	LVL20
McCoy
James T. Kirk	1	30
```

Or without header (again, Tab between fields):

```text
Kirk	3	45
Spock	T2	
McCoy
James T. Kirk	1	30
```

---

## Parsing and defaults (failsafe)

**Tier parsing** — accept any of (case-insensitive, optional space):

- `T3`, `T 3`, `t3` → tier 3  
- `tier 2`, `Tier 2`, `tier2` → tier 2  
- `1`, `2`, `3` → tier 1, 2, 3

Rule: strip an optional prefix `tier` or `T`, then parse the remaining number. Invalid or empty → treat as “not given”.

**Level parsing** — accept any of (case-insensitive, optional space):

- `lvl 20`, `lvl20`, `LVL 20`, `LVL20` → level 20  
- `20` → level 20

Rule: strip an optional prefix `lvl` (or `LVL`), then parse the remaining number. Invalid or empty → treat as “not given”.

**Defaults (failsafe):**

- **Only name given** (no tier, no level) → assume **max tier** and **max level** for that tier (officer is “fully maxed”).
- **Name + tier given, no level** → assume **max level for that tier** (e.g. tier 1 → level 10, tier 2 → level 20, tier 3 → level 30; exact caps defined once in code, e.g. constants in `import.rs` or from game data).
- **Name + tier + level given** → use the parsed values.

So minimal input like `McCoy` (one column) or `Kirk	T3` (name and tier, tab between) is valid and gets sensible defaults.

**Command:** `kobayashi import my_roster.txt`. Use a `.txt` file for roster; use a `.json` file for Spocks export.

---

## Implementation

### 1. Tab-separated roster parser and import in [src/data/import.rs](src/data/import.rs)

- **New `ImportError` variant** (e.g. `ParseLine(line_number, message)`) for invalid lines, so the CLI can report “line 5: invalid tier”.
- **New public function** `import_roster_csv(path: &str) -> Result<ImportReport, ImportError>`:
  - Read the file line by line; split each line on **Tab** (`line.splitn(3, '\t')`): first segment = name, second = tier cell, third = level cell. No comma parsing—names can contain commas with no quoting.
  - Skip empty lines. If the first non-empty line looks like a header (e.g. first field is `"name"` case-insensitive), skip it.
  - For each data row: column 0 = name (required, trim); column 1 = tier via parse_tier_cell; column 2 = level via parse_level_cell. Apply defaults. Push `(name, None, tier, level)` (no “rank” from CSV).
  - Reuse the same **name resolution and write** logic as the Spocks importer: shared refactor; write `data/officers/roster.imported.json` and return `ImportReport`.
- **Refactor to avoid duplication:** Extract the resolution loop (and report building) into an internal function that accepts a list of “raw records” with `(raw_name, rank, tier, level)`. Then:
  - `import_spocks_export` flattens the Spocks JSON into that list and calls the shared function.
  - `import_roster_csv` parses tab-separated lines into that list and calls the same shared function.

So the only new user-facing code path is “parse TSV (tab-separated) → tier/level + defaults → existing resolution + write; tab delimiter avoids comma-in-name issues”.

**Parsing and defaulting (implementation detail):** Implement `parse_tier_cell(s)` (strip optional prefix `tier` or `T`, case-insensitive + optional space, then parse number) and `parse_level_cell(s)` (strip optional prefix `lvl` or `LVL`, same). Define `MAX_OFFICER_TIER` and `max_level_for_tier(tier)`; when tier is missing use max tier, when level is missing use max level for that tier. Apply these when building the raw record list for the shared resolution step.

### 2. CLI: format selection by extension

- In [src/main.rs](src/main.rs) (and [src/cli.rs](src/cli.rs) if it still has its own `handle_import`): in `handle_import`, if the path has extension `.txt`, call `import_roster_csv(path)`; if `.json`, call `import_spocks_export(path)`; otherwise error or treat as roster.
- Update the usage string from “path-to-export.json” to “path” and mention both formats, e.g.:  
`kobayashi import <path>` — use a `.txt` file for your roster (tab-separated), or a `.json` file for Spocks export.

### 3. Documentation

- In [data/import/README.md](data/import/README.md), add a **“Officer roster (your owned officers)”** section at the top (most user-facing):
  - File: a `.txt` file (e.g. `data/import/my_roster.txt`).
  - Command: `kobayashi import my_roster.txt`.
  - Format: one line per officer, **Tab** between name, tier, and level (e.g. `name    tier    level`). Optional header. Names can contain commas—no quoting needed. Tier can be T3, Tier 2, 2; level can be lvl 20, LVL20, 20.
  - Failsafe: name only = max tier and max level; name + tier only = max level for that tier.
  - One short example (e.g. Kirk, Spock, McCoy with mixed styles).
  - Note that the app writes `data/officers/roster.imported.json` for the optimizer; users do not need to edit that file.
  - Optionally one line: “Advanced: Spocks.club JSON export is also supported (use a `.json` file).”

---

## Summary

- **Format:** Tab-separated text: one row per officer = `name[TAB]tier[TAB]level` (tier and level optional). Names can contain commas. Tier accepts T3, Tier 2, 2, etc.; level accepts lvl 20, LVL20, 20, etc.
- **Failsafe defaults:** Name only → max tier and max level; name + tier only → max level for that tier.
- **No JSON** required from the user; they create or edit a `.txt` file and run `kobayashi import my_roster.txt`.
- **Code:** Add `import_roster_csv` in `import.rs` (with tier/level parsers and default constants), refactor shared resolution + write, and in the CLI branch on `.txt` (roster) vs `.json` (Spocks). Document in `data/import/README.md`.

