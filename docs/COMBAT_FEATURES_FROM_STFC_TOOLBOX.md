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

- [ ] **Per-weapon pierce/crit/proc from upstream data**
  - [x] Engine already supports optional per-weapon overrides on `WeaponStats` (fallback to combatant-level when unset).
  - [ ] When upstream or STFC data differs by weapon, extend normalizers / importers so those fields are populated consistently.

## Suggested implementation order

Track the same ordering as above; status mirrors sections above.

- [ ] 1. Raw-log parser and simulator integration (client-fidelity stream and snapshots) — **in progress** (structured ingest exists; full fidelity still open).
- [x] 2. Monte Carlo snapshot mode + damage/survival distributions — **shipped** (core MC + CIs + compare histograms).
- [ ] 3. Ability boost rules + temporary combat-only state
- [ ] 4. Compatibility toggles + regression suite — **partial** (fixtures exist; duplicate-officer toggle and full corpus still open).
- [x] 5. Mitigation analyzer endpoint + trace decomposition for mitigation — **partial** (CLI/library sensitivity done; HTTP endpoint and full trace decomposition still open).
