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

# CLI usage
./target/release/kobayashi simulate <rounds> <seed>
./target/release/kobayashi optimize --ship <id> --hostile <id> --sims <n> [--max-candidates <n>]
./target/release/kobayashi import <path.txt|path.json>
./target/release/kobayashi validate [data/officers/officers.canonical.json]

# Validate LCARS officer definitions
./target/release/kobayashi validate data/officers

# Regenerate LCARS (runs the generate_lcars binary)
cargo build --bin generate_lcars && ./target/release/kobayashi generate-lcars [path/to/officers.canonical.json] [--output data/officers]

# Run benchmarks
cargo bench
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
cargo run --bin merge_lcars
cargo run --bin import_forbidden_chaos
cargo run --bin import_syndicate_reputation
```

## Architecture

### Backend (Rust)

The library is at `src/lib.rs` and exposes these modules:

- **`src/combat/`** — Core fight loop (`engine.rs`). This is the hot path: zero allocations, no dynamic dispatch, SplitMix64 PRNG. `abilities.rs` evaluates effects per round; `buffs.rs` implements stacking rules; `stacking.rs` handles the base→flat→pct→multiply→cap resolution order.
- **`src/lcars/`** — LCARS YAML parser (`parser.rs`) and resolver (`resolver.rs`) that collapses officer definitions into a `BuffSet` (static buffs + per-round effects + triggered effects). Only files matching `*.lcars.yaml` are loaded from a directory.
- **`src/optimizer/`** — `monte_carlo.rs` runs N simulations per crew; `crew_generator.rs` enumerates candidates; `genetic.rs` is the GA strategy (select via `strategy: "genetic"` in API); `tiered.rs` is a placeholder. `ranking.rs` scores by win_rate, hull_remaining, r1_kill_rate.
- **`src/data/`** — Data loading/validation. Ships from `data/ships/index.json` + per-ship JSON; hostiles from `data/hostiles/index.json` + per-hostile JSON; buildings from `data/buildings/index.json`. Officers: `officers.canonical.json` is canonical; `officers.lcars.yaml` is the LCARS source of truth. `loader.rs` resolves by id or "name_level" (e.g. `explorer_30`).
- **`src/server/`** — Custom blocking TCP HTTP server (no Axum, no Tokio). Single-threaded `TcpListener` accept loop. REST only — no WebSocket. Serves the React SPA from `frontend/dist` when present. API routes in `routes.rs`; handler logic in `api.rs`.
- **`src/parallel/`** — Rayon thread pool integration; each thread owns its PRNG instance.
- **`src/cli.rs`** — CLI dispatch (used by tests via `run_with_args`); `src/main.rs` is the binary entry point.

**Key constraint:** Always run the server from the project root so it resolves `frontend/dist` and `data/` correctly.

### Data layout

```
data/
├── officers/
│   ├── officers.lcars.yaml        # LCARS source of truth for officer abilities
│   ├── officers.canonical.json    # Canonical officer catalog (regenerated from LCARS)
│   ├── id_registry.json           # Officer id → canonical id mapping
│   └── name_aliases.json          # Name normalization aliases
├── ships/index.json + per-ship JSON
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

### REST API surface

```
GET  /api/officers          POST /api/simulate
GET  /api/ships             POST /api/optimize   (strategy: "exhaustive"|"genetic")
GET  /api/hostiles          GET  /api/heuristics
GET  /api/profile           PUT  /api/profile
POST /api/officers/import   POST /api/optimize/start  (async job)
                            GET  /api/optimize/status/:job_id
```

### LCARS officer definition format

Officer abilities are YAML with `type`, `stat`, `operator`, `value`, `trigger`, `duration`, `scaling`, optional `condition`, `decay`, and `accumulate` fields. See `DESIGN.md` for the full spec. Effect resolution order per round: passive → round_start → per-sub-round (attack/defense) → round_end → burning tick → cleanup.

Unknown effect types are skipped with a warning (graceful degradation).

## Testing

- Backend integration tests live in `tests/` (not `src/`). Run all with `cargo test`.
- Combat calibration tests use fixtures in `tests/fixtures/recorded_fights/` — real fight data from in-game.
- Frontend tests run with `npm run test` in `frontend/`.
- CI runs: `cargo test`, `cargo build --release`, `cargo clippy --all-targets`, and the frontend build+test.

### Heuristics seeds

Community-known crew lists stored in `data/heuristics/*.txt`. Format: `label:Captain,Bridge1,Bridge2:BD1,BD2,...` (lines starting with `#` are comments). These are simulated first before the normal optimizer runs.

- **`src/data/heuristics.rs`** — parser, name resolution (alias lookup → exact → substring), and BD expansion logic
- **BD strategies**: `Ordered` (take first k from list) or `Exploration` (try all C(n,k) combinations) — passed as `below_decks_strategy: "ordered"|"exploration"` in the API request
- **`GET /api/heuristics`** — lists available seed file stems from `data/heuristics/`
- **`POST /api/optimize/start`** — accepts `heuristics_seeds: string[]`, `heuristics_only: bool`, `below_decks_strategy`
- Officer names in seed files are resolved case-insensitively via `data/officers/name_aliases.json` then fuzzy substring match

## Key architectural decisions that were made before and that Claude can challenge

- **No Tokio/Axum**: the server is a hand-rolled blocking TCP implementation. This means long-running `optimize` requests block all other requests until complete.
- **Optimizer strategies**: exhaustive is the default; pass `strategy: "genetic"` for large search spaces. Tiered simulation (`tiered.rs`) is a placeholder — not yet wired in.
- **LCARS as source of truth**: officer abilities are defined in YAML, not code. The engine resolves YAML → `BuffSet` before the fight loop; only dynamic effects (decay, accumulate, proc) are evaluated inside the loop.
- **SplitMix64 PRNG**: deterministic per seed, one instance per Rayon thread. Same seed → same fight outcome.
- **Data provenance**: `ships/index.json` and `hostiles/index.json` carry `data_version` and `source_note` fields documenting the upstream source.
