# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Summary

KOBAYASHI is a Monte Carlo combat simulator and crew optimizer for Star Trek Fleet Command (STFC). It's written in Rust with a React frontend. Officers are described in LCARS (Language for Combat Ability Resolution & Simulation), a declarative YAML DSL — no code changes needed to add or update officers.

## Commands

### Rust backend

```bash
# Build (dev)
cargo build

# Build (release — required for performance benchmarks)
cargo build --release

# Run all tests
cargo test

# Run a specific test
cargo test <test_name>

# Lint
cargo clippy --all-targets

# Start the web server (run from project root so it can find frontend/dist and data/)
./target/release/kobayashi serve
# Binds to 127.0.0.1:3000 by default; override with KOBAYASHI_BIND env var

# Optional simulation/profile tuning (see src/data/profile.rs):
#   KOBAYASHI_FT_LEVEL_TIER_SCALING=1 — scale forbidden-tech catalog bonuses by synced tier/level

# CPU footprint (process-wide; restart server after changing):
#   KOBAYASHI_RAYON_THREADS=<n> — cap Rayon’s global pool (Monte Carlo / optimizer). Omit or 0 = all logical CPUs.
#   KOBAYASHI_LOW_PRIORITY=1 — Windows only: SetPriorityClass(BELOW_NORMAL) for the whole process (keeps UI snappier; does not replace a thread cap).
#   KOBAYASHI_MAX_CONCURRENT_CPU_JOBS=<n> — server: max concurrent CPU-heavy routes at once (simulate, compare, optimize, replay-seed, optimize/start thread; default 1).
#   Raising this above 1 runs multiple optimizers/simulations in parallel; they still share one global Rayon pool — tune KOBAYASHI_RAYON_THREADS to avoid CPU oversubscription.
#   KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS=<n> — optional: read at server start; if set to a positive millisecond value, acquiring a CPU slot waits at most that long, then returns HTTP 503 with code cpu_busy and Retry-After. Unset or 0 = wait indefinitely (backward compatible). Restart after changing.
# Background optimize jobs use POST /api/optimize/start (detached thread); they still share the same Rayon pool and process priority as the server.
# Integration tests and Criterion benches that use Rayon before init_from_env runs cannot change the thread count; use default or run those binaries in isolation.

# CLI usage
./target/release/kobayashi simulate <rounds> <seed>
./target/release/kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]
./target/release/kobayashi optimize --ship <id> --hostile <id> --sims <n> [--max-candidates <n>]
./target/release/kobayashi import <path.txt|path.json>
./target/release/kobayashi validate [data/officers/officers.canonical.json]

# Validate LCARS officer definitions
./target/release/kobayashi validate data/officers

# Regenerate LCARS monolith (`officers.lcars.yaml` under --output)
# After upstream officer cache refresh: python3 scripts/normalize_officer_id_strings.py
cargo build --bin generate_lcars && ./target/release/kobayashi generate-lcars [path/to/officers.canonical.json] [--output data/officers]

# Run benchmarks
cargo bench
```

### CI parity (local)

GitHub Actions workflow **CI** (`.github/workflows/ci.yml`) runs fmt, clippy (`-D warnings`), tests, release build, and `cargo audit` for Rust; frontend runs Biome, `tsc`, Vitest, and production build under `frontend/`.

```bash
# Optional: pre-commit (install: pip install pre-commit; once: pre-commit install)
pre-commit run --all-files

# Full Rust + frontend checks like CI (see CONTRIBUTING.md)
./scripts/local-ci.sh
```

### Frontend

```bash
cd frontend
npm install
npm run build    # outputs to frontend/dist, served by the Rust server
npm run dev      # Vite dev server (hot reload; API calls still go to the Rust server)
npm run test     # Vitest tests
```

### Utility binaries (data maintenance)

```bash
cargo run --bin normalize_stfc_data
cargo run --bin validate_data
cargo run --bin merge_lcars   # optional: merge legacy per-faction *.lcars.yaml shards only
cargo run --bin import_forbidden_chaos
cargo run --bin import_syndicate_reputation
cargo run --bin generate_officer_scorecard   # docs/OFFICER_MODELING_SCORECARD.md — edit fidelity in data/officers/officer_modeling_fidelity.yaml
```

### Ship data (data-stfc.space)

Cache per-ship JSON (default: missing-only; `--full` refreshes all ids in `summary-ship.json`):

```bash
node scripts/fetch_stfcspace_ships.mjs
```

```bash
# 1. Build ship_id_registry from summary-ship + translations (run when upstream ships change)
python3 scripts/build_ship_registry.py

# 2. Normalize all ships to extended format (tiers + levels)
cargo run --bin normalize_data_stfc_space
```

### Hostile data (data.stfc.space cache)

Populate upstream detail JSON from the API (default: only **missing** files; `**--full`** re-downloads every id in `summary-hostile.json`):

```bash
node scripts/fetch_stfcspace_hostiles.mjs
```

Requires `data/upstream/data-stfc-space/hostiles/*.json`. Writes `data/hostiles/` (numeric string ids) and merge-updates `data/registry.json` for the `hostiles` entry only.

```bash
cargo run --bin normalize_hostiles_stfc_space
# Optional: STFCSPACE_HOSTILES_VERSION, STFCSPACE_HOSTILES_SOURCE_NOTE
```

### Research catalog (data.stfc.space)

Caches per-`rid` JSON under `data/upstream/data-stfc-space/research/` (tracked; refetch with `fetch_stfcspace_research.mjs`), then writes `data/research_catalog.json`.

```bash
node scripts/fetch_stfcspace_research.mjs   # default missing-only; --full to refresh all; or supply research/*.json by other means
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0   # reads local research/*.json only
# Subset: --limit N, --rid a,b  |  Inspect unmapped buff ids: --dump-unmapped
# Heuristic CSV hints (human review): node scripts/suggest_research_buff_mappings.mjs
```

## Architecture

### Backend (Rust)

The library is at `src/lib.rs` and exposes these modules:

- `**src/combat/**` — Core fight loop (`engine.rs`). This is the hot path: zero allocations, no dynamic dispatch, SplitMix64 PRNG. `abilities.rs` evaluates effects per round; `buffs.rs` implements stacking rules; `stacking.rs` handles the base→flat→pct→multiply→cap resolution order.
- `**src/lcars/**` — LCARS YAML parser (`parser.rs`) and resolver (`resolver.rs`) that collapses officer definitions into a `BuffSet` (static buffs + per-round effects + triggered effects). Only files matching `*.lcars.yaml` are loaded from a directory.
- `**src/optimizer/**` — `monte_carlo.rs` runs N simulations per crew; `crew_generator.rs` enumerates candidates (optional **pool narrowing** from search constraints on the registry path); `genetic.rs` is the GA strategy (`strategy: "genetic"`); `tiered.rs` is scout → confirm top K (`strategy: "tiered"`, optional `tiered_scout_sims` / `tiered_top_k`, respects `ship_tier`/`ship_level`). Omitting `strategy` on optimize auto-picks tiered vs exhaustive from **effective** candidate count after warm-start + constraints (`count_effective_optimize_candidates`, `src/server/api/execution.rs`). `warm_start_crews` prepends deduped crews before generation. Omitted `**analytical_prefilter_keep`** may use `**analytical_prefilter_keep_auto`** (`src/optimizer/mod.rs`). `ranking.rs` scores by win_rate, hull_remaining, r1_kill_rate.
- `**src/data/**` — Data loading/validation. Ships from `data/ships_extended/` (extended schema with tiers/levels, Option B); hostiles from `data/hostiles/index.json` + per-hostile JSON; buildings from `data/buildings/index.json`. Officers: `officers.canonical.json` is the maintainer-curated catalog; `officers.lcars.yaml` is generated from it (`generate_lcars`) and is the combat YAML the sim loads. `loader.rs` resolves by id (e.g. data.stfc.space numeric string `2918121098`) or by normalized hostile name + level (e.g. `hostile_2918121098_81` for placeholder display names).
- `**src/server/**` — Tokio + Axum 0.7 HTTP server. `mod.rs` spins up a multi-thread Tokio runtime; `routes.rs` defines the Axum `Router` with async handlers. CPU-bound work uses `tokio::task::spawn_blocking`; a process-wide semaphore (`cpu_admission.rs`, `KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`) limits concurrent CPU-heavy routes, with optional bounded wait → HTTP 503 `cpu_busy` (`KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS`). **REST-first** plus **SSE** for optimize job progress (`GET /api/optimize/jobs/:job_id/stream`); no WebSocket. Serves the React SPA from `frontend/dist` when present. API routes in `routes.rs`; handler logic in `api.rs`; sync ingress in `sync.rs`.
- `**src/parallel/`** — Rayon thread pool integration; each thread owns its PRNG instance.
- `**src/cli.rs**` — CLI dispatch (used by tests via `run_with_args`); `src/main.rs` is the binary entry point.

**Key constraint:** Always run the server from the project root so it resolves `frontend/dist` and `data/` correctly.

### Data layout

```
data/
├── officers/
│   ├── officers.lcars.yaml        # LCARS combat definitions (from generate_lcars)
│   ├── officers.canonical.json    # Maintainer catalog; normalize_officer_id_strings.py + manual edits
│   ├── id_registry.json           # Officer id → canonical id mapping
│   └── name_aliases.json          # Name normalization aliases
├── ships_extended/                # Extended schema: index.json + <id>.json (tiers, levels)
├── hostiles/index.json + per-hostile JSON
├── buildings/index.json + per-building JSON
├── registry.json                  # Top-level data registry
└── import/                        # Imported roster files
```

### Frontend (React + Vite + TypeScript)

SPA at `frontend/src/`. Key components in `frontend/src/components/`:

- `CrewBuilder.tsx` — officer slot selection
- `OptimizePanel.tsx` — optimization run configuration
- `SimResults.tsx` — ranked crew results table
- `Workspace.tsx` / `WorkspaceHeader.tsx` — layout shell

