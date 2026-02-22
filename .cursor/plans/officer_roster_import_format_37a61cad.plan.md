---
name: Officer roster import format
overview: "Document the expected file formats for manual officer roster import: (1) Spocks-style JSON input for the CLI importer, and (2) the direct roster JSON format used by the optimizer when no community-mod sync is used."
todos: []
isProject: false
---

# Expected file format for manual officer roster import

When a player does **not** use Community Mod sync, they can supply their officer roster in one of two ways.

---

## Option A: Use the Spocks-style importer (recommended)

**Command:** `kobayashi import <path-to-export.json>`

**Input:** A JSON file in one of the shapes the importer accepts (`[SpocksExport](src/data/import.rs)` in [src/data/import.rs](src/data/import.rs)). The importer flattens to a list of officer records and matches names to Kobayashi’s canonical officer list.

Accepted top-level structures (any one of):


| Top-level shape                          | Description                                   |
| ---------------------------------------- | --------------------------------------------- |
| **Array**                                | Root is directly an array of officer objects. |
| `{ "officers": [ ... ] }`                | Object with an `officers` array.              |
| `{ "data": { "officers": [ ... ] } }`    | Nested under `data.officers`.                 |
| `{ "profile": { "officers": [ ... ] } }` | Nested under `profile.officers`.              |


**Each officer record** can use any of these field names (snake_case or camelCase), all optional except that at least one identifier is needed for matching:

- **Identity:** `id`, `officerId`, `officer_id`, or `template_id`; and/or `name`, `officerName`, `officer_name`, or `title`; and/or nested `officer: { id, name }`.
- **Progress:** `rank` or `officerRank`, `tier` or `officerTier`, `level` or `officerLevel` (numeric).

Names are normalized and matched against [data/officers/officers.canonical.json](data/officers/officers.canonical.json); aliases are applied from [data/officers/name_aliases.json](data/officers/name_aliases.json). The importer writes the resolved roster to `**data/officers/roster.imported.json`** (see Option B for that format).

---

## Option B: Write the roster file directly (no importer)

If the player skips the importer and edits the roster file by hand (or generates it from another tool), they must write the same format the optimizer and sync logic expect.

**File location:** `data/officers/roster.imported.json` (or another path if the app is configured to use it; the default is [DEFAULT_IMPORT_OUTPUT_PATH](src/data/import.rs) and the same structure is loaded in [load_imported_roster_ids](src/data/import.rs) and [load_existing_roster](src/server/sync.rs)).

**Required structure:**

```json
{
  "officers": [
    {
      "canonical_officer_id": "kirk-1323b6",
      "canonical_name": "Kirk",
      "rank": 2,
      "tier": 3,
      "level": 45
    }
  ]
}
```

- `**officers**` (required): Array of officer entries.
- **Per entry:**
  - `**canonical_officer_id`** (required): Must match an officer `id` from [data/officers/officers.canonical.json](data/officers/officers.canonical.json). The crew generator uses this set to restrict candidates to officers the player “owns” ([crew_generator.rs](src/optimizer/crew_generator.rs)).
  - `**canonical_name`** (required): Display name (typically the same as in the canonical list).
  - `**rank`**, `**tier**`, `**level**` (optional): Omitted fields are serialized as absent; they can be used by the app for display or future logic.

**Optional top-level field:** `source_path` is often set by the importer or sync (e.g. `"stfc-mod sync"`); it is not required for manual files.

---

## Summary

- **Manual import via CLI:** Use a Spocks-style JSON (Option A) and run `kobayashi import <file>`; the importer produces `roster.imported.json` in the format above.
- **Manual file without importer:** Write or edit `data/officers/roster.imported.json` (Option B) with the `officers` array and required `canonical_officer_id` / `canonical_name` per entry, using IDs from the canonical officer list.

No code changes are required to support these formats; this plan only describes the expected file formats for manual roster import without community mode sync.