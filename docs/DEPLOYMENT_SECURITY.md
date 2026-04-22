# Deployment and security model

Kobayashi is designed primarily as a **local-first** tool: you run `kobayashi serve` on your machine and open the UI in a browser. When you expose the same HTTP server to a **LAN** or the **internet**, the trust assumptions change. This document describes what the server does *not* guarantee by default and how to harden it.

## What is not authentication

### `X-Profile-Id` and `?profile=`

These values **select which profile** (roster, imports, presets, etc.) the server uses for a request. They are **not** a security boundary. Anyone who can reach the API can send any profile id they know or guess.

Treat profile ids like filenames: convenient scoping, not proof of identity.

### Sync ingress (`POST /api/sync/ingress`)

The [STFC Community Mod](https://github.com/netniV/stfc-mod) sends data using the `**stfc-sync-token`** header (per-profile secret in `profiles/<id>/profile.json`). That token **scopes writes** to the matching profile directory. It is still important to **keep the token secret** and to **not expose** the sync endpoint to untrusted networks without additional controls.

Sync ingress is **not** covered by `KOBAYASHI_API_KEY` (see below): it keeps its own token-based routing.

## Release binaries and integrity

Prebuilt **GitHub Release** archives (Linux, macOS arm64, Windows) ship the `kobayashi` binary and `frontend/dist/` only; they are meant to be extracted **on top of** a checkout of the same **git tag** so `data/`, `profiles/`, and other repo paths remain available. See [`packaging/RELEASE-BUNDLE-README.txt`](../packaging/RELEASE-BUNDLE-README.txt).

Each release includes **`SHA256SUMS`** (SHA-256 of every attached archive). After downloading, verify before unpacking, for example:

- Linux: `sha256sum -c SHA256SUMS` (remove lines for archives you did not download if the checker complains).
- macOS: `shasum -a 256 -c SHA256SUMS`

Treat third-party binaries like any other downloaded executable: fetch only from the project’s **Releases** page, verify hashes, and prefer **signed or annotated tags** when correlating source (`git tag -v vX.Y.Z` for signed tags).

## Threat surfaces by deployment


| Deployment                          | Typical risk                                   | Mitigations                                                                                                |
| ----------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| **Localhost only** (`127.0.0.1`)    | Low: only local processes can connect          | Default `KOBAYASHI_BIND` is loopback-friendly; optional API key still works                                |
| **LAN** (`0.0.0.0` or a machine IP) | Medium: anyone on the network can call the API | Firewall, bind to a trusted interface, TLS reverse proxy, optional `KOBAYASHI_API_KEY`                     |
| **Internet**                        | High: scanning, abuse of CPU-heavy routes      | **Do not** expose raw HTTP without TLS; use a reverse proxy, strong secrets, rate limits, and network ACLs |


## Optional shared secret for mutating API routes

When `**KOBAYASHI_API_KEY`** is set to a non-empty value, the server requires that secret on **mutating** HTTP API calls: `POST`, `PUT`, `DELETE`, and `PATCH` under `/api/`, **except** `POST /api/sync/ingress` (sync keeps using `stfc-sync-token`).

Accepted headers (either works):

- `Authorization: Bearer <token>`
- `X-Api-Key: <token>`

**Loopback by default:** If `**KOBAYASHI_API_KEY_TRUST_LOOPBACK`** is `1`, `true`, or `yes` (the default), clients whose **TCP peer address** is a loopback address (`127.0.0.1`, `::1`) **do not** need to send the key. That keeps local development and same-machine browser traffic working without embedding a secret in the frontend.

Set `**KOBAYASHI_API_KEY_TRUST_LOOPBACK=0`** (or `false` / `no`) to require the key **even from loopback** (useful for strict local testing of the header path).

### Browser UI and API keys

The React app does **not** read the API key from build-time environment variables (you should not bake long-lived secrets into `frontend/dist`). Typical patterns:

- **Same machine, trust loopback:** leave `KOBAYASHI_API_KEY_TRUST_LOOPBACK` at default; no browser changes.
- **Reverse proxy in front of the API:** terminate TLS at the proxy and inject `Authorization` or `X-Api-Key` for upstream requests.
- **LAN client with key required:** configure the client or proxy to add the header; never commit the key to the repo.

## Related environment variables


| Variable                           | Purpose                                                                |
| ---------------------------------- | ---------------------------------------------------------------------- |
| `KOBAYASHI_BIND`                   | Address to listen on (default `127.0.0.1:3000`)                        |
| `KOBAYASHI_API_KEY`                | Optional shared secret for mutating `/api/`* routes (not sync ingress) |
| `KOBAYASHI_API_KEY_TRUST_LOOPBACK` | When `1` (default), loopback peers skip the key check                  |
| `KOBAYASHI_LOG`                    | Kobayashi log level/filter alias (`info`, `debug`, or full filter)     |
| `RUST_LOG`                         | Full tracing filter (highest precedence over `KOBAYASHI_LOG`)          |


See also [SYNC.md](SYNC.md) for mod sync tokens and [README.md](../README.md) for general run instructions.

## Structured logging and tracing

Server logs are emitted as newline-delimited JSON via `tracing` / `tracing-subscriber`.

- Per-request logs include fields like `method`, `matched_path`, `status`, and `latency_ms`.
- Optimize lifecycle logs include `job_id`, `seed`, `requested_strategy`, `effective_strategy`, and progress phase (`heuristics`, `monte_carlo`, `tiered_scout`, `tiered_confirm`, `genetic`).
- Simulation batch logs include `batch_index`, `batch_total`, `batch_start`, `batch_end`, and candidate counts.

### Enable log levels

```bash
# Common operator default
KOBAYASHI_LOG=info ./target/release/kobayashi serve

# Full filter syntax (overrides KOBAYASHI_LOG when set)
RUST_LOG='warn,kobayashi=info,tower_http=info' ./target/release/kobayashi serve
```

### `jq` recipes

```bash
# Request latency summary by route (p50/p95 in ms)
jq -r 'select(.fields.message=="request_completed") | [.fields.matched_path, .fields.latency_ms] | @tsv' server.log \
  | awk -F '\t' '{a[$1]=a[$1]" "$2} END {for (k in a) {n=split(substr(a[k],2),v," "); asort(v); p50=v[int((n+1)*0.50)]; p95=v[int((n+1)*0.95)]; printf "%s\tp50=%sms\tp95=%sms\tn=%d\n",k,p50,p95,n}}'
```

```bash
# Non-2xx request completions with route + status
jq 'select(.fields.message=="request_completed" and (.fields.status|tonumber >= 400)) | {ts: .timestamp, route: .fields.matched_path, status: .fields.status, latency_ms: .fields.latency_ms}'
```

```bash
# Optimize phase transitions and progress ticks
jq 'select((.fields.message=="optimize_phase_started") or (.fields.message=="optimize_phase_completed") or (.fields.message=="optimize_progress_tick")) | {ts: .timestamp, job_id: .fields.job_id, phase: .fields.phase, progress: .fields.progress, crews_done: .fields.crews_done, total_crews: .fields.total_crews}'
```

```bash
# Batch-level throughput view for optimize Monte Carlo/tiered scout batches
jq 'select((.fields.message=="optimize_sim_batch_started") or (.fields.message=="optimize_sim_batch_completed")) | {ts: .timestamp, phase: .fields.phase, strategy: .fields.strategy, batch_index: .fields.batch_index, batch_total: .fields.batch_total, batch_start: .fields.batch_start, batch_end: .fields.batch_end, crews_done: .fields.crews_done}'
```