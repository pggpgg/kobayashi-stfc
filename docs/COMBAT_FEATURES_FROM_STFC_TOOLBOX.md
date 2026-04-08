# Combat Feature Backlog (derived from stfc-toolbox.vercel.app)

Checkboxes track **Kobayashi implementation status** in this repository (`[x]` = done or substantially shipped, `[ ]` = not done; nested bullets refine partial work). Update these when scope changes.

Source pages reviewed:

- `/mitigation`
- `/game-mechanics`
- `/simulator`
- `/combatlog`
- `/ship-comparison`

## High-priority features (core combat accuracy)

- [ ] **Separate raw combat pipeline from CSV combat log parser**
  - [x] First-class structured ingest: JSON (`parse_combat_log_json`, `IngestedCombatLog` in `src/combat/log_ingest.rs`) and game export TSV (`parse_fight_export` in `src/combat/export_csv.rs`); see `docs/combat_log_format.md` and `tests/log_ingest_tests.rs`.
  - [ ] Preserve subround-level events and **full** intermediate stat state snapshots for mechanics reverse-engineering (beyond what ingest/export carry today).
  - [ ] Encode canonical round/sub-round ordering from observed client event identifiers:
    - `START_ROUND` → `HULL_REPAIR_START/END` (once per round, before first sub-round)
    - Per sub-round: officer/ship abilities apply, then forbidden tech + chaos tech buffs, then attacks for that sub-round weapon index
    - `END_ROUND`: burning tick (1% of target max hull per round while burning active), temporary-effect cleanup, then next round (up to 100 rounds)
  - [ ] Persist full ordered event stream (including repeated per-ship applications) even when the UI collapses duplicate ability/FT log lines.

- [x] **Monte Carlo combat simulator mode**
  - [x] Simulation runner over combat inputs with iteration count (`src/optimizer/monte_carlo/`, `POST /api/simulate`, `POST /api/optimize`, CLI `simulate`).
  - [x] Distributions / uncertainty: win-rate and related **95% CIs** on optimize/sim API responses (`win_rate_95_ci`, hull CIs in `src/server/api.rs` / `api/execution.rs`); crew compare histograms (`src/optimizer/monte_carlo/compare_crews.rs`, `POST /api/compare/crews`).

## Medium-priority features (mechanics completeness)

- [ ] **Implement ability-boost interaction rules**
  - [x] Schema hooks: `boostable` / `boosted` on `Ability` / `ActiveAbilityEffect`, seat gating (`can_activate_in_seat` in `src/combat/abilities.rs`).
  - [ ] Boost logic for effects that modify maneuver/ability potency; respect “boostable at combat begin or subround end” timing; keep per-effect boostability so unsupported effects are not amplified (still open).

- [ ] **Model temporary-combat-only effects**
  - Add transient state for combat-only gains removed after battle (e.g. temporary hull restoration behavior like Leslie).
  - Ensure post-combat state rollback for those effects.

- [ ] **Add duplicate-officer bug compatibility toggle**
  - Note: the engine **deduplicates** duplicate officer ids today (`apply_duplicate_officer_policy` in `src/combat/abilities.rs`); this item is an optional **parity** mode to reproduce legacy bug behavior, not current default.

- [ ] **Improve stat nomenclature and baseline definitions**
  - Standardize HHP/SHP and component stat naming.
  - Define “base” values consistently (component bonuses + tier-max level assumptions, excluding research unless toggled on).

## Validation and tooling features

- [x] **Mitigation scenario analyzer** (partial)
  - [x] CLI + library: `kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]` and `src/combat/mitigation_sensitivity.rs` (sensitivity rows vs baseline stats).
  - [ ] Dedicated HTTP tool endpoint mirroring toolbox-style “what-if” mitigation tables (if desired for UI parity).

- [x] **Mechanics regression corpus from raw logs** (partial)
  - [x] Fixture suite under `tests/fixtures/recorded_fights/` and calibration tests using recorded fights.
  - [ ] Broader corpus from representative **raw** client/toolbox logs with snapshot tests for mitigation%, per-round damage, and effect-stack outcomes as described here.

- [ ] **Engine explainability output (mitigation decomposition)**
  - Extend optional debug trace for mitigation with per-step calculations beyond today’s scalar `mitigation_calc` events (`mitigation` + `multiplier` only in `src/combat/engine.rs`):
    - defense/piercing ratios per component
    - each `f(x)` value
    - weighted component contributions (`cA`, `cS`, `cD`)
    - final multiplicative combination

## Future / optional (sub-round and weapons)

- [x] **Per-weapon pierce/crit/proc from upstream data** (shipped; see caveats below)
  - [x] Engine: optional per-weapon fields on `WeaponStats` with combatant-level fallback (`src/combat/types.rs`).
  - [x] Ship normalizers: per-component `penetration` / `modulation` / `accuracy`, crit, and proc when present — `src/bin/normalize_data_stfc_space.rs`; legacy STFCcommunity ship path `raw_to_ship_record` in `src/bin/normalize_stfc_data.rs` fills `WeaponRecord` from each `weapons_info` row (armor/shield pierce, accuracy, crit).
  - [x] `WeaponRecord` carries optional `armor_piercing` / `shield_piercing` / `accuracy` for importer round-trip (`src/data/ship.rs`).
  - [x] Scenario: `ship_weapons_with_resolved_pierce_through` merges row-level piercing with profile/static accuracy bonuses and sets each weapon’s **damage-through** pierce via `pierce_damage_through_bonus` vs the resolved hostile (`src/optimizer/monte_carlo/scenario.rs`).
  - [x] Hostiles: weapon components sorted by upstream `order`; parse `penetration` / `modulation` / `crit_modifier`; counter-attack weapons use `weapons_for_counter_attack` (no raw ap+sp stuffed into `WeaponStats.pierce`) — `src/data/hostile.rs`.
  - [ ] **Still open:** mitigation **multiplier** for the fight remains from **tier-averaged** `ShipRecord` attacker stats; only the additive damage-through pierce term is per-weapon. Full per–sub-round mitigation from per-weapon piercing would need engine work.
  - [ ] **Still open:** hostile counter-fire still uses placeholder zero player `DefenderStats`, so per-weapon counter pierce may not diverge until that path is richer.
  - Re-run `cargo run --bin normalize_data_stfc_space` after refreshing upstream ship JSON so `data/ships_extended/` picks up new per-weapon fields (older files deserialize with absent optional fields and keep previous fallbacks).

## Suggested implementation order

Track the same ordering as above; status mirrors sections above.

- [ ] 1. Raw-log parser and simulator integration (client-fidelity stream and snapshots) — **in progress** (structured ingest exists; full fidelity still open).
- [x] 2. Monte Carlo snapshot mode + damage/survival distributions — **shipped** (core MC + CIs + compare histograms).
- [ ] 3. Ability boost rules + temporary combat-only state
- [ ] 4. Compatibility toggles + regression suite — **partial** (fixtures exist; duplicate-officer toggle and full corpus still open).
- [x] 5. Mitigation analyzer endpoint + trace decomposition for mitigation — **partial** (CLI/library sensitivity done; HTTP endpoint and full trace decomposition still open).
- [x] 6. Per-weapon upstream fields + scenario/hostile wiring — **shipped** (normalizers + `ship_weapons_with_resolved_pierce_through` + hostile weapon ordering/parsing; see **Future / optional** above for mitigation/counter-fire caveats).
