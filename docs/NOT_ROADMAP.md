# Not on the roadmap

This file lists **explicit non-goals**: ideas or enhancements we are **not** treating as planned work in [ROADMAP.md](ROADMAP.md). They may be technically imaginable or mentioned in code/docs, but we are **deferring or declining** them as roadmap items unless that stance changes.

---

## Snapshot-bound calibration (simulation fidelity)

**Not planned:** growing a snapshot-bound recorded-fight corpus, the profile-snapshot completeness audit, or the full snapshot-calibration iterate loop. Tooling shipped in 2026-06 (composite-score harness, suite manifest, recorded runner, import faction resolution) remains in the repo for ad-hoc use; we are not scheduling the maintainer freeze window or corpus growth as roadmap work.

### Calibration scoreboard + recorded-fight corpus growth

Only 20 `drift_*.json` fixtures (mostly synthetic) plus a handful of recorded fights exist. **Shipped (2026-06-14):** composite-score harness ([`src/calibration/scoreboard.rs`](../src/calibration/scoreboard.rs)), `cargo xtask calibration-scoreboard`, committed [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md), CI artifact + stale-doc gate, suite manifest ([`recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json)), profile-bound recorded runner ([`src/calibration/recorded.rs`](../src/calibration/recorded.rs)), [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md). **Declined as roadmap work:** populate ~40 snapshot-bound fights; fill three `_TBD_` in-game damage anchors in [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md). Reference material if the stance changes: [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md).

### Profile-snapshot completeness audit

Verify every sim input — including artifacts, syndicate, and exocomp-class bonuses — is capturable in one profile export before a freeze window. Prep checklist: [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md).

### Snapshot-calibration protocol

**Why corpus growth keeps getting parked.** The live game state evolves continuously — research, buildings, artifacts/forbidden tech, officer and ship levels all improve steadily — while recorded fights are frozen snapshots of whatever the state was when each one was captured. Calibrating against a mixed-vintage corpus risks **overfitting the engine to assumptions that are no longer true** (or were never true simultaneously): a "fix" that improves agreement with stale fights can encode a wrong model of the current game. Calibration data is only trustworthy when the profile the sim consumes matches the exact game state that produced the fights.

**The protocol** — one sitting, in a deliberately chosen window (the freeze blocks STFC event participation, which is why it can't happen casually):

1. **Snapshot** — capture the full game state into a Kobayashi profile: research, buildings, artifacts/forbidden tech, officer levels/tiers, ship tiers/levels — everything the engine consumes as input.
2. **Freeze** — no tiering, leveling, research, building, or any other state-changing progression for the duration of the window.
3. **Record** — run a varied, curated set of in-game fights (different hostiles, levels, crews, ship classes) and export each one. That set is the **fight test suite for that snapshot**, permanently bound to the frozen profile.
4. **Score** — a composite accuracy score over the suite (per-fight deviation → aggregate) measuring how faithfully the engine replicates the frozen reality.
5. **Iterate** — the composite score becomes the objective function for an auto-research self-improvement loop: engine and model changes are evaluated against the frozen suite, accepted when the composite improves without regressing individual fights.

**Prep already shipped:** defender faction for TSV imports; composite-score harness (developable against existing synthetic drift fixtures).

---

## Buildings / scenarios

- **Economy/meta building buffs** — Opaque `buff_*` rows for generators, storage, repair, unlocks, and similar non-combat effects are tracked in `data/buildings/opaque_buff_allowlist.json` and excluded from the actionable gap report. They never merge into the combat profile by design.
- **Out-of-simulator-scope building buffs** — Alliance starbase assault mechanics, defense platforms, armada slot caps, solo armada ship limits, defense platform damage, outpost fleet size, and broken-ship-parts drop rates are also allowlisted as intentionally unmapped for the default ship-vs-hostile optimizer.
- **Scoped combat building buffs** — Actionable opaque backlog is cleared (see `docs/building_gaps.md`). Allowlisted rows include armada-fleet participant weapon damage and Academy crit mitigation (`buff_341625291` / Remote Campus — no `crit_mitigation` engine stat). Aggregation hyperthermic stabilizer (`buff_1422729787` / Recon Locus) is modeled vs Aggregation hostiles. Station-defense mode remains future work.
- **Conditions for station defense** — When station/starbase defense is in scope: populate `BonusEntry.conditions` (e.g. `defense_platform_only`, `ship_combat_only`) from import or mapping; support `BuildingMode::StationDefense` in the optimizer.

---

## Empty or incomplete crew (game rule vs simulator)

**Game rule:** In STFC you cannot start a fight with an empty crew. The client requires a legal roster — at minimum a **captain** and **bridge officers** in filled slots before combat begins. Below-decks slots may be unfilled only when the ship has not unlocked them yet; you still cannot launch with zero officers assigned.

**Simulator behavior today:** Kobayashi accepts empty or partial crews in simulate/optimize paths (e.g. captain-only with no bridge, or an empty captain string). Empty bridge/below-decks slots are skipped in LCARS resolution ([`crew_resolution.rs`](../src/optimizer/monte_carlo/crew_resolution.rs)). This is intentional for **analysis** — e.g. probing whether a hostile is trivial with ship stats + profile alone ([`tests/gorn_evisc_solo_crew_trivial.rs`](../tests/gorn_evisc_solo_crew_trivial.rs)).

**Not planned:** mirroring the in-game UI gate by rejecting empty or incomplete crews at API/CLI ingress. Maintainers may use partial crews as a deliberate test harness; production optimize assumes roster-legal full crews from the generator, not player-empty submissions.
