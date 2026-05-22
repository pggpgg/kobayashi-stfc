# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

## Shipped

- Deterministic combat engine (SplitMix64 PRNG, zero-allocation hot loop)
- Monte Carlo simulation runner with parallel batch execution via Rayon
- LCARS schema, parser, and ability resolver (YAML → `BuffSet`)
- CLI (`serve`, `simulate`, `optimize`, `import`, `validate`, `resolve`, `mitigation-sensitivity`, `sensitivity`)
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
- **Sensitivity analysis** (`/sensitivity`, `POST /api/sensitivity`, CLI `sensitivity`): for a fixed scenario, perturb each in-game stat by one realistic step of investment and rank stats by their measured Δ on a user-chosen outcome metric, with a 95% paired-CRN confidence interval. v1 covers 15 stats (`weapon_damage`, `crit_chance`, `crit_damage`, `armor_piercing`, `shield_piercing`, `accuracy`, `apex_shred`, `isolytic_damage`, `mitigation`, `apex_barrier`, `isolytic_defense`, `crit_damage_reduction`, `hull_hp`, `shield_hp`, `shield_mitigation`).

## Planned

- Synergy learning from simulation results (co-occurrence matrix → bias future searches)
- Armada mode (multi-ship combat)
- **Sobol / Morris sensitivity** — variance-decomposition for first-order, total-order, and pairwise interactions. Answers "what is the best **pair** of stats to invest in together." Morris (~`r × (k+1)` sims) for cheap screening, Sobol (~`N × (2k+2)` sims at `N` ≥ 1024) for stable indices. Build on top of the v1 OAT engine in `src/optimizer/sensitivity.rs`.
- Full LCARS coverage of all 280+ officers (incremental; see [OFFICER_MODELING_SCORECARD.md](OFFICER_MODELING_SCORECARD.md) for fidelity gaps)
- Defender-side support buffs and alliance debuffs as scenario inputs (partial: defender-static support buff keys apply in PvP-shaped scenarios; alliance debuffs not yet scenario inputs)
- Async + SSE for `/api/sensitivity` (v1 is synchronous; long runs are gated by the CPU semaphore).

## Stat modeling improvements

Engine work that would unlock more granular sensitivity analysis (and remove caveats currently flagged in the v1 sensitivity UI):

- ~~**Split mitigation components.** Today `apply_profile_to_attacker` collapses profile bonuses for `armor`, `shield_deflection`, `dodge`, and `damage_reduction` into a single `Combatant.mitigation` scalar ([src/data/profile.rs:2296-2302](../src/data/profile.rs)). Sensitivity v1 surfaces one aggregated `mitigation` row as a result. To split: track each of the four post-resolution, plumb separately through the mitigation / counter-fire path, and expose four distinct `StatKey` variants.~~ **Shipped.** `Combatant` now carries four distinct fields (`armor`, `shield_deflection`, `dodge`, `damage_reduction`); `apply_profile_to_attacker` populates each separately ([src/data/profile.rs](../src/data/profile.rs)). The inbound counter-fire path in [src/combat/engine.rs](../src/combat/engine.rs) weights each by attacker ship-type coefficients (`c_armor`, `c_shield`, `c_dodge`); `damage_reduction` is a flat post-mitigation reduction. Sensitivity catalog grew from 15 → 18 stats with `StatKey::Armor`, `ShieldDeflection`, `Dodge`, `DamageReduction` replacing the aggregated `StatKey::Mitigation`. Aggregated scalar retained as a back-compat fallback when components are all zero (legacy fixtures).
- **Critical Damage Floor as a separate defensive clamp.** In-game, this prevents enemies from debuffing the player's effective crit damage below a fixed minimum. The engine has no such clamp — research nodes named "Critical Damage Floor" are ingested as additive bonuses on `crit_damage` ([data/research_catalog.json](../data/research_catalog.json)). To model correctly: track a separate `crit_damage_floor` value and clamp the effective crit damage at `max(effective, floor)` after opponent debuff resolution. Then expose as a separately perturbable stat.
- **`player_crit_damage_reduction` in sensitivity.** Currently plumbed through a PvP-specific crew-extension mechanism ([src/optimizer/monte_carlo/scenario.rs:205](../src/optimizer/monte_carlo/scenario.rs)) — there's no post-resolution scalar to perturb. v1 ships a universal `crit_damage_reduction` perturbation that rides on top of the resolved crew-derived reduction via `SimulationConfig.crit_damage_reduction_perturb` ([src/combat/engine.rs](../src/combat/engine.rs)); making the player-side path uniform with the hostile-side path would let us drop the per-call config plumb.

## Buildings

Buildings are fully modeled for ship combat. Backlog items tracked here so cross-references from [data/README.md](../data/README.md) and [NOT_ROADMAP.md](NOT_ROADMAP.md) land in one place:

- Station-defense mode in the optimizer (`BuildingMode::StationDefense`), gated on `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) populated from import or mapping. Currently parked in [NOT_ROADMAP.md](NOT_ROADMAP.md) until station defense is in scope.
- Strict validation report for opaque `buff_*` stats not yet mapped into the combat profile; see [building_gaps.md](building_gaps.md) and `data/buildings/buff_id_to_stat.json`.
- Building catalog API + UI panel (currently the catalog is consumed silently during scenario load).

## Forbidden tech

- Per-sub-round vs profile-only timing for forbidden-tech effects (calibration uncertainty; see [data/README.md § Forbidden tech](../data/README.md#forbidden-tech-catalog-and-partial-status)).
- Flat hull/shield HP from research/forbidden-tech rows (no agreed conversion to fractional profile multipliers; currently skipped).
