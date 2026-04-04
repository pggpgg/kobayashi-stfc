# Local player profiles

## What is OK to share publicly

Only the bundled **`demo/`** tree (sample `profile.json`, roster, etc.) is meant for the public repo. **Do not** publish your real **`index.json`**, other profile directories (e.g. named accounts), or backups containing real sync tokens or player-chosen names.

## Private data

- **`index.json`** — Lists profile ids, display names, and **per-profile sync tokens** (`stfc-sync-token` for the STFC Community Mod). Treat tokens like secrets: **do not commit** `profiles/index.json` to a public repository.

- **Fresh clone** — There is no `index.json` in git. On first `kobayashi serve`, migration creates it automatically:
  - If the bundled **`demo/`** directory is present, the server registers a **Demo** profile with a **newly generated** sync token and sets it as the default.
  - Otherwise it creates a single **default** profile.

- **Schema** — See [`index.json.example`](index.json.example) for the JSON shape (placeholder token only).

- **Backup** — Use the app’s profile menu **Export backup (zip)** to archive `profiles/` safely offline.

- **Leaked token** — If a sync token was ever exposed, generate a new profile or replace the token in `index.json` and update the mod’s token for that profile.

## Roster CSV / Spocks sources (optional)

Put files such as `my_roster.txt` or `spocks-export.json` **inside the profile directory** they belong to (e.g. `profiles/default/my_roster.txt`). Then:

- `kobayashi import my_roster.txt --profile default` (bare name resolves under that profile folder), or
- pass a full path.

The importer writes **`roster.imported.json` in the same profile directory** (shared with Community Mod sync). There is no separate global `rosters/` output anymore.

**Legacy:** If you still have a `rosters/` folder with `*.imported.json` from an old Kobayashi version, a one-time migration (when `profiles/index.json` is missing) may copy those files into `profiles/default/`.
