# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

## Shipped

- Deterministic combat engine (SplitMix64 PRNG, zero-allocation hot loop)
- Monte Carlo simulation runner with parallel batch execution via Rayon
- LCARS schema, parser, and ability resolver (YAML → `BuffSet`)
- CLI (`serve`, `simulate`, `optimize`, `import`, `validate`, `resolve`, `mitigation-sensitivity`)
- Crew generator with exhaustive enumeration and search-constraint pool narrowing
- Tiered optimization (scout → confirm) with optional `tiered_scout_sims` / `tiered_top_k`
- Genetic-algorithm optimizer (`strategy: "genetic"`)
- Strategy auto-selection from effective candidate count after warm-start + constraints
- Web UI on localhost (Workspace, Results Library, Roster & Profile, Data & Mechanics)
- Roster import (Spocks.club JSON, plain `name,tier,level` text)
- Roster sync from the game via STFC Community Mod
- Player profile bonuses: research, buildings, reputation, artifacts, exocomps, forbidden tech
- Request-scoped support buffs (Cerritos, Defiant reinforcement, Titan-A Fortification, …)
- Chain-grinding simulation (N sequential fights, hull carry-over, full shields each link) — optimizer + API + UI
- SSE streaming for long-running optimize jobs
- Process-wide CPU admission control (`KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`, optional bounded queue)
- **PvP mode** (`/pvp`): ship-vs-ship fights with a fixed defender ship + opponent profile; optimize searches attacker crews only ([PVP.md](PVP.md))

## Planned

- Synergy learning from simulation results (co-occurrence matrix → bias future searches)
- Armada mode (multi-ship combat)
- Sensitivity analysis ("what if I promote this officer one rank?")
- Full LCARS coverage of all 280+ officers (incremental; see [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md) for fidelity gaps)
- Defender-side support buffs and alliance debuffs as scenario inputs (partial: defender-static support buff keys apply in PvP-shaped scenarios; alliance debuffs not yet scenario inputs)

## Buildings

Buildings are fully modeled for ship combat. Backlog items tracked here so cross-references from [data/README.md](../data/README.md) and [NOT_ROADMAP.md](NOT_ROADMAP.md) land in one place:

- Station-defense mode in the optimizer (`BuildingMode::StationDefense`), gated on `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) populated from import or mapping. Currently parked in [NOT_ROADMAP.md](NOT_ROADMAP.md) until station defense is in scope.
- Strict validation report for opaque `buff_*` stats not yet mapped into the combat profile; see [building_gaps.md](building_gaps.md) and `data/buildings/buff_id_to_stat.json`.
- Building catalog API + UI panel (currently the catalog is consumed silently during scenario load).

## Forbidden tech

- Per-sub-round vs profile-only timing for forbidden-tech effects (calibration uncertainty; see [data/README.md § Forbidden tech](../data/README.md#forbidden-tech-catalog-and-partial-status)).
- Flat hull/shield HP from research/forbidden-tech rows (no agreed conversion to fractional profile multipliers; currently skipped).
