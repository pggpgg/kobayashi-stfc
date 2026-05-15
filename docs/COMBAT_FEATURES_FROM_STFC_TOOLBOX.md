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
  - [x] Versioned ingest payload (`schema_version`, default `1`) with optional per-event metadata: monotonic `sequence`, toolbox-facing `client_kind` / `client_payload`, optional flat `stats_snapshot` maps (`serde_json::Value`) — see `docs/combat_log_format.md`.
  - [x] Canonical timeline validation (`validate_canonical_timeline` in `src/combat/log_validate.rs`): strict errors when `schema_version >= 2` or when events carry `sequence`; warn-only mode preserves permissive parsing for legacy `schema_version == 1` logs without full sequencing.
  - [x] Trace-level regression helpers: `compare_ingested_trace_to_simulator` (subsequence match on skeleton fields + optional numeric `values` keys); rich fixture `tests/fixtures/recorded_fights/rich_engine_aligned_log.json`; invalid-order fixtures `invalid_timeline_v2.json`, `invalid_sequence_v2.json`; calibration-style test vs `simulate_combat` + `TraceMode::Events` in `tests/log_ingest_tests.rs`.
  - [x] CLI: `kobayashi validate-log <path.json>` (parse + timeline validation).
  - [ ] Preserve subround-level events and **full** intermediate stat state snapshots for mechanics reverse-engineering (beyond optional `stats_snapshot` maps and Kobayashi-export traces today).
  - [ ] Encode **observed client/toolbox** round/sub-round identifiers into this ingest IR end-to-end (validator rules mirror intended combat ordering below; mapping from real client streams is still pending samples).
    - `START_ROUND` → `HULL_REPAIR_START/END` (once per round, before first sub-round)
    - Per sub-round: officer/ship abilities apply, then forbidden tech + chaos tech buffs, then attacks for that sub-round weapon index
    - `END_ROUND`: burning tick (1% of target max hull per round while burning active), temporary-effect cleanup, then next round (up to 100 rounds)
  - [ ] Persist full ordered event stream (including repeated per-ship applications) even when the UI collapses duplicate ability/FT log lines.

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

- [x] **Mechanics regression corpus from raw logs** (partial)
  - [x] Fixture suite under `tests/fixtures/recorded_fights/` and calibration tests using recorded fights.
  - [ ] Broader corpus from representative **raw** client/toolbox logs with snapshot tests for mitigation%, per-round damage, and effect-stack outcomes as described here.

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

- [ ] 1. Raw-log parser and simulator integration (client-fidelity stream and snapshots) — **partial**: versioned ingest + timeline validator + trace comparison + `validate-log` + sim-vs-ingest subsequence test; **still open** real toolbox/client JSON corpus and full snapshot fidelity.
- [ ] 2. Ability boost rules + temporary combat-only state
- [ ] 3. Compatibility toggles + regression suite — **partial** (fixtures exist; duplicate-officer toggle and full corpus still open).
- [x] 4. Per-weapon upstream fields + scenario/hostile wiring — **shipped** (normalizers + `ship_weapons_with_resolved_pierce_through` + hostile weapon ordering/parsing; see **Future / optional** above for mitigation/counter-fire caveats).
