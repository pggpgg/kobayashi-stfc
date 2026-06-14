# Add a recorded fight (≈10 minutes)

Quick workflow for promoting a game TSV export into the snapshot-bound calibration suite. **Do not add mixed-vintage fights** — wait for the [snapshot freeze](RECORDED_FIGHT_SUITE_GUIDE.md).

## 1. Export from the game

During the freeze window, with the frozen profile loaded in Kobayashi:

1. Fight in-game.
2. Export TSV via the normal game flow.
3. Save as `fight samples/<ship>_vs_<hostile>_<level>_<outcome>.csv` (lowercase, underscores).

See [combat_log_format.md](combat_log_format.md) for column layout.

## 2. Parser smoke test

```bash
cargo test --test recorded_fight_calibration_tests fight_export
cargo test --test log_ingest_tests
```

Fix parse errors before adding manifest rows.

## 3. Add a manifest row

Edit [`tests/fixtures/recorded_fights/recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json):

- Set top-level `profile_id` to the frozen profile (e.g. `higgsbozo`).
- Add a fight object with `ship_id`, tier/level, crew, `hostile_id` or display name + level, `primary_axes`, `holdout`, and `bands` (observed outcome metrics from the export).
- Optional: `officer_anchor` (`kirk`, `marla`, or `kras`) for formula magnitude anchors.

Do **not** enable CI scoring until the fight was recorded on that profile during the freeze.

## 4. Synthetic mechanic isolation (no freeze required)

For a single mechanic regression without a full fight log, copy a `drift_*.json` template from [combat_log_format.md](combat_log_format.md) and run:

```bash
cargo test --test calibration_drift_tests
cargo xtask calibration-scoreboard
```

## 5. Regenerate the scoreboard

```bash
cargo xtask calibration-scoreboard --write
```

Committed output: [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md).

## Related

- [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) — fight selection (~40 fights)
- [ROADMAP.md](ROADMAP.md) — snapshot-calibration protocol
