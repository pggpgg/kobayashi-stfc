# KOBAYASHI

**Komprehensive Officer Battle Analysis: Your Assets Simulated against Hostiles Iteratively**

A high-performance Monte Carlo combat simulator and crew optimizer for Star Trek Fleet Command. Locally run, multithreaded, with a web interface on localhost. Inspired by [tu_optimize](https://github.com/zachanassian/tu_optimize) for Tyrant Unleashed.

Officers are described using **LCARS** (Language for Combat Ability Resolution & Simulation), a declarative DSL that allows any officer ability to be defined without code changes.

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Architecture](#2-architecture)
3. [LCARS Language Specification](#3-lcars-language-specification)
4. [Combat Engine](#4-combat-engine)
5. [Player Profile & Bonus Layer](#5-player-profile--bonus-layer)
6. [Optimizer Strategies](#6-optimizer-strategies)
7. [Synergy System](#7-synergy-system)
8. [Parallelism & Performance](#8-parallelism--performance)
9. [Data Import & Spocks.club Integration](#9-data-import--spocksclub-integration)
10. [Frontend & UI](#10-frontend--ui)
11. [Project Structure](#11-project-structure)
12. [Dependencies](#12-dependencies)
13. [Open Questions & Future Work](#13-open-questions--future-work)

---

## 1. Project Overview

### Problem

STFC has ~280 officers (and growing), each with abilities that vary by slot (captain, bridge, below decks), rank, and level. Combat effectiveness depends on the interaction between officers, ship stats, player research, buildings, reputation, artifacts, exocomps, forbidden tech, alliance research, favors, and more. The combinatorial space is enormous and players currently rely on community guides and intuition to pick crews.

### Solution

KOBAYASHI simulates thousands of fights using Monte Carlo methods, testing crew combinations against specific hostiles and ranking them by configurable metrics (round-1 kill rate, win rate, hull remaining, etc.). It uses smart search strategies (synergy prioritization, tiered simulation, genetic algorithms) to handle the massive search space efficiently.

### Design Principles

- **Single binary**: Rust backend with embedded frontend. Download, run, open browser. No Docker, no Node, no dependencies.
- **Community-driven data**: Officers defined in LCARS (YAML), hostiles and ships in JSON. Community contributes definitions via pull requests. Schema validation catches errors automatically.
- **Graceful degradation**: Unknown ability types are logged and skipped, not crashed on. Accuracy improves incrementally as more mechanics are supported.
- **Performance-first**: The combat engine is the hot loop. Zero allocations, no dynamic dispatch, pre-computed buffs. Target: 2–5M simulations/sec/core.

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────┐
│                    FRONTEND                         │
│  React/Svelte on localhost:3000                     │
│  ┌─────────┐ ┌──────────┐ ┌───────────┐            │
│  │  Crew   │ │   Sim    │ │  Synergy  │            │
│  │ Builder │ │ Results  │ │   Graph   │            │
│  └────┬────┘ └────┬─────┘ └─────┬─────┘            │
│       └───────────┼─────────────┘                   │
│              WebSocket + REST                       │
├─────────────────────────────────────────────────────┤
│                  RUST BACKEND                       │
│                                                     │
│  ┌──────────┐  ┌───────────┐  ┌──────────────────┐ │
│  │  Axum    │  │ Optimizer │  │  Combat Engine   │ │
│  │  Server  │──│  Layer    │──│  (hot loop)      │ │
│  └──────────┘  └───────────┘  └──────────────────┘ │
│                      │                    │         │
│  ┌──────────┐  ┌─────┴─────┐  ┌──────────┴───────┐ │
│  │  Import  │  │  Synergy  │  │  LCARS Parser    │ │
│  │ Pipeline │  │  Index    │  │  & Validator     │ │
│  └──────────┘  └───────────┘  └──────────────────┘ │
│                      │                              │
│  ┌───────────────────┴──────────────────────┐      │
│  │  Data Layer: officers.lcars.yaml,        │      │
│  │  ships.json, hostiles.json, profiles.json│      │
│  └──────────────────────────────────────────┘      │
│                                                     │
│  ┌──────────────────────────────────────────┐      │
│  │  Rayon Thread Pool (work-stealing)       │      │
│  │  Each thread: own PRNG, lock-free output │      │
│  └──────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────┘
```

---

## 3. LCARS Language Specification

### 3.1 Overview

LCARS is a YAML-based DSL for describing officer abilities declaratively. Each officer has up to three ability sets (captain, bridge, below_decks), each containing one or more effects. Effects are composed from a vocabulary of primitives.

File extension: `.lcars.yaml`

### 3.2 Primitives

#### Stats (anything the combat engine tracks)

Combat stats: `weapon_damage`, `shield_hp`, `shield_mitigation`, `hull_hp`, `armor`, `crit_chance`, `crit_damage`, `dodge_chance`, `armor_pierce`, `shield_pierce`, `accuracy`, `damage_reduction`, `isolytic_damage`, `burning_damage`, `shield_regen`

Non-combat stats: `repair_speed`, `warp_speed`, `cargo_capacity`, `mining_rate`

The stat list is extensible. The engine ignores stats it doesn't recognize (with a warning).

#### Targets

| Target | Description |
|---|---|
| `self` | The player's ship |
| `enemy` | The hostile / opponent |
| `all_allies` | All friendly ships (armadas) |
| `all_enemies` | All hostile ships |

#### Triggers

| Trigger | When it fires |
|---|---|
| `passive` | Always active |
| `on_combat_start` | Once, before round 1 |
| `on_round_start` | Each round, before attacks |
| `on_attack` | Each time this ship attacks |
| `on_hit` | Each time an attack lands |
| `on_critical` | Each time a critical hit lands |
| `on_shield_break` | When target's shields reach 0 |
| `on_hull_breach` | When target's hull drops below threshold |
| `on_kill` | When this ship destroys a target |
| `on_receive_damage` | When this ship takes damage |
| `on_round_end` | Each round, after attacks |
| `on_combat_end` | Once, after fight resolves |

#### Operators

| Operator | Behavior |
|---|---|
| `add` | Flat addition to stat |
| `multiply` | Multiplicative scaling |
| `set` | Override stat to exact value |
| `min` | Set a floor |
| `max` | Set a ceiling |
| `add_pct_of_max` | Add a percentage of the stat's maximum value |

#### Duration

| Duration | Behavior |
|---|---|
| `permanent` | Lasts entire fight |
| `rounds: N` | Lasts N rounds from activation |
| `stacks: N` | Can stack up to N times |
| `until: <condition>` | Lasts until condition is met |

### 3.3 Effect Types

#### `stat_modify` — the workhorse

Modifies a stat on a target. Supports scaling, decay, accumulation, and conditions.

```yaml
- type: stat_modify
  stat: weapon_damage
  target: self
  operator: multiply
  value: 1.60
  trigger: on_round_start
  duration:
    rounds: 1
  decay:
    type: linear          # linear | exponential
    amount: 0.15          # per round
    floor: 1.0            # minimum value
  scaling:
    base: 1.40            # value at rank 1
    per_rank: 0.05        # added per rank
    max_rank: 5           # effective_value = base + (rank-1) * per_rank
  condition:
    type: stat_below
    stat: shield_hp
    threshold_pct: 0.50
```

#### `extra_attack` — additional shots

```yaml
- type: extra_attack
  chance: 0.50
  multiplier: 1.0         # damage multiplier on extra shot
  trigger: on_attack
  duration:
    rounds: 2
  scaling:
    base_chance: 0.35
    per_rank: 0.0375
    max_rank: 5
```

#### `tag` — non-combat metadata

For effects that don't affect the combat simulation directly (loot bonuses, mining bonuses, etc.) but are useful for crew selection.

```yaml
- type: tag
  tag: loot_bonus
  value: 0.25
  trigger: passive
```

#### `accumulate` — effects that grow over time

```yaml
accumulate:
  type: linear             # linear | exponential | step
  amount: 0.05            # growth per round
  ceiling: 1.50           # maximum accumulated value
```

### 3.4 Conditions

Conditions gate whether an effect activates. They are predicates evaluated by the engine.

| Condition Type | Parameters | Example |
|---|---|---|
| `stat_below` | stat, threshold_pct | Shields below 50% |
| `stat_above` | stat, threshold_pct | Hull above 80% |
| `vs_faction` | faction | Against Romulan hostiles |
| `round_range` | min, max | Only rounds 1–3 |
| `group_count` | group, min_members | 2+ Botany Bay officers |
| `has_tag` | tag | Ally has "federation" tag |

Conditions are composable with `and` / `or` / `not`:

```yaml
condition:
  type: and
  conditions:
    - type: stat_below
      stat: hull_hp
      threshold_pct: 0.50
    - type: round_range
      min: 3
```

### 3.5 Complete Officer Example

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

    bridge_ability:
      name: "Wrath"
      effects:
        - type: stat_modify
          stat: weapon_damage
          target: self
          operator: multiply
          value: 1.15
          trigger: passive
          duration: permanent
          scaling:
            base: 1.08
            per_rank: 0.0175
            max_rank: 5

    below_decks_ability:
      name: "Augmented Blood"
      effects:
        - type: stat_modify
          stat: hull_hp
          target: self
          operator: multiply
          value: 1.10
          trigger: passive
          duration: permanent
```

### 3.6 Resolution Order

Per round, the engine processes effects in this order:

1. Passive effects (always on)
2. `on_round_start` triggers
3. Player attacks → `on_attack` → `on_hit` / `on_critical` triggers
4. Enemy attacks → `on_receive_damage` triggers
5. `on_round_end` triggers
6. Check `on_kill`, `on_shield_break`, `on_hull_breach`

### 3.7 Stacking Rules

- Same stat, same operator from different sources: all apply
- Resolution order: base → flat adds → pct adds → multipliers → caps
- `set` overrides everything (last `set` wins)
- `min` and `max` applied after all other operations

### 3.8 Extensibility & Validation

- Unknown effect types are logged as warnings and skipped (no crash)
- Unknown stats are stored but ignored by the combat engine
- On load, every officer definition is validated against the LCARS schema
- Officers with validation warnings are still usable, flagged in the UI
- This allows the community to define officers before the engine supports all their mechanics

---

## 4. Combat Engine

### 4.1 Design

The combat engine is the hot loop. Every design decision here affects throughput by millions of simulations.

```rust
/// Pure function. No side effects, no allocations.
fn simulate(
    ship: &ShipStats,
    hostile: &HostileStats,
    crew: &ResolvedCrew,      // LCARS abilities pre-resolved to a BuffSet
    player: &PlayerProfile,    // pre-combat modifier layer
    seed: u64,
) -> FightResult
```

Key design constraints:

- **Zero allocations in the hot path**: pre-allocated round buffer, all data on the stack
- **No trait objects or dynamic dispatch in the inner loop**: abilities are resolved to a flat `BuffSet` before combat starts
- **Only abilities with per-round variance** (Nero's double shot, decay/accumulate effects) are evaluated inside the loop; static buffs are pre-computed
- **SplitMix64 PRNG**: ~0.8ns per call, passes BigCrush, deterministic per seed

### 4.2 Pre-combat Resolution

Before the fight loop begins, LCARS definitions are collapsed into a `BuffSet`:

```
LCARS YAML → parsed Officer → ResolvedAbilities → BuffSet
                                                      ├── static_buffs (applied once)
                                                      ├── per_round_effects (evaluated each round)
                                                      └── triggered_effects (evaluated on trigger)
```

Static buffs (passive `stat_modify` with permanent duration) are folded into the ship's effective stats before round 1. Only dynamic effects remain in the loop.

### 4.3 Fight Loop

```
for each round (1..MAX_ROUNDS):
    1. Apply on_round_start effects (decay, accumulate, round-limited buffs)
    2. Compute player effective damage (base × modifiers × crit roll)
    3. Resolve extra_attack chances
    4. Apply damage to enemy (shields → mitigation → overflow → armor → hull)
    5. Check on_hit, on_critical, on_shield_break, on_kill triggers
    6. Compute enemy damage, apply to player
    7. Check on_receive_damage, on_hull_breach triggers
    8. Apply on_round_end effects
    9. Check termination (enemy hull ≤ 0, player hull ≤ 0, max rounds)
```

### 4.4 Output

```rust
struct FightResult {
    win: bool,
    rounds: u8,
    hull_remaining: f32,
    hull_pct: f32,
    damage_dealt_r1: f32,       // for R1 kill optimization
    total_damage_dealt: f32,
    critical_hits: u8,
    double_shots: u8,
    round_log: Option<Vec<RoundSnapshot>>,  // only for sample/replay fights
}
```

### 4.5 Target Throughput

| Metric | Target |
|---|---|
| Single sim, single core | < 1 μs |
| Sims/sec, single core | 2–5 million |
| Sims/sec, 16 cores | 30–80 million |
| Full 800K crew sweep (Phase 1, 500 sims each) | ~8 seconds |
| Full sweep Phase 1 + Phase 2 | ~16 seconds |

---

## 5. Player Profile & Bonus Layer

### 5.1 The Problem

Combat effectiveness depends on a massive stack of multiplicative and additive modifiers from non-officer sources: research trees, station buildings, reputation tiers, alliance research, artifacts, exocomps, forbidden tech, favors, and more. Scopely keeps adding new modifier sources.

### 5.2 Key Insight

Most bonuses collapse into the same handful of stats before combat. The player profile captures effective stat modifiers after all systems are applied.

### 5.3 Quick Mode (MVP)

Player enters their effective total bonuses from all non-officer sources:

```yaml
player_profile:
  name: "MyAccount"
  effective_bonuses:
    weapon_damage: 1.45       # +145% from all non-officer sources combined
    shield_hp: 1.30
    shield_mitigation: 0.05
    hull_hp: 1.55
    armor: 2500               # flat bonus
    crit_chance: 0.08
    crit_damage: 0.20
```

The engine applies these as a pre-combat modifier layer. This gets ~90% accuracy for ~10% of the implementation effort.

### 5.4 Advanced Mode (Future)

Itemized sources, each resolved independently respecting add vs. multiply and any caps or diminishing returns:

```yaml
sources:
  - type: research
    tree: combat
    nodes:
      - { id: "weapon_dmg_1", level: 30, stat: weapon_damage, value: 0.45, operator: add }
  - type: building
    name: "Operations"
    level: 35
    bonuses:
      - { stat: hull_hp, value: 0.20, operator: add }
  - type: reputation
    faction: "Federation"
    tier: 5
    bonuses:
      - { stat: weapon_damage, value: 0.10, operator: add, condition: { vs_faction: "romulan" } }
  - type: exocomp
    bonuses:
      - { stat: crit_damage, value: 0.15, operator: add }
  - type: artifact
    bonuses:
      - { stat: shield_pierce, value: 0.05, operator: add }
  - type: forbidden_tech
    bonuses:
      - { stat: armor, value: 800, operator: add }
  - type: alliance_research
    bonuses:
      - { stat: weapon_damage, value: 0.12, operator: add }
```

### 5.5 Why Quick Mode First

Modeling every individual research node is a huge data entry burden and may not meaningfully change crew *rankings*. It shifts absolute numbers but the relative order of crews tends to stay stable. Quick mode is the pragmatic MVP; advanced mode follows if there's demand.

---

## 6. Optimizer Strategies

### 6.1 Monte Carlo Simulation

The baseline approach. Run N thousand iterations of a given crew vs. a given hostile, with RNG for crit rolls, proc chances, etc. Track win rate, average rounds to kill, average hull remaining, and R1 kill rate. Works well because STFC combat has meaningful randomness.

### 6.2 Analytical / Deterministic Solver

Reduce combat to closed-form math: expected damage per round given stats. Skip simulation entirely and just compute the answer. Dramatically faster, but only works for abilities without complex variance. Useful as a fast pre-filter.

### 6.3 Tiered Simulation (recommended default)

```
Phase 1: "Scouting"
  - 100–500 sims per crew
  - All synergy combos + random sample of others
  - Keep top 5% by primary metric (e.g., R1 kill rate)

Phase 2: "Confirmation"
  - 5,000–50,000 sims per surviving crew
  - Full statistical output (confidence intervals, percentiles)
  - Final ranking with error bars

Phase 3: "Deep Dive" (optional, user-triggered)
  - 100,000+ sims on top 10
  - Per-round damage distribution histograms
  - Sensitivity analysis (what if officer X is +1 rank?)
```

### 6.4 Hill Climbing

Start with a random crew, try swapping one officer at a time, keep the swap if it improves your score, repeat until no single swap helps. Simple and fast, but can get trapped in local optima (a crew that can't be improved by changing one officer, but swapping two simultaneously would find something better).

Mitigations: random restarts, beam search (track top N candidates in parallel).

### 6.5 Genetic Algorithm

For large search spaces (especially with multiple below-decks slots where exhaustive search is impractical):

1. Generate random population of crews
2. Score each via Monte Carlo
3. Breed top performers (swap officers between high-scoring crews)
4. Mutate a few randomly
5. Iterate until convergence

Converges on good solutions much faster than exhaustive search, at the cost of potentially missing the global optimum.

### 6.6 Simulated Annealing

Like hill climbing but with a "temperature" parameter that allows occasionally accepting worse solutions early on, helping escape local optima. Temperature cools over time, gradually locking in. Good middle ground between hill climbing and genetic algorithms.

### 6.7 Bayesian Optimization

Builds a probabilistic model of which crew configurations are likely to score well and strategically picks the next crew to test based on where the model is most uncertain. Very sample-efficient — useful when each simulation is expensive or the search space is vast.

### 6.8 Recommended Approach

**Tiered simulation with synergy prioritization** as the default, with genetic algorithm available for full below-decks optimization. Analytical pre-filtering to prune obviously bad combos before any simulation runs.

---

## 7. Synergy System

### 7.1 Overview

Synergies are a first-class concept in KOBAYASHI. They serve two purposes: guiding the optimizer to try promising combinations first, and helping the player understand *why* certain crews work well together.

### 7.2 Manual Synergies

Known mechanical synergies, tagged by the community:

```yaml
synergies:
  - id: "khan_marcus_pierce"
    name: "Shield Breaker"
    officers: [khan, marcus]
    mechanism: "Both add shield_pierce — stacks to ~45%"
    priority: high

  - id: "khan_nero_burst"
    name: "Alpha Strike"
    officers: [khan, nero]
    mechanism: "Shield pierce + double shot = massive R1 burst"
    priority: high

  - id: "botany_bay_group"
    name: "Botany Bay Crew"
    officers: [khan, harrison, mudd]
    mechanism: "Group bonus: +10% to all abilities"
    group: "Botany Bay"
    bonus:
      - type: stat_modify
        stat: all_ability_values
        operator: multiply
        value: 1.10
        condition:
          min_group_members: 2
```

### 7.3 Learned Synergies

After running a large batch of simulations, KOBAYASHI analyzes which officer *pairs* appear in top-performing crews disproportionately often vs. random baseline. This builds a co-occurrence matrix:

```rust
pub struct SynergyIndex {
    manual: Vec<SynergyTag>,
    learned: CoOccurrenceMatrix,  // built from past simulation runs
}

impl SynergyIndex {
    /// After a batch run, find officer pairs that co-occur
    /// in top-N results more often than chance predicts
    pub fn learn_from_results(&mut self, results: &[RankedCrew]) { ... }
}
```

Over time, this discovers synergies the player (or the community) didn't know about.

### 7.4 Synergy-Prioritized Search

The crew generator yields combinations in priority order:

1. Synergy-tagged combos (manual + learned, high priority first)
2. High-tier officers in novel combinations
3. Exhaustive remainder (if enabled)

This front-loads the most promising candidates, meaning even if the user cancels a long optimization run early, they likely already have the best results.

---

## 8. Parallelism & Performance

### 8.1 Architecture

Each simulation is independent — the problem is embarrassingly parallel. KOBAYASHI uses Rayon's work-stealing thread pool to distribute crew combos across all cores.

- Each thread owns its own PRNG instance (seeded deterministically from crew index)
- Lock-free result collection via crossbeam channel
- Progress updates pushed to frontend via WebSocket every 100ms
- Backpressure: if frontend disconnects, simulations continue, results buffered to disk

### 8.2 Scaling Estimates

For ~280 officers with 3 crew slots:

| Scenario | Combos | Sims | Total Sims | Time (16 cores) |
|---|---|---|---|---|
| Phase 1 scouting | ~800K | 500 each | 400M | ~8 sec |
| Phase 2 top 5% | ~40K | 10K each | 400M | ~8 sec |
| Full sweep | ~800K | 10K each | 8B | ~160 sec |
| With 5 below-decks | billions | — | — | genetic algo needed |

### 8.3 PRNG Choice

SplitMix64: ~0.8ns per call, passes BigCrush, deterministic, trivially seedable per thread. Reproducible results across runs (same seed → same fight outcome).

---

## 9. Data Import & Spocks.club Integration

### 9.1 Import Pipeline

```
1. Accept JSON or CSV upload via /api/officers/import
2. Parse & validate: map officer names to canonical IDs
3. Extract: tier, level, ability ranks → compute effective values
4. Diff against existing database (flag new/changed officers)
5. Store to data/officers.lcars.yaml (or SQLite if it grows)
6. Rebuild synergy index with updated roster
```

### 9.2 Spocks.club

Spocks.club offers a manual export of all officers with their tier and level. KOBAYASHI's MVP supports importing this export to personalize simulations to the player's actual roster.

### 9.3 Future: Direct Sync

If the STFC community mod exposes an API endpoint, add a "sync" button that pulls officer data directly. The architecture is ready — just add another import source in the pipeline.

### 9.4 Community Contribution

Since officers are YAML files following the LCARS spec, a GitHub repository can accept pull requests for new or corrected officer definitions. Schema validation in CI catches errors automatically. This is how tu_optimize's card data was maintained.

---

## 10. Frontend & UI

### 10.1 Delivery

The frontend is a React or Svelte app, built to static files and embedded in the Rust binary via `rust-embed`. No separate frontend server needed.

### 10.2 Styling

LCARS-inspired UI aesthetic: the iconic Star Trek computer interface with rounded rectangles, orange/purple/blue color blocks, and Federation typography. The dashboard should feel like operating a starship's tactical console.

### 10.3 Key Components

| Component | Purpose |
|---|---|
| **CrewBuilder** | Drag-and-drop crew assembly with slot constraints |
| **SimResults** | Results table + charts, sortable by multiple metrics |
| **FightReplay** | Round-by-round visual replay of a sample fight |
| **SynergyGraph** | Network visualization of officer synergies (nodes = officers, edges = synergy strength) |
| **ImportWizard** | Spocks.club data import UI with diff preview |
| **PlayerProfile** | Quick mode bonus entry + advanced mode source editor |
| **OptimizePanel** | Configuration for optimization runs (strategy, sim count, constraints) with live progress |

### 10.4 API

```
GET  /api/officers                  # list all (with filters)
POST /api/officers/import           # upload Spocks.club export
GET  /api/ships                     # list ships
GET  /api/hostiles                  # list hostiles
POST /api/simulate                  # single crew simulation
  → { ship, hostile, crew, num_sims }
  ← { stats, sample_log }
POST /api/optimize                  # find best crews
  → { ship, hostile, constraints, strategy, num_sims }
  ← WebSocket stream: { progress, partial_results, final_ranking }
GET  /api/synergies                 # synergy graph data
POST /api/synergies/learn           # trigger learning from past results
GET  /api/profile                   # player profile
PUT  /api/profile                   # update player profile
```

---

## 11. Project Structure

```
kobayashi/
├── Cargo.toml
├── README.md
├── data/
│   ├── officers/              # LCARS officer definitions
│   │   ├── augments.lcars.yaml
│   │   ├── federation.lcars.yaml
│   │   ├── romulan.lcars.yaml
│   │   ├── klingon.lcars.yaml
│   │   └── ...
│   ├── ships.json
│   ├── hostiles.json
│   ├── synergies.json
│   └── profiles/
│       └── default.yaml
│
├── src/
│   ├── main.rs                # CLI parsing, starts server or batch mode
│   │
│   ├── data/
│   │   ├── mod.rs
│   │   ├── officer.rs         # Officer struct, ability enums, slot constraints
│   │   ├── ship.rs            # Ship stats
│   │   ├── hostile.rs         # Hostile stats + special mechanics
│   │   ├── synergy.rs         # Synergy definitions, co-occurrence matrix
│   │   ├── profile.rs         # Player profile, bonus resolution
│   │   └── import.rs          # Spocks.club parser, LCARS loader, validation
│   │
│   ├── lcars/
│   │   ├── mod.rs
│   │   ├── parser.rs          # YAML → typed LCARS structures
│   │   ├── schema.rs          # Schema definition & validation rules
│   │   ├── resolver.rs        # LCARS abilities → pre-combat BuffSet
│   │   └── errors.rs          # Validation warnings & errors
│   │
│   ├── combat/
│   │   ├── mod.rs
│   │   ├── engine.rs          # Core fight loop (the hot path)
│   │   ├── buffs.rs           # Buff/debuff system, stacking rules
│   │   ├── effects.rs         # Effect evaluation (decay, accumulate, triggers)
│   │   └── rng.rs             # SplitMix64 PRNG
│   │
│   ├── optimizer/
│   │   ├── mod.rs
│   │   ├── monte_carlo.rs     # Monte Carlo runner (N sims → stats)
│   │   ├── crew_generator.rs  # Exhaustive & synergy-prioritized enumeration
│   │   ├── tiered.rs          # Two-pass: scouting → confirmation
│   │   ├── genetic.rs         # Genetic algorithm for large spaces
│   │   ├── analytical.rs      # Closed-form expected damage calculator
│   │   └── ranking.rs         # Multi-metric scoring & ranking
│   │
│   ├── parallel/
│   │   ├── mod.rs
│   │   ├── pool.rs            # Rayon thread pool configuration
│   │   ├── batch.rs           # Crew combo → worker thread distribution
│   │   └── progress.rs        # Progress tracking, ETA, throughput
│   │
│   └── server/
│       ├── mod.rs
│       ├── api.rs             # REST + WebSocket endpoints
│       ├── routes.rs          # Route definitions
│       └── static/            # Embedded frontend
│           └── index.html
│
├── frontend/
│   ├── package.json
│   ├── src/
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── CrewBuilder.tsx
│   │   │   ├── SimResults.tsx
│   │   │   ├── FightReplay.tsx
│   │   │   ├── SynergyGraph.tsx
│   │   │   ├── ImportWizard.tsx
│   │   │   ├── PlayerProfile.tsx
│   │   │   └── OptimizePanel.tsx
│   │   └── lib/
│   │       ├── api.ts
│   │       └── types.ts
│   └── dist/                  # Built → embedded in Rust binary
│
└── tests/
    ├── combat_tests.rs
    ├── lcars_tests.rs         # Parser & validation tests
    ├── optimizer_tests.rs
    └── fixtures/
        ├── officers/          # Test LCARS files
        └── recorded_fights/   # Real fight data for validation
            ├── fight_001.json
            └── ...
```

---

## 12. Dependencies

```toml
[dependencies]
rayon = "1.10"              # Parallel iterators, work-stealing
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"          # LCARS parsing
tokio = { version = "1", features = ["full"] }
axum = "0.7"                # HTTP server
axum-extra = "0.9"          # WebSocket
tower-http = "0.5"          # Static file serving, CORS
crossbeam = "0.8"           # Lock-free channels
rand = "0.8"                # PRNG traits
csv = "1.3"                 # CSV import
clap = "4"                  # CLI args
tracing = "0.1"             # Structured logging
rust-embed = "8"            # Embed frontend in binary
jsonschema = "0.18"         # LCARS schema validation (optional)
```

---

## 13. Open Questions & Future Work

### Open Questions

- **Combat formula accuracy**: STFC's exact formulas aren't public. The engine needs validation against recorded in-game fights. How many recorded fights do we need for confidence?
- **Below-decks slot count**: Real STFC has 2–3 below-decks slots depending on ship tier. This explodes the search space. When do we switch from exhaustive to genetic?
- **Hostile-specific mechanics**: Borg, Eclipse, Swarm, Armada bosses all have special behaviors. How deeply do we model these in LCARS vs. hardcoding?
- **Ability interaction edge cases**: Do some abilities interact in non-obvious ways that LCARS's stacking rules don't capture? Need community testing.

### Future Work

- **Chain grinding simulation**: Model N sequential fights with hull/shield carry-over between fights. Optimize for fights-before-repair, not just single-fight metrics. (Mudd's repair ability becomes relevant here.)
- **Armada mode**: Multi-ship combat with ally-targeting abilities.
- **Sensitivity analysis**: "What if I promote officer X to the next rank? How much does my best crew improve?"
- **Auto-updater**: Check for new LCARS definitions on GitHub and pull updates.
- **GPU acceleration**: Port combat engine to CUDA/WebGPU for billions of sims. Probably overkill but fun.
- **Mobile companion**: PWA version that talks to the desktop KOBAYASHI instance on the local network.
- **Direct account sync**: If community mod exposes an API, pull officer data and research levels directly.