API base URL is configurable at build time via `VITE_API_BASE` (e.g. `VITE_API_BASE=/api npm run build`). By default the SPA and API are served from the same origin.

### HTTP API surface (REST + SSE)

```
GET  /api/health
GET  /api/officers          POST /api/simulate
GET  /api/ships             POST /api/optimize   (strategy optional → auto tiered|exhaustive; or "tiered"|"genetic"|"exhaustive")
GET  /api/hostiles          GET  /api/heuristics
GET  /api/profile           PUT  /api/profile
GET  /api/profile/buildings-summary
POST /api/officers/import   POST /api/optimize/start  (async job)
                            GET  /api/optimize/status/:job_id
                            GET  /api/optimize/jobs/:job_id/stream   (SSE)
                            POST /api/optimize/jobs/:job_id/cancel
GET  /api/sync/status       POST /api/sync/ingress
GET  /api/optimize/estimate
GET  /api/data/version
GET  /api/presets           POST /api/presets
GET  /api/presets/:id
```

### LCARS officer definition format

Officer abilities are YAML with `type`, `stat`, `operator`, `value`, `trigger`, `duration`, `scaling`, optional `condition`, `decay`, and `accumulate` fields. See `docs/DESIGN.md` for the full spec. Effect resolution order per round: passive → round_start → per-sub-round (attack/defense) → round_end → burning tick → cleanup.

Unknown effect types are skipped with a warning (graceful degradation).

## Testing

- Backend integration tests live in `tests/` (not `src/`). Run all with `cargo test`.
- Combat calibration tests use fixtures in `tests/fixtures/recorded_fights/` — real fight data from in-game.
- Frontend tests run with `npm run test` in `frontend/`.
- CI runs: `cargo test`, `cargo build --release`, `cargo clippy --all-targets`, and the frontend build+test.

### Heuristics seeds

Community-known crew lists stored in `data/heuristics/*.txt`. Format: `label:Captain,Bridge1,Bridge2:BD1,BD2,...` (lines starting with `#` are comments). These are simulated first before the normal optimizer runs.

- `**src/data/heuristics.rs**` — parser, name resolution (alias lookup → exact → substring), and BD expansion logic
- **BD strategies**: `Ordered` (take first k from list) or `Exploration` (try all C(n,k) combinations) — passed as `below_decks_strategy: "ordered"|"exploration"` in the API request
- `**GET /api/heuristics`** — lists available seed file stems from `data/heuristics/`
- `**POST /api/optimize/start`** — same body as `/api/optimize`: `heuristics_seeds`, `heuristics_only`, `below_decks_strategy`, optional `fast_discovery` (merge expanded seeds into warm-start for the main optimize path); optional `tiered_scout_sims`, `tiered_top_k` (tiered), `warm_start_crews` (deduped prepend), `chain`, `support_buffs`, `analytical_prefilter_keep`, etc.
- Officer names in seed files are resolved case-insensitively via `data/officers/name_aliases.json` then fuzzy substring match

## Key architectural decisions that were made before and that Claude can challenge

- **Axum + Tokio**: Axum 0.7 on a multi-threaded Tokio runtime. CPU-heavy handlers use `spawn_blocking` so they do not stall other requests. The public `run_server()` entry point is synchronous and creates the runtime internally, keeping the CLI unchanged. Concurrent CPU-heavy work is **admission-controlled**: a shared `Semaphore` (`KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`, default 1) limits how many simulate/optimize/compare/replay-seed routes (and the optimize/start worker thread) run at once; optional `KOBAYASHI_CPU_JOB_QUEUE_WAIT_MS` caps wait time for a slot and returns **503** `cpu_busy` with **Retry-After** when set positive. This is not a durable multi-tenant job queue—background optimize jobs remain detached threads with JSON/SSE status polling.
- **Data freshness**: ship and hostile data is sourced from community databases and may lag behind in-game updates. `data.stfc.space` provides raw game JSON (e.g. `/hostile/summary.json`, `/hostile/{id}.json`) and is a promising avenue for automated data refresh.
- **Optimizer strategies**: the SPA defaults to **tiered**; omitting `strategy` on optimize lets the server **auto-route** tiered vs exhaustive from **effective** candidate count (warm-start + constraints; response exposes `effective_strategy` / `strategy_auto`). Explicit `strategy: "exhaustive"`, `"genetic"`, or `"tiered"` still supported. Tiered uses optional `tiered_scout_sims` / `tiered_top_k` and `warm_start_crews`. SPA warm-start persistence uses a versioned localStorage key (`frontend/src/lib/optimizeWarmStart.ts`, schema bumps invalidate old cache).
- **LCARS as source of truth**: officer abilities are defined in YAML, not code. The engine resolves YAML → `BuffSet` before the fight loop; only dynamic effects (decay, accumulate, proc) are evaluated inside the loop.
- **SplitMix64 PRNG**: deterministic per seed, one instance per Rayon thread. Same seed → same fight outcome.
- **Data provenance**: `ships_extended/index.json` and `hostiles/index.json` carry `data_version` and `source_note` fields documenting the upstream source.

