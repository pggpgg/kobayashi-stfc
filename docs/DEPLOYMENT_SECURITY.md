# Deployment and the security model

Kobayashi is primarily a local-first tool. You run `kobayashi serve` on your computer, and
you open the user interface in a browser. When you make the same HTTP server available on a
**LAN** or on the **internet**, the trust conditions change. This document tells you what
the server does not do by default, and how to protect it.

## What is not authentication

### `X-Profile-Id` and `?profile=`

These values select the profile that the server uses for a request. The profile contains the
roster, the imports, the presets, and other data. These values are not a security boundary.
Any client that can reach the API can send any profile id that the user knows or guesses.

Think of a profile id as a file name. It is a convenient scope, but it is not proof of
identity.

### Sync ingress (`POST /api/sync/ingress`)

The [STFC Community Mod](https://github.com/netniV/stfc-mod) sends data with the
`stfc-sync-token` header. This token is a secret for one profile, and `profiles/index.json`
holds it. The token limits the writes to the directory of the profile that matches. Keep the
token secret. Do not make the sync endpoint available to a network that you do not trust
unless you add more controls.

`KOBAYASHI_API_KEY` does not apply to sync ingress. Refer to the section below. Sync ingress
keeps its own token.

## The release binaries and their integrity

The prebuilt archives on the **GitHub Releases** page (Linux, macOS arm64, and Windows)
contain all the necessary files. Each archive holds the `kobayashi` binary, the built
`frontend/dist/` directory, the normalized runtime `data/` directory, and a starter
`profiles/demo/` directory. You do not need a repository checkout or a build toolchain. The
archives do not hold the upstream caches and the import sources, because they are for
maintenance only. Refer to
[`packaging/RELEASE-BUNDLE-README.txt`](../packaging/RELEASE-BUNDLE-README.txt).

Each release also gives a **`SHA256SUMS`** file. It holds the SHA-256 hash of each attached
archive. Check the archive before you extract it, for example:

- Linux: `sha256sum -c SHA256SUMS`. If the tool reports an error, delete the lines for the
  archives that you did not download.
- macOS: `shasum -a 256 -c SHA256SUMS`

Treat a third-party binary as you treat any other executable that you download. Download
only from the **Releases** page of the project. Check the hashes. When you compare an
archive with the source, prefer a signed tag or an annotated tag. For a signed tag, use
`git tag -v vX.Y.Z`.

## The risks of each deployment

| Deployment                          | Typical risk                                   | Protections                                                                                                |
| ----------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| **Localhost only** (`127.0.0.1`)    | Low. Only a local process can connect.         | The default `KOBAYASHI_BIND` uses the loopback address. The optional API key also operates.                |
| **LAN** (`0.0.0.0` or a machine IP) | Medium. Any client on the network can call the API. | Use a firewall. Bind to an interface that you trust. Use a reverse proxy with TLS. Set the optional `KOBAYASHI_API_KEY`. |
| **Internet**                        | High. Scanners find the server, and a client can abuse the routes that use much CPU time. | Do not make raw HTTP available. Use a reverse proxy with TLS, strong secrets, rate limits, and network access control lists. |

## The optional shared secret for the mutating API routes

Set `KOBAYASHI_API_KEY` to a value that is not empty. The server then needs this secret on
each mutating HTTP API call. A mutating call is a `POST`, a `PUT`, a `DELETE`, or a `PATCH`
under `/api/`. There is one exception: `POST /api/sync/ingress` continues to use
`stfc-sync-token`.

The server accepts two headers. Use one of them:

- `Authorization: Bearer <token>`
- `X-Api-Key: <token>`

**The loopback address by default.** Set `KOBAYASHI_API_KEY_TRUST_LOOPBACK` to `1`, `true`,
or `yes`. This is the default. A client whose **TCP peer address** is a loopback address
(`127.0.0.1` or `::1`) then does not send the key. Thus local development and browser
traffic from the same computer continue to operate, and you do not put a secret in the
frontend.

Set `KOBAYASHI_API_KEY_TRUST_LOOPBACK=0` (or `false` or `no`) to make the key necessary for
a loopback client also. This is useful when you test the header path locally.

### The browser interface and the API keys

The React application does not read the API key from a build-time environment variable. Do
not put a long-life secret in `frontend/dist`. Use one of these methods:

- **The same computer, with trust for the loopback address.** Keep the default value of
  `KOBAYASHI_API_KEY_TRUST_LOOPBACK`. You do not change the browser.
- **A reverse proxy in front of the API.** The proxy ends the TLS connection. It then adds
  `Authorization` or `X-Api-Key` to each request that it sends to the server.
- **A LAN client that must send the key.** Configure the client or the proxy to add the
  header. Never commit the key to the repository.

## Related environment variables

| Variable                           | Purpose                                                                |
| ---------------------------------- | ---------------------------------------------------------------------- |
| `KOBAYASHI_BIND`                   | The address to listen on. The default is `127.0.0.1:3000`.             |
| `KOBAYASHI_API_KEY`                | The optional shared secret for the mutating `/api/` routes. It does not apply to sync ingress. |
| `KOBAYASHI_API_KEY_TRUST_LOOPBACK` | When the value is `1` (the default), a loopback client does not send the key. |
| `KOBAYASHI_LOG`                    | The alias for the log level or the log filter of Kobayashi (`info`, `debug`, or a full filter). |
| `RUST_LOG`                         | The full tracing filter. It has precedence over `KOBAYASHI_LOG`.       |

For the sync tokens of the mod, refer to [SYNC.md](SYNC.md). For the general instructions to
run the server, refer to [README.md](../README.md).

## Structured logs and tracing

The server writes the logs as newline-delimited JSON. It uses `tracing` and
`tracing-subscriber`.

- The log of each request has these fields and others: `method`, `matched_path`, `status`,
  and `latency_ms`.
- The log of an optimize job has the fields `job_id`, `seed`, `requested_strategy`,
  `effective_strategy`, and the phase of the progress (`heuristics`, `monte_carlo`,
  `tiered_scout`, `tiered_confirm`, or `genetic`).
- The log of a simulation batch has the fields `batch_index`, `batch_total`, `batch_start`,
  `batch_end`, and the candidate counts.

### How to set the log level

```bash
# Common operator default
KOBAYASHI_LOG=info ./target/release/kobayashi serve

# Full filter syntax (overrides KOBAYASHI_LOG when set)
RUST_LOG='warn,kobayashi=info,tower_http=info' ./target/release/kobayashi serve
```

### `jq` examples

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
