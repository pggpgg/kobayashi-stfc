# ⚔ KOBAYASHI

**Komprehensive Officer Battle Analysis: Your Assets Simulated against Hostiles Iteratively**

A high-performance Monte Carlo combat simulator and crew optimizer for [Star Trek Fleet Command](https://www.startrekfleetcommand.com/). Runs locally, uses every CPU core, ranks crews by the metrics that actually matter — no guesswork.

[![CI](https://github.com/pggpgg/kobayashi-stfc/actions/workflows/ci.yml/badge.svg)](https://github.com/pggpgg/kobayashi-stfc/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 2021](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

![KOBAYASHI Workspace](docs/web-ui-screenshot.png)

> *The only way to win the Kobayashi Maru is to change the conditions of the test.*

---

## Contents

- [Why KOBAYASHI](#why-kobayashi)
- [Quick start](#quick-start)
  - [For players](#for-players)
  - [For contributors](#for-contributors)
- [How it works](#how-it-works)
- [Project layout](#project-layout)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [Acknowledgments](#acknowledgments)
- [License](#license)

---

## Why KOBAYASHI

STFC has **280+ officers** and growing. Each ability interacts with slot, rank, synergies, the hostile you're fighting, your ship's tier and level, your research, buildings, reputation, artifacts, exocomps, and forbidden tech. Finding the optimal crew by hand is a combinatorial nightmare.

KOBAYASHI solves this by brute force: thousands of fights per crew, every viable permutation, ranked by the metrics you choose — round-1 kill rate, win rate, hull remaining, fights before repair. Sub-microsecond per fight, embarrassingly parallel, deterministic per seed.

**Highlights**

- Monte Carlo combat engine modelling crits, procs, shield mitigation, armor, ability timing, decay/accumulate effects, and on-kill triggers
- Three optimizer strategies — exhaustive sweep, tiered scout→confirm, genetic algorithm — auto-selected from search-space size
- **LCARS** (Language for Combat Ability Resolution & Simulation): officer abilities live in declarative YAML, no code changes needed to add new officers
- Player profile bonuses for research, buildings, reputation, artifacts, exocomps, and forbidden tech
- Local-first: release archives bundle the server, web UI, runtime game data, and a starter profile; no Docker, no cloud

---

## Quick start

### For players

Prebuilt binaries for Linux x86_64, macOS arm64, and Windows x86_64 are published on every tagged release.

1. Grab the archive for your OS from [Releases](https://github.com/pggpgg/kobayashi-stfc/releases), verify it against `SHA256SUMS` ([deployment notes](docs/DEPLOYMENT_SECURITY.md)), and extract it. The archive is self-contained; no repository checkout or build toolchain is required.
2. From the extracted folder:

   ```bash
   ./kobayashi serve
   # Windows PowerShell: .\kobayashi.exe serve
   ```

3. Open <http://localhost:3000>.

Choose **Guided** mode in the left rail for a focused scenario → crew → run → results walkthrough. It uses your roster and hides advanced optimizer tuning until you switch back to Roster or Sandbox mode.

To pull your roster from the game in near-real time, set up the [STFC Community Mod sync](docs/SYNC.md). For LAN or internet exposure, read [Deployment & security](docs/DEPLOYMENT_SECURITY.md) — the API has no authentication on its default loopback bind.

### For contributors

```bash
git clone https://github.com/pggpgg/kobayashi-stfc
cd kobayashi-stfc
cargo build --release
(cd frontend && npm ci && npm run build)
./target/release/kobayashi serve          # http://localhost:3000

cargo test                                # backend tests
./scripts/local-ci.sh                     # full CI parity, locally
```

- **Pre-commit, CI parity, branch protection** — [CONTRIBUTING.md](CONTRIBUTING.md)
- **Officer LCARS definitions** — [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md)
- **Full CLI, env vars, ops tuning** — [CLAUDE.md](CLAUDE.md)
- **Documentation index** — [docs/README.md](docs/README.md)

---

## How it works

**Combat engine.** A deterministic, zero-allocation fight loop in Rust. Each simulation takes well under a microsecond. Decay and accumulate buffs, on-kill triggers, extra-attack procs, shield→hull overflow on shield break, apex and isolytic mechanics, and player-profile bonuses are all modelled. Same seed → same outcome, every time. A small Python reference in [tools/combat_engine](tools/combat_engine/) mirrors the core math and is exercised in CI.

**Optimizer.** Given a ship and a hostile, the optimizer searches the crew space. With `strategy` omitted on `/api/optimize`, the server auto-picks tiered vs exhaustive from the effective candidate count after warm-start and constraints. `strategy: "genetic"` runs a GA for very large spaces; `strategy: "tiered"` runs a cheap scout pass over all candidates then a confirmation pass over the top K. Synergy-tagged crews are simulated first, so cancelling early still leaves you with the best results found.

**LCARS officers.** Officers live in [`data/officers/officers.lcars.yaml`](data/officers/), generated from a maintainer-curated canonical JSON. The resolver collapses YAML into a `BuffSet` at load time; the hot loop only evaluates dynamic effects (decay, accumulate, proc). Unknown effect types are logged and skipped — officers can ship before the engine fully supports every mechanic. Full grammar in [docs/DESIGN.md § LCARS](docs/DESIGN.md#3-lcars-language-specification); contributing officers in [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md).

**Player profile.** Non-officer bonuses (research, buildings, reputation, artifacts, exocomps, forbidden tech) merge into a profile applied as a pre-combat modifier layer. Sync your roster via [STFC Community Mod](docs/SYNC.md) or import a Spocks.club export from the Roster page. Request-scoped support buffs (Cerritos, Defiant reinforcement, Titan-A Fortification, …) are configured per simulation, not persisted to the profile.

Deeper coverage of combat formulas, optimizer theory, and the data pipeline lives in [docs/](docs/README.md).

---

## Project layout

```
kobayashi/
├── src/
│   ├── data/         data models (officers, ships, hostiles, profiles)
│   ├── lcars/        LCARS parser, validator, resolver
│   ├── combat/       fight loop, PRNG, buff stacking
│   ├── optimizer/    exhaustive / tiered / genetic, ranking
│   ├── parallel/     Rayon thread pool, batch distribution
│   └── server/       Axum HTTP server (REST + SSE; no WebSocket)
├── data/             officers, ships_extended, hostiles, buildings, registry, heuristics
├── profiles/         per-player sync + imports; only profiles/demo/ is shareable
├── frontend/         React + Vite SPA (built to frontend/dist)
├── tools/            Python combat reference + CI checks
├── tests/            integration tests + recorded fight fixtures
└── docs/             design, deployment, performance, LCARS, data pipeline
```

**Architecture at a glance.** Tokio + Axum 0.7 multi-threaded runtime; REST-first with Server-Sent Events for optimize-job progress; CPU-heavy handlers run under `spawn_blocking` behind a process-wide admission semaphore (`KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`). The React SPA is built separately and served from `frontend/dist`. At startup the binary discovers runtime assets from `KOBAYASHI_HOME`, the working directory, or the executable directory, so extracted release bundles work without a checkout. Deep dive: [CLAUDE.md § Architecture](CLAUDE.md) and [docs/DESIGN.md](docs/DESIGN.md).

---

## Contributing

PRs welcome. Three good ways in:

- **Code** — build, test, and CI parity in [CONTRIBUTING.md](CONTRIBUTING.md). Every PR runs `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, the frontend Biome+tsc+Vitest+build pipeline, and `cargo audit`.
- **Officer definitions** — every officer is YAML. See [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md) for the schema, mapping tables, and the `generate_lcars` workflow.
- **Real fight data** — the combat engine calibrates against recorded fights in `tests/fixtures/recorded_fights/`. If you can record damage-per-round, rounds-to-kill, or full fight logs, open an issue or PR with the fixture.

Bug reports — especially *"the optimizer ranks X but in-game Y wins"* — are valuable. Please include your crew, ship, hostile, and player profile.

---

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for future work and planning priorities, and [docs/NOT_ROADMAP.md](docs/NOT_ROADMAP.md) for explicit non-goals.

---

## Acknowledgments

- Inspired by [tu_optimize](https://github.com/zachanassian/tu_optimize), the Monte Carlo deck optimizer for Tyrant Unleashed.
- The STFC community for reverse-engineering combat formulas.
- The name references the [Kobayashi Maru](https://memory-alpha.fandom.com/wiki/Kobayashi_Maru_scenario) — because the only way to win is to change the conditions of the test.

---

## License

[MIT](LICENSE).

*⚔ Live long and optimize.*
