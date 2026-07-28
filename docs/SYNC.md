# Sync the game state with the STFC Community Mod

Kobayashi can accept the game state in near-real time from the
[STFC Community Mod](https://github.com/netniV/stfc-mod) (netniV/stfc-mod). Configure the
mod to send data to Kobayashi. The mod then writes each roster update from the game to the
same roster file that the optimizer reads. Thus the crew recommendations agree with the
officers that you own, and you do not import a file manually.

The mod uses the same sync protocol as [Spocks.club](https://spocks.club/syncing/). You can
send the data to Spocks.club and to Kobayashi at the same time.

## Prerequisites

- The [STFC Community Mod](https://github.com/netniV/stfc-mod/releases) must be installed
  and must operate correctly with Star Trek Fleet Command. Use Windows, or use Wine on
  macOS, as the INSTALL.md file of the mod tells you.
- The Kobayashi server must run, for example with `kobayashi serve`. You can start it from
  a source checkout or from an extracted release archive. The binary finds `data/` and
  `profiles/` next to itself. Set `KOBAYASHI_HOME` only when you keep these assets in
  another location.

## Configuration

### 1. Kobayashi (the sync token of the profile)

Sync authentication is **per profile**. It is not a global environment variable. Each
Kobayashi profile has its own `sync_token`. This token is a secret, and `profiles/index.json`
holds it. The mod sends the same token in the `stfc-sync-token` request header. The server
uses the token to find the profile to write to.

A token is always necessary. `POST /api/sync/ingress` returns **401** when the header is
absent, and also when the header matches no profile. There is no mode that accepts all
requests. To find or manage the token of each profile, use `GET /api/profiles` or the
profile page in the web interface.

### 2. The Community Mod (add Kobayashi as a sync target)

Edit `community_patch_settings.toml` in the **folder where you installed the game**. This
is the same directory as `version.dll`. On Windows the path is often similar to
`C:\Games\Star Trek Fleet Command\...\default\game\`. For the exact path on your system,
refer to the INSTALL.md file of the mod. Make sure that sync is on, and add a target for
Kobayashi. Also set the officer sync toggle of the mod to on, for example `officer = true`
in the `[sync]` section. The mod then sends the roster.

```toml
[patches]
syncpatches = true

[sync]
# Top-level token/url are for your default sync target (e.g. Spocks.club).
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
token = "<your Kobayashi profile's sync_token>"
```

Set the `token` of the Kobayashi target to the `sync_token` of the profile that you want to
write to. To find the token, use `GET /api/profiles` or the profile page. The server
rejects a request with **401** when the token is absent or matches no profile.

Change the URL when Kobayashi runs on a different host or a different port, for example
`http://192.168.1.10:3000/api/sync/ingress`.

## The status of the sync implementation

Sync has the scope of one profile. The `stfc-sync-token` header identifies the profile, and
the server writes the data to the directory of that profile (`profiles/{profile_id}/...`).
The optimizer reads from the path of the default profile. `GET /api/sync/status` returns
the same paths. Start the server from the root of the project. The server then finds
`profiles/` and `data/`.

| Payload type                                                    | Persisted          | File / usage                                                                                                                                                                                       |
| --------------------------------------------------------------- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| officer                                                         | Yes                | `profiles/{id}/roster.imported.json` — the roster for “Owned only” and for the optimizer                                                                                                           |
| research                                                        | Yes                | `profiles/{id}/research.imported.json` — the **full merged savepoint** of each `rid` and `level` that the mod sent for this profile. The server does not filter it by `research_catalog.json`. The catalog changes the combat bonuses only. |
| buildings / module                                              | Yes                | `profiles/{id}/buildings.imported.json`                                                                                                                                                            |
| ships / ship                                                    | Yes                | `profiles/{id}/ships.imported.json`                                                                                                                                                                |
| ft (forbidden tech)                                             | Yes                | `profiles/{id}/forbidden_tech.imported.json` — the bonuses merge into the optimizer profile                                                                                                        |
| tech                                                            | Yes (same as ft)   | The **STFC Community Mod** sends the forbidden tech and the chaos tech with the JSON `type: "tech"` (fid, tier, level, shard_count). The server writes them to the same `forbidden_tech.imported.json` as `ft`. |
| buffs                                                           | Yes                | `profiles/{id}/buffs.imported.json` — the global active buffs (`bid`, `level`, optional `expiry_time`). To remove a buff, the mod sends `type: "expired_buffs"` with `bid`.                        |
| battlelogs                                                      | Yes                | `profiles/{id}/battlelogs.imported.json` — a moving window of the **last 50** battle log objects. The server keeps the order in which it received them, and it drops the older entries.            |
| resources, missions, traits, slots, inventory, jobs             | No (accepted, 200) | —                                                                                                                                                                                                  |

## The data that the mod sends

- **Officers.** The server merges each payload with `type: "officer"` into
  `profiles/{id}/roster.imported.json`. It maps the officer id of the game (`oid`) to the
  canonical officer id of Kobayashi with `data/officers/id_registry.json`. The optimizer
  then uses this roster to limit the crew candidates to the officers that you own.
- **Research.** The server merges each payload with `type: "research"` into
  `profiles/{id}/research.imported.json`, by `rid`. To read the file, use
  `load_imported_research`. When the research catalog is present
  (`data/research_catalog.json`), the optimizer merges the research bonuses into the player
  profile for combat. Refer to `data/README.md` § Research.
- **Buildings.** The server merges each payload with `type: "buildings"` or
  `type: "module"` into `profiles/{id}/buildings.imported.json`, by `bid`. The mod sends
  `"module"`. The optimizer reads this file from the path of the default profile and merges
  the building bonuses into the player profile. Refer to `data/README.md` § Buildings.
- **Ships.** The server merges each payload with `type: "ships"` or `type: "ship"` into
  `profiles/{id}/ships.imported.json`, by `psid`. The mod sends `"ship"`. To read the file,
  use `load_imported_ships`. In **Roster mode** the ship list shows only the ships that you
  own. The server maps the `hull_id` from the sync to the Kobayashi ship id with
  `data/hull_id_registry.json`. When the game or the Kobayashi catalog gets new ships, make
  the registry again with `node scripts/build_hull_id_registry.mjs`. Run this command from
  the root of the project.
- **Forbidden tech (`ft` or `tech`).** The server merges each payload with `type: "ft"` or
  `type: "tech"` into `profiles/{id}/forbidden_tech.imported.json`, by `fid`. The mod uses
  `"tech"`. To read the file, use `load_imported_forbidden_tech`. The server merges the
  player state into the optimizer profile with `data/forbidden_chaos_tech.json`, by `fid`.
- **Battlelogs.** For a payload with `type: "battlelogs"`, the server adds each element of
  the array to `profiles/{id}/battlelogs.imported.json`. It then keeps only the **50**
  objects that it received last, and drops the oldest objects first. It keeps each object
  as the mod sent it, as opaque JSON, for calibration or for other tools later.
- **The other types** (resources, missions, traits, slots, inventory, jobs). The server
  accepts these payloads and returns 200, but it does not write them to a file.

## The mapping of the officer id

The mod sends the officer id in the format of the game (`oid`). Kobayashi maps it to the
canonical id, for example `kirk-1323b6`, with `data/officers/id_registry.json`. When the
game gets a new officer that is not yet in the registry, Kobayashi skips that officer. It
skips the officer until a maintainer or a data pipeline updates the registry.

## How to check the sync

Do these steps to make sure that the sync operates correctly:

1. Open the game and cause a sync. For example, open the officers screen or change
   something.
2. Make sure that the server updated `roster.imported.json` for the profile. To find the
   path, use `GET /api/sync/status`.
3. In the Kobayashi web interface, turn on “Owned only” in the crew builder. Make sure that
   the officer list agrees with your roster in the game.

## The API

- **Endpoint**: `POST /api/sync/ingress`
- **Headers**: `Content-Type: application/json` and `stfc-sync-token: <profile sync_token>`.
  The token is necessary. It identifies the target profile. The server returns 401 when the
  token is absent or matches no profile.
- **Body**: a JSON array of objects. Each object has a `type` field. The `type` of the first
  element tells the server how to process the payload (officer, research, buildings, ships,
  or another type). The shape of each type is the shape of the
  [Community Mod sync payloads](https://github.com/netniV/stfc-mod/blob/main/mods/src/patches/parts/sync.cc).
- **Response**: 200 with `{"status":"ok","accepted":["officer(N)"]}` or a similar body. The
  server returns **401** when the `stfc-sync-token` is absent or matches no profile. It
  returns **400** when the body is not a JSON array. It returns **500** when it cannot write
  the file.
- **Endpoint**: `GET /api/sync/status`
- **Response**: 200 with the data of the selected profile. The server selects the profile
  from `X-Profile-Id` or from `?profile=`. If neither is present, it uses the effective
  default profile. The body contains these fields:
  - `profile_id`.
  - The seven path fields: `roster_path`, `research_path`, `buildings_path`, `ships_path`,
    `forbidden_tech_path`, `buffs_path`, and `battlelogs_path`.
  - One last-modified timestamp for each file. The field is `last_modified_iso` for the
    roster, and `<type>_last_modified_iso` for the other files. The value is an ISO8601
    timestamp, or null when the file is absent.
  - `last_mod_sync_utc`. The server sends this field after the mod wrote a batch.
  - `research_catalog_loaded` and `research_catalog_item_count`.

## Reference of the sync payloads

The request body is a JSON array. The `type` field of the first element tells the server how
to process the payload. The table gives the shape of each type. The source is the
[Community Mod sync.cc](https://github.com/netniV/stfc-mod/blob/main/mods/src/patches/parts/sync.cc)
file.

| Type                       | Keys per item                                                                                                          | Notes                                                                                                                                                        |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **officer**                | `type`, `oid` (game id), `rank`, optional `tier` (the server uses `tier` before `rank` when `tier` is present), `level`, optional `shard_count` | The server merges the item into `profiles/{id}/roster.imported.json`. It maps `oid` with `data/officers/id_registry.json`.                                    |
| **research**               | `type`, `rid` (int64), `level` (int32)                                                                                 | One object for each research project level. The server writes it to `profiles/{id}/research.imported.json`. The server uses it for combat when `data/research_catalog.json` is present. |
| **buildings** / **module** | `type`, `bid` (int64), `level` (int32)                                                                                 | Starbase modules. The mod sends `type: "module"`. Kobayashi accepts `"buildings"` and `"module"`. The server writes the item to `profiles/{id}/buildings.imported.json`. |
| **ships** / **ship**       | `type`, `psid` (int64), `tier`, `level`, `level_percentage` (double), `hull_id` (int64), `components` (array of int64) | One ship of the player. The mod sends `type: "ship"`. Kobayashi accepts `"ships"` and `"ship"`. The server writes the item to `profiles/{id}/ships.imported.json`. |
| **ft**                     | `type`, `fid` (int64), `tier`, `level`, `shard_count` (int64)                                                          | Forbidden tech and chaos tech. The server writes the item to `profiles/{id}/forbidden_tech.imported.json`.                                                    |
| **tech**                   | The same fields as **ft**                                                                                              | The server writes the item as it writes **ft**. This is the queue name of the mod for the forbidden tech and the chaos tech.                                  |
| **buffs**                  | `type`, `bid` (buff id), `level`, optional `expiry_time` (null or unix seconds)                                        | The global active buffs go to `profiles/{id}/buffs.imported.json`. To remove a buff, the mod sends `type: "expired_buffs"` with `bid`.                        |
| **battlelogs**             | `type`, and the other fields that the mod sets for each object                                                          | The server adds the item to `profiles/{id}/battlelogs.imported.json`. It keeps only the last **50** objects, in the order in which it received them.          |

The server accepts the other types (resources, missions, traits, slots, inventory, and jobs)
and returns 200, but it does not write them to a file.
