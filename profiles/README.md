# Local player profiles

- **`index.json`** — Lists profile ids, display names, and **per-profile sync tokens** (`stfc-sync-token` for the STFC Community Mod). Treat tokens like secrets: **do not commit** `profiles/index.json` to a public repository.

- **Fresh clone** — There is no `index.json` in git. On first `kobayashi serve`, migration creates it automatically:
  - If the bundled **`demo/`** directory is present, the server registers a **Demo** profile with a **newly generated** sync token and sets it as the default.
  - Otherwise it creates a single **default** profile.

- **Schema** — See [`index.json.example`](index.json.example) for the JSON shape (placeholder token only).

- **Backup** — Use the app’s profile menu **Export backup (zip)** to archive `profiles/` safely offline.

- **Leaked token** — If a sync token was ever exposed, generate a new profile or replace the token in `index.json` and update the mod’s token for that profile.
