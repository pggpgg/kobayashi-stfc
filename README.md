<div align="center">

# ⚔ KOBAYASHI

**Komprehensive Officer Battle Analysis: Your Assets Simulated against Hostiles Iteratively**

A high-performance Monte Carlo combat simulator and crew optimizer for [Star Trek Fleet Command](https://www.startrekfleetcommand.com/).

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

*Because the only way to win the Kobayashi Maru is to change the conditions of the test.*

</div>

---

## What is this?

STFC has **280+ officers** and growing. Each one has abilities that interact differently depending on slot, rank, synergies, and the hostile you're fighting. Factor in ship stats, research, buildings, reputation, artifacts, exocomps, and forbidden tech — and finding the optimal crew becomes a combinatorial nightmare.

**KOBAYASHI solves this through brute force.** It simulates thousands of fights per crew combination, tests every viable permutation, and ranks them by the metrics you care about: round-1 kill rate, win rate, hull remaining, fights before repair.

It runs locally on your machine, uses all your CPU cores, and gives you answers in seconds — not guesswork.

### Key Features

- **Monte Carlo combat simulation** — models crits, proc chances, shield mitigation, armor, ability timing, and more
- **Smart crew optimization** — synergy-prioritized search, tiered simulation (fast scouting pass → expensive confirmation), genetic algorithm for large search spaces
- **LCARS officer definitions** — every officer ability is described in a declarative YAML-based language, no code changes needed to add new officers
- **Player profile support** — account for your research, buildings, reputation, and other non-officer bonuses
- **Synergy discovery** — manually tag known synergies, and let KOBAYASHI discover new ones from simulation data
- **Multithreaded** — embarrassingly parallel workload distributed across all CPU cores via Rayon
- **Single binary** — download, run, open browser. No Docker, no Node, no dependencies

---

## Quick Start

### Build from source

```bash
git clone https://github.com/YOUR_USERNAME/kobayashi.git
cd kobayashi
cargo build --release
```

### Run

```bash
# Start the web interface
./target/release/kobayashi serve
# → Open http://localhost:3000

# Or use the CLI directly
./target/release/kobayashi optimize \
  --ship saladin \
  --hostile explorer_30 \
  --sims 5000

./target/release/kobayashi simulate \
  --ship saladin \
  --hostile explorer_30 \
  --captain khan \
  --bridge nero \
  --below tlaan \
  --sims 10000
```

### Other commands

```bash
# Validate LCARS officer definitions
./target/release/kobayashi validate --path data/officers
# Emits severity levels:
# - error: malformed effect/condition data
# - warning: recognized mechanic exists but simulation support is partial/planned
# - info: ignored non-combat tags (loot/mining/cargo/etc.)
# Includes per-officer simulation_fidelity so ranking output can flag partial mechanics

```

### Data maintenance policy

The project-maintained officer catalog (full officer list + tier progression) is updated manually by maintainers when the game adds officers. Separately, player-specific owned-roster data is intended to be importable for personalization (including imports sourced from Spocks.club exports).

---

## How It Works

### The Combat Engine

KOBAYASHI's core is a fast, deterministic combat simulator written in Rust. Each fight simulation takes under 1 microsecond. The engine models:

- Round-by-round attack resolution
- Shield mitigation and pierce mechanics
- Shield → hull overflow on shield break
- Armor damage reduction
- Critical hit chance and damage
- Extra attack procs (e.g., Nero's double shot)
- Decaying and accumulating buffs (e.g., Harrison's first-strike damage)
- On-kill triggers (e.g., Mudd's hull repair)
- Player profile bonuses applied as a pre-combat modifier layer

### The Optimizer

Given a ship and a hostile, the optimizer searches the crew space using a tiered approach:

| Phase | Sims per crew | What it does |
|---|---|---|
| **Scouting** | 100–500 | Tests all synergy combos + a sample of others. Keeps top 5%. |
| **Confirmation** | 5,000–50,000 | Full statistical analysis on surviving crews. Final ranking. |
| **Deep Dive** | 100,000+ | Optional. Per-round damage distributions, sensitivity analysis. |

Synergy-tagged crews are tested first, so even if you cancel early, you likely have the best results already.

**Scaling estimate** (280 officers, 3 crew slots, 16-core machine):

| Scenario | Time |
|---|---|
| Full Phase 1 scouting (~800K combos) | ~8 seconds |
| Phase 1 + Phase 2 | ~16 seconds |
| Full exhaustive sweep | ~3 minutes |

### LCARS — The Officer Description Language

**Language for Combat Ability Resolution & Simulation**

Every officer is defined in a declarative YAML file. No code changes needed to add or update officers. The community can contribute definitions via pull requests.

```yaml
officers:
  - id: khan
    name: "Khan Noonien Singh"
    faction: augment
    rarity: epic
    group: "Botany Bay"

    captain_ability:
      name: "Superior Intellect"
      effects:
        - type: stat_modify
          stat: shield_pierce
          target: self
          operator: add
          value: 0.30
          trigger: passive
          duration: permanent
          scaling:
            base: 0.20
            per_rank: 0.025
            max_rank: 5
```

LCARS supports stat modifiers, extra attacks, tags, decay/accumulate effects, conditional triggers, and composable conditions. See [`DESIGN.md`](DESIGN.md#3-lcars-language-specification) for the full spec.

**Graceful degradation**: unknown effect types are logged and skipped — never crashed on. Officers can be defined before the engine fully supports all their mechanics.

### Synergy System

KOBAYASHI treats synergies as a first-class concept:

- **Manual synergies** — tag known combos like Khan + Marcus (shield pierce stacking) or Khan + Nero (alpha strike burst)
- **Learned synergies** — after optimization runs, KOBAYASHI analyzes which officer pairs appear in top results more often than chance predicts, building a co-occurrence matrix that guides future searches
- **Group bonuses** — Botany Bay, Borg, and other officer groups with set bonuses

### Player Profile

Your effective combat stats depend on research, buildings, reputation, artifacts, and more. KOBAYASHI supports two modes:

- **Quick mode** (recommended): enter your total effective bonuses from all non-officer sources
- **Advanced mode** (future): itemize each source individually

```yaml
player_profile:
  name: "MyAccount"
  effective_bonuses:
    weapon_damage: 1.45    # +145% total from research, buildings, etc.
    shield_hp: 1.30
    hull_hp: 1.55
    crit_chance: 0.08
```

---

## Project Structure

```
kobayashi/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── data/                # Data models (officers, ships, hostiles, profiles)
│   ├── lcars/               # LCARS parser, schema validator, ability resolver
│   ├── combat/              # Combat engine (the hot loop), PRNG, buff system
│   ├── optimizer/           # Monte Carlo, tiered sim, genetic algo, ranking
│   ├── parallel/            # Rayon thread pool, batch distribution, progress
│   └── server/              # Axum web server, REST + WebSocket API
├── data/
│   ├── officers/            # LCARS officer definitions (.lcars.yaml)
│   ├── ships.json           # Ship stat sheets
│   ├── hostiles.json        # Hostile stat sheets
│   ├── synergies.json       # Synergy definitions
│   └── profiles/            # Player profiles
├── frontend/                # Web UI (React/Svelte, embedded in binary)
└── tests/                   # Combat validation, LCARS parsing, optimizer regression
```

---

## Contributing

### Adding or updating officers

Officer definitions live in `data/officers/*.lcars.yaml`. To add a new officer:

1. Create or edit the appropriate faction file
2. Follow the [LCARS schema](DESIGN.md#3-lcars-language-specification)
3. Run `kobayashi validate` to check your definition
4. Submit a PR

Contributions for all 280+ officers are very welcome — this is a community effort.

### Validating against real fights

The combat engine needs calibration against real in-game results. If you can record fight outcomes (damage dealt per round, rounds to kill, etc.) and submit them as test fixtures in `tests/fixtures/recorded_fights/`, that's incredibly valuable.

### Reporting issues

If the optimizer's ranking doesn't match your in-game experience, open an issue with your crew, ship, hostile, and player profile. This helps us identify combat formula inaccuracies.

---

## Roadmap

- [x] Combat engine with deterministic PRNG
- [x] LCARS schema and parser
- [x] Monte Carlo simulation runner
- [x] CLI interface
- [ ] LCARS ability resolver (YAML → BuffSet)
- [ ] Tiered optimization with synergy prioritization
- [ ] Crew generator (exhaustive + filtered)
- [ ] Parallel batch execution
- [ ] Web UI on localhost
- [ ] User-owned roster import workflow (e.g., Spocks.club export)
- [ ] Synergy learning from simulation results
- [ ] Genetic algorithm optimizer
- [ ] Chain grinding simulation (multi-fight with carry-over)
- [ ] Armada mode (multi-ship combat)
- [ ] Sensitivity analysis ("what if I promote this officer?")
- [ ] Full 280+ officer LCARS database

---

## Acknowledgments

- Inspired by [tu_optimize](https://github.com/zachanassian/tu_optimize), the Monte Carlo deck optimizer for Tyrant Unleashed
- The STFC community for reverse-engineering combat formulas
- The name references the [Kobayashi Maru](https://memory-alpha.fandom.com/wiki/Kobayashi_Maru_scenario) — because the only way to win is to change the conditions of the test

---

## License

[MIT](LICENSE) — do whatever you want with it.

---

<div align="center">

*⚔ Live long and optimize.*

</div>
