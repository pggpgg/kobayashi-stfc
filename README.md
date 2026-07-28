# ⚔ KOBAYASHI

**Komprehensive Officer Battle Analysis: Your Assets Simulated against Hostiles Iteratively**

KOBAYASHI is a Monte Carlo combat simulator and crew optimizer for
[Star Trek Fleet Command](https://www.startrekfleetcommand.com/). It runs on your computer.
It uses all CPU cores. It ranks each crew by the metrics that you select.

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

STFC has more than 280 officers, and the game adds more officers regularly. The result of
an officer ability changes with the crew slot, the officer rank, the other officers in the
crew, and the hostile. The ship tier, the ship level, and the player profile also change
the result. The player profile contains the research, the buildings, the reputation, the
artifacts, the exocomps, and the forbidden tech. Therefore you cannot find the best crew
manually.

KOBAYASHI solves this problem with a large number of simulations. It simulates thousands
of fights for each crew and ranks the results. You select the metric: round-1 kill rate, win rate, hull
remaining, or the number of fights before a repair. One fight takes less than one
microsecond. The fights run in parallel on all cores. The same seed always gives the same
result.

**Highlights**

- A Monte Carlo combat engine that models critical hits, procs, shield mitigation, armor,
  ability timing, decay effects, accumulate effects, and on-kill triggers.
- Three optimizer strategies: an exhaustive sweep, a tiered scout pass with a confirmation
  pass, and a genetic algorithm. The server selects the strategy from the size of the
  search space.
- **LCARS** (Language for Combat Ability Resolution and Simulation). Officer abilities are
  declarative YAML. You can add a new officer without a change to the code.
- Player profile bonuses for the research, the buildings, the reputation, the artifacts,
  the exocomps, and the forbidden tech.
- Local-first operation. A release archive contains the server, the web interface, the
  runtime game data, and a starter profile. You do not need Docker, and you do not need a
  cloud account.

---

## Quick start

### For players

Each tagged release gives prebuilt binaries for Linux x86_64, macOS arm64, and Windows
x86_64.

1. Download the archive for your operating system from
   [Releases](https://github.com/pggpgg/kobayashi-stfc/releases).
2. Check the archive against `SHA256SUMS`. Refer to the
   [deployment notes](docs/DEPLOYMENT_SECURITY.md).
3. Extract the archive. The archive contains all the necessary files. You do not need a
   repository checkout or a build toolchain.
4. Start the server from the folder that you extracted:

   ```bash
   ./kobayashi serve
   # Windows PowerShell: .\kobayashi.exe serve
   ```

5. Open <http://localhost:3000>.

Select **Guided** mode in the left rail to get a walkthrough with four steps: scenario,
crew, run, and results. Guided mode uses your roster. It hides the advanced optimizer
controls until you select Roster mode or Sandbox mode again.

To get your roster from the game in near-real time, set up the
[STFC Community Mod sync](docs/SYNC.md). Before you make the server available on a LAN or
on the internet, read [Deployment and security](docs/DEPLOYMENT_SECURITY.md). The API has
no authentication on the default loopback bind.

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

- **Pre-commit hooks, CI parity, and branch protection** — [CONTRIBUTING.md](CONTRIBUTING.md)
- **Officer LCARS definitions** — [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md)
- **The full CLI, the environment variables, and the operations controls** — [CLAUDE.md](CLAUDE.md)
- **The documentation index** — [docs/README.md](docs/README.md)
- **The documentation style guide** — [docs/STYLE_STE100.md](docs/STYLE_STE100.md)

---

## How it works

**The combat engine.** The fight loop is deterministic, and it makes no allocation. It is
written in Rust. One simulation takes much less than one microsecond. The engine models
decay buffs, accumulate buffs, on-kill triggers, extra-attack procs, and the overflow from
the shield to the hull when the shield breaks. It also models the apex mechanics, the
isolytic mechanics, and the player profile bonuses. The same seed always gives the same
result. A small Python reference in [tools/combat_engine](tools/combat_engine/) does the
same core calculations, and CI runs it.

**The optimizer.** You give the optimizer a ship and a hostile, and it searches the crew
space. If you do not set `strategy` on `/api/optimize`, the server selects the tiered
strategy or the exhaustive strategy. It selects from the effective candidate count after
it applies the warm start and the constraints. `strategy: "genetic"` runs a genetic
algorithm for a very large space. `strategy: "tiered"` runs a cheap scout pass over all
the candidates, then a confirmation pass over the best K candidates. The optimizer
simulates the crews with synergy tags first. Thus you keep the best results that it found
if you stop the run early.

**The LCARS officers.** The officers are in
[`data/officers/officers.lcars.yaml`](data/officers/). A maintainer keeps a canonical JSON
file, and the tool generates the YAML from it. At load time the resolver collapses the
YAML into a `BuffSet`. The hot loop then evaluates only the dynamic effects: decay,
accumulate, and proc. The engine writes a log message for an unknown effect type and skips
it. Therefore you can add an officer before the engine models every mechanic. For the full
grammar, refer to [docs/DESIGN.md § LCARS](docs/DESIGN.md#3-lcars-language-specification).
To contribute an officer, refer to [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md).

**The player profile.** The bonuses that do not come from officers merge into one profile.
These bonuses come from the research, the buildings, the reputation, the artifacts, the
exocomps, and the forbidden tech. The engine applies the profile as a modifier layer
before combat. To get your roster, use the [STFC Community Mod](docs/SYNC.md) sync, or
import a Spocks.club export from the Roster page. Support buffs (Cerritos, Defiant
reinforcement, Titan-A Fortification, and others) apply to one simulation only. The server
does not write them to the profile.

For more data about the combat formulas, the optimizer theory, and the data pipeline,
refer to [docs/](docs/README.md).

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

**The architecture.** The server uses Tokio and Axum 0.7 on a multi-thread runtime. The
API is REST-first, and it uses Server-Sent Events for the progress of an optimize job. A
handler that uses much CPU time runs under `spawn_blocking`. A semaphore for the full
process controls how many of these handlers run together
(`KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`). The React SPA has its own build, and the server
sends it from `frontend/dist`. At start the binary finds the runtime assets in
`KOBAYASHI_HOME`, in the working directory, or in the directory of the executable.
Therefore an extracted release archive runs without a checkout. For more data, refer to
[CLAUDE.md § Architecture](CLAUDE.md) and [docs/DESIGN.md](docs/DESIGN.md).

---

## Contributing

You can send a pull request. There are three good ways to help:

- **Code.** For the build steps, the tests, and the CI parity, refer to
  [CONTRIBUTING.md](CONTRIBUTING.md). Each pull request runs `cargo fmt`,
  `cargo clippy -D warnings`, `cargo test`, `cargo audit`, and the frontend pipeline. The
  frontend pipeline runs Biome, `tsc`, Vitest, and the build.
- **Officer definitions.** Each officer is YAML. For the schema, the mapping tables, and
  the `generate_lcars` operation, refer to
  [docs/LCARS_CONTRIBUTING.md](docs/LCARS_CONTRIBUTING.md).
- **Data from real fights.** The combat engine calibrates against the recorded fights in
  `tests/fixtures/recorded_fights/`. You can record the damage per round, the number of
  rounds to a kill, or a full fight log. Then open an issue or a pull request with the
  fixture.

Bug reports are also of much value. For example, this report is very useful: *"the
optimizer ranks crew X first, but crew Y wins in the game"*. Include your crew, your ship,
the hostile, and your player profile.

---

## Roadmap

For the future work and the planning priorities, refer to
[docs/ROADMAP.md](docs/ROADMAP.md). For the work that the project will not do, refer to
[docs/NOT_ROADMAP.md](docs/NOT_ROADMAP.md).

---

## Acknowledgments

- The idea for this project comes from
  [tu_optimize](https://github.com/zachanassian/tu_optimize), the Monte Carlo deck optimizer
  for Tyrant Unleashed.
- The STFC community found the combat formulas by reverse engineering.
- The name comes from the
  [Kobayashi Maru](https://memory-alpha.fandom.com/wiki/Kobayashi_Maru_scenario), because
  the only way to win is to change the conditions of the test.

---

## License

[MIT](LICENSE).

*⚔ Live long and optimize.*
