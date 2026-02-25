# Syncing game state with the STFC Community Mod

Kobayashi can accept **quasi real-time** game state from the [STFC Community Mod](https://github.com/netniV/stfc-mod) (netniV/stfc-mod). When the mod is configured to send data to Kobayashi, officer roster updates from the game are written to the same roster file used by the optimizer, so crew recommendations stay in sync with what you own without manual file import.

This uses the same sync protocol as [Spocks.club](https://spocks.club/syncing/); you can point the mod at both Spocks.club and Kobayashi if you wish.

## Requirements

- [STFC Community Mod](https://github.com/netniV/stfc-mod/releases) installed and working with Star Trek Fleet Command (Windows or Wine/macOS as per the mod’s INSTALL.md).
- Kobayashi server running (e.g. `kobayashi serve`). **Run the server from the project root** so that `data/officers/` and `rosters/` paths resolve correctly.

## Configuration

### 1. Kobayashi (optional token)

- **`KOBAYASHI_SYNC_TOKEN`** (optional): If set, the server will require the `stfc-sync-token` request header to match this value for `POST /api/sync/ingress`. If unset, any request is accepted (suitable for local-only use).

Example (PowerShell):

```powershell
$env:KOBAYASHI_SYNC_TOKEN = "your-secret-token"
kobayashi serve
```

### 2. Community Mod (add Kobayashi as a sync target)

Edit `community_patch_settings.toml` in your **game install folder** (the same directory as `version.dll`; on Windows often something like `C:\Games\Star Trek Fleet Command\...\default\game\`). See the mod’s INSTALL.md for the exact path on your system. Ensure sync is enabled and add a target for Kobayashi. **Turn on the mod’s officer sync toggle** (e.g. `officer = true` under `[sync]`) so the roster is sent.

```toml
[patches]
syncpatches = true

[sync]
# Optional: use the same token as KOBAYASHI_SYNC_TOKEN if you set it
token = ""
url = ""

# Data toggles (at least officer for roster sync)
officer = true
research = true
buildings = true
ships = true
# ... other options as needed

[sync.targets.kobayashi]
url = "http://localhost:3000/api/sync/ingress"
token = "your-secret-token"
```

Use the same value for `token` as for `KOBAYASHI_SYNC_TOKEN` if you set the env var. If you run Kobayashi without `KOBAYASHI_SYNC_TOKEN`, you can leave `token` empty or set any value.

Change the URL if Kobayashi runs on another host or port (e.g. `http://192.168.1.10:3000/api/sync/ingress`).

## What gets synced

- **Officers**: Each sync payload with `type: "officer"` is merged into `rosters/roster.imported.json`. Game officer IDs (`oid`) are mapped to Kobayashi’s canonical officer IDs via `data/officers/id_registry.json`. The optimizer then uses this roster to restrict crew candidates to officers you own.
- **Other types** (research, buildings, ships, etc.): The server accepts the payloads and returns 200 but does not persist them yet. Persistence for future optimizer features can be added later.

## Officer ID mapping

The mod sends officer IDs in the game’s format (`oid`). Kobayashi maps them to canonical IDs (e.g. `kirk-1323b6`) using `data/officers/id_registry.json`. If a new officer is added to the game and not yet in the registry, that officer will be skipped until the registry is updated (e.g. by a maintainer or a data pipeline).

## Verification

To confirm sync is working: (1) Open the game and trigger a sync (e.g. open the officers screen or change something). (2) Check that `rosters/roster.imported.json` was updated (file modification time). (3) In the Kobayashi web UI, enable “Owned only” in the crew builder and confirm the officer list matches your in-game roster. You can also call `GET /api/sync/status` to see the roster file path and last modified time.

## API

- **Endpoint**: `POST /api/sync/ingress`
- **Headers**: `Content-Type: application/json`; optional `stfc-sync-token: <token>` (required if `KOBAYASHI_SYNC_TOKEN` is set).
- **Body**: JSON array of objects. Each object has a `type` field; the first element’s `type` determines how the payload is handled (officer, research, buildings, ships, etc.). Shape per type matches the [Community Mod sync payloads](https://github.com/netniV/stfc-mod/blob/main/mods/src/patches/parts/sync.cc).
- **Response**: 200 with `{"status":"ok","accepted":["officer(N)"]}` or similar; 401 if token is required and missing/invalid; 400 if body is not a JSON array.

- **Endpoint**: `GET /api/sync/status`
- **Response**: 200 with JSON `{ "roster_path": "rosters/roster.imported.json", "last_modified_iso": "<ISO8601 or null if file missing>" }` so you can see when the roster was last updated by sync.
