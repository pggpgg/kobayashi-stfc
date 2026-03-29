# Deployment and security model

Kobayashi is designed primarily as a **local-first** tool: you run `kobayashi serve` on your machine and open the UI in a browser. When you expose the same HTTP server to a **LAN** or the **internet**, the trust assumptions change. This document describes what the server does *not* guarantee by default and how to harden it.

## What is not authentication

### `X-Profile-Id` and `?profile=`

These values **select which profile** (roster, imports, presets, etc.) the server uses for a request. They are **not** a security boundary. Anyone who can reach the API can send any profile id they know or guess.

Treat profile ids like filenames: convenient scoping, not proof of identity.

### Sync ingress (`POST /api/sync/ingress`)

The [STFC Community Mod](https://github.com/netniV/stfc-mod) sends data using the **`stfc-sync-token`** header (per-profile secret in `profiles/<id>/profile.json`). That token **scopes writes** to the matching profile directory. It is still important to **keep the token secret** and to **not expose** the sync endpoint to untrusted networks without additional controls.

Sync ingress is **not** covered by `KOBAYASHI_API_KEY` (see below): it keeps its own token-based routing.

## Threat surfaces by deployment

| Deployment | Typical risk | Mitigations |
|------------|--------------|-------------|
| **Localhost only** (`127.0.0.1`) | Low: only local processes can connect | Default `KOBAYASHI_BIND` is loopback-friendly; optional API key still works |
| **LAN** (`0.0.0.0` or a machine IP) | Medium: anyone on the network can call the API | Firewall, bind to a trusted interface, TLS reverse proxy, optional `KOBAYASHI_API_KEY` |
| **Internet** | High: scanning, abuse of CPU-heavy routes | **Do not** expose raw HTTP without TLS; use a reverse proxy, strong secrets, rate limits, and network ACLs |

## Optional shared secret for mutating API routes

When **`KOBAYASHI_API_KEY`** is set to a non-empty value, the server requires that secret on **mutating** HTTP API calls: `POST`, `PUT`, `DELETE`, and `PATCH` under `/api/`, **except** `POST /api/sync/ingress` (sync keeps using `stfc-sync-token`).

Accepted headers (either works):

- `Authorization: Bearer <token>`
- `X-Api-Key: <token>`

**Loopback by default:** If **`KOBAYASHI_API_KEY_TRUST_LOOPBACK`** is `1`, `true`, or `yes` (the default), clients whose **TCP peer address** is a loopback address (`127.0.0.1`, `::1`) **do not** need to send the key. That keeps local development and same-machine browser traffic working without embedding a secret in the frontend.

Set **`KOBAYASHI_API_KEY_TRUST_LOOPBACK=0`** (or `false` / `no`) to require the key **even from loopback** (useful for strict local testing of the header path).

### Browser UI and API keys

The React app does **not** read the API key from build-time environment variables (you should not bake long-lived secrets into `frontend/dist`). Typical patterns:

- **Same machine, trust loopback:** leave `KOBAYASHI_API_KEY_TRUST_LOOPBACK` at default; no browser changes.
- **Reverse proxy in front of the API:** terminate TLS at the proxy and inject `Authorization` or `X-Api-Key` for upstream requests.
- **LAN client with key required:** configure the client or proxy to add the header; never commit the key to the repo.

## Related environment variables

| Variable | Purpose |
|----------|---------|
| `KOBAYASHI_BIND` | Address to listen on (default `127.0.0.1:3000`) |
| `KOBAYASHI_API_KEY` | Optional shared secret for mutating `/api/*` routes (not sync ingress) |
| `KOBAYASHI_API_KEY_TRUST_LOOPBACK` | When `1` (default), loopback peers skip the key check |

See also [SYNC.md](SYNC.md) for mod sync tokens and [README.md](../README.md) for general run instructions.
