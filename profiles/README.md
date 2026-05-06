# Local player profiles

## What is OK to share publicly

Only the bundled **`demo/`** tree (sample `profile.json`, roster, etc.) is meant for the public repo. **Do not** publish your real **`index.json`**, other profile directories (e.g. named accounts), or backups containing real sync tokens or player-chosen names.

The checked-in **`demo/roster.imported.json`** is intentionally a large sample (hundreds of officers) so fresh clones can exercise roster-aware flows (“Owned only”, optimizer pools). Do not overwrite it with tiny import fixtures from local CLI runs or tests; CI guards minimum size. To refresh your personal roster, import into your own profile directory (or restore this file from git history when updating the shared demo deliberately).

## Private data

- **`index.json`** — Lists profile ids, display names, and **per-profile sync tokens** (`stfc-sync-token` for the STFC Community Mod). Treat tokens like secrets: **do not commit** `profiles/index.json` to a public repository.

- **Fresh clone** — There is no `index.json` in git. On first `kobayashi serve`, migration creates it automatically:
  - If the bundled **`demo/`** directory is present, the server registers a **Demo** profile with a **newly generated** sync token and sets it as the default.
  - Otherwise it creates a single **default** profile.

- **Schema** — See [`index.json.example`](index.json.example) for the JSON shape (placeholder token only).

- **Backup** — Use the app’s profile menu **Export backup (zip)** to archive `profiles/` safely offline.

- **Leaked token** — If a sync token was ever exposed, generate a new profile or replace the token in `index.json` and update the mod’s token for that profile.

## Roster CSV / Spocks sources (optional)

Put files such as `my_roster.txt` or `spocks-export.json` **inside the profile directory** they belong to (e.g. `profiles/demo/my_roster.txt`). Then:

- `kobayashi import my_roster.txt --profile demo` (bare name resolves under that profile folder), or
- pass a full path.

The importer writes **`roster.imported.json` in the same profile directory** (shared with Community Mod sync).
