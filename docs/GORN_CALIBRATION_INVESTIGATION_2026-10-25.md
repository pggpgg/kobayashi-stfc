# Gorn calibration investigation — 2026-10-25

The failing check compared a target HP-pool/kill-summary reference with two
legacy simulator counters that include damage beyond available HP. This was a
measurement mismatch. The combat equations and all numerical fixture bands,
ratio bands, Monte Carlo floors and trace requirements remain unchanged.

The June export reports Acrocanth hull 178,272,182 and shields 132,956,074.
Their sum is **311,228,256**, within 8 HP of the exported kill summary
**311,228,248**. The export's attack columns separately report 99,186,368 normal
damage and 12,258,506,752 isolytic damage; they are not kill-summary measurements.

## Reproduction and accounting

The frozen demo-profile replay still produces the same weapon damage and outcome:

| Quantity | HP / damage |
| --- | ---: |
| Legacy `total_damage` | 1,451,399,671.375463 |
| Legacy `total_isolytic_damage` | 6,592,217,986.877317 |
| Actual shield HP removed | 132,956,074 |
| Actual hull HP removed | 178,272,182 |
| Hull overkill | 1,140,171,415.375463 |
| Discarded shield assignment | 5,140,818,315.501854 |
| Applied weapon HP damage | 311,228,256 |

Isolytic Vulnerability correctly routes only the isolytic leg into this target.
With 80% shield mitigation, its uncapped hull assignment is 20% of the isolytic
pool; the legacy total then adds the capped shield assignment. The legacy
isolytic counter accumulates the entire pool before routing. Neither counter
measures the target HP removed.

Outbound trace events now expose applied weapon/isolytic HP damage, hull overkill
and discarded shield damage. Independent synthetic checks cover lethal overkill,
discarded shield damage without a kill, a mixed normal/isolytic pool and apex
reduction. They verify conservation of the resolved damage pool, exact known HP
loss, and unchanged traced/untraced legacy counters. The new trace-contract test
failed before the fields were added and passes afterward.

The golden engine snapshot gains only the four diagnostic keys (192 values).
Removing those keys reproduces the previous parsed snapshot exactly: every
existing outcome, counter and trace value is unchanged. No golden tolerance was
changed.

The Gorn check projects a complete trace's applied weapon HP totals into its
existing health-pool bands. Both applied totals are 311,228,256 and pass the
unchanged 300–360 million bands. Existing ratios are 1.0000 against kill summary,
0.0254 against the raw isolytic column and 3.138 against the normal attack column.
The parser, activation/vulnerability checks and existing 2,000-seed Monte Carlo
floors also pass. Raw legacy counters remain printed separately for audit.

## What this establishes

This validates target identity, weapon HP accounting, kill/round behavior under
the fixture's bounds and the expected ability routing. It does **not** establish
the historical player's raw weapon damage, buff compilation or incoming damage.
The June fight used a different player setup from today's bundled demo profile;
the recorded hit was critical while this deterministic replay is not. The raw
12.259-billion isolytic observation is therefore still not calibrated against a
matching snapshot. The earlier October run's different raw magnitude remains an
input/model-history question, not a reason to tune a multiplier.

The existing Monte Carlo `avg_hull_remaining` used here is a legacy overkill-derived
score, not an independently measured remaining-HP fraction. This change neither
changes that optimizer behavior nor treats its floor as validation of the June
fight's 93.21% surviving hull. That metric deserves a separate correction with
its callers and cache invalidation considered together.

No recorded fight, numerical tolerance or game setup was changed. Human-assisted
fights remain paused. The production capture server was not redeployed.

## Integrated verification

The isolated public-data Rust run passes all 134 runnable test artifacts:
**1,772 passed, 0 failed, 1 ignored**, including the full 1,135-test library.
Formatting and all-target Clippy with `-D warnings` pass. The real saved player
index is unchanged. Workflow security tests belong to separate PR #378.

The CI gates previously skipped after the failing calibration also exposed stale
generated outputs. Regeneration adds the existing native critical-unit marker
to 116 ship files; all stat values and the ship index are unchanged. Realta's
integer-valued floats receive normalizer formatting only. A second generation is
byte-identical. The registry records the regeneration date; its source data
version is unchanged. The calibration scoreboard is refreshed from passing drift
fixtures with every band and pass/fail status retained; composite mean sigma is
0.2960. No reference bands were regenerated or widened.
