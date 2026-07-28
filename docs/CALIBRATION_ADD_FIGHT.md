# Add a recorded fight (about 10 minutes)

This document gives the short procedure to move a TSV export from the game into the
calibration suite. Each fight in the suite is bound to one snapshot.

**Caution: Do not add fights from different snapshots.** Wait for the
[snapshot freeze](RECORDED_FIGHT_SUITE_GUIDE.md).

## 1. Export the fight from the game

Do these steps during the freeze window, with the frozen profile loaded in Kobayashi:

1. Fight in the game.
2. Export the TSV file with the usual flow of the game.
3. Save the file as `fight samples/<ship>_vs_<hostile>_<level>_<outcome>.csv`. Use lowercase
   letters and underscores.

For the layout of the columns, refer to [combat_log_format.md](combat_log_format.md).

## 2. Test the parser

```bash
cargo test --test recorded_fight_calibration_tests fight_export
cargo test --test log_ingest_tests
```

Correct each parse error before you add a row to the manifest.

## 3. Add a row to the manifest

Edit
[`tests/fixtures/recorded_fights/recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json):

- Set the top-level `profile_id` to the frozen profile, for example `higgsbozo`.
- Add a fight object. It must have `ship_id`, the tier, the level, the crew, `hostile_id` or
  the display name with the level, `primary_axes`, `holdout`, and `bands`. The `bands` field
  holds the outcome metrics from the export.
- You can also add `officer_anchor` (`kirk`, `marla`, or `kras`). It gives an anchor for the
  magnitude of the formula.

**Caution: Do not turn on the CI score for a fight too early.** The fight must come from
that profile. It must also come from the freeze window.

## 4. Isolate one mechanic with a synthetic fight (no freeze necessary)

You can test one mechanic for a regression without a full fight log. Copy a `drift_*.json`
template from [combat_log_format.md](combat_log_format.md). Then run these commands:

```bash
cargo test --test calibration_drift_tests
cargo xtask calibration-scoreboard
```

## 5. Generate the scoreboard again

```bash
cargo xtask calibration-scoreboard --write
```

The command writes [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md). Commit that file.

## Related documents

- [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) — How to select the fights.
  The suite has about 40 fights.
- [NOT_ROADMAP.md](NOT_ROADMAP.md) — The protocol for snapshot calibration. The project does
  not plan this work.
