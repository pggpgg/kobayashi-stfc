# Remaining GitHub issue validation — 2026-10-24

This report continues [the first validation batch](GITHUB_ISSUE_VALIDATION_2026-10-24.md).
The combined scope is issues #300–303 and #315–375: 65 reports. Human-assisted
fights are paused. No new combat observations were requested or fitted.

Reproductions use synthetic fixtures or frozen observations. A failed compilation,
an empty test filter, and an intermediate test setup failure do not count as
reproducing an issue. The initial working tree was already modified; those changes
have been retained. The publication snapshot and final integrated results are
recorded below when verification is complete.

## Issue outcomes

The first batch covers #316, #318, #328, #330, #333, #340, #341, #352, #363,
#366, #374 and #375. This table covers the remaining reports. A local fix does
not imply that a release containing it has shipped.

| Issues | Verified behavior and resulting change |
| --- | --- |
| #300 | Generated optimizer wire types replace competing frontend interfaces. Rust schema export checks request, response, progress and job-result shapes. Both optimizer clients forward the generated request options. Regeneration is repeatable. |
| #301, #302 | Optimizer settings use coherent typed groups and focused sections. Searchable officer-group controls replace raw JSON; valid saved settings retain their wire representation and invalid groups block submission visibly. |
| #303 | Preparation and persistence are extracted from optimizer execution without changing statement order, seeds, candidate selection, cancellation or progress events. |
| #315 | Canonical triggers no longer fall back to unconditional passive effects. Resolver-level checks also distinguish taking hull damage from entering the Hull Breach state, and enemy shield depletion from own shield depletion. Unsupported event families are reported and omitted rather than assigned a different timing. |
| #317 | An owner-faction-only research row follows one compilation path. A separate independent condition still remains conditional. |
| #319 | Percentage-flagged research requires an explicit verified unit. A monotonic rank ramp no longer changes units at 1. Unsupported unit or condition rows are quarantined with reasons; see the coverage limits below. |
| #320 | Stateful counter-fire uses scalar per-hit ordering so traced replay and an untraced run retain the same RNG and state-update order. |
| #321 | Conditional maximum hull and shield bonuses apply relative to the previous active factor, avoiding repeated round-by-round compounding. |
| #322, #323, #324 | Officer-stat percentages add within their pool; conditional non-officer passive effects reach runtime evaluation; additive critical damage uses its flat-bonus channel without changing native critical-damage units. |
| #325 | Early-stop and abandonment checkpoints are evaluated at the requested trial count inside a work batch. |
| #326, #355 | Enumeration, benchmark recall and final-list deduplication use identity that ignores order within a role. Numerical simulation caches retain ordered identity because supported stochastic effects can depend on officer evaluation order. |
| #327 | An omitted candidate limit has a 10,000-crew safety ceiling. Estimates use the effective count, and capped counting stops traversing combinations when it reaches the ceiling. Explicit supported limits remain available. |
| #329 | Learned officer scores no longer limit sampling to one result per pool cell. Generation and count estimates use the same legal pool. |
| #332 | Empty, unresolved, conflicting or otherwise critically invalid roster imports preserve the previous roster. Native officer IDs resolve through the registry. Successful saves use atomic replacement. |
| #334, #345, #350 | Profile-index, restore, ingress and background writes share a process-local transaction gate. Checked reloads and unique atomic replacements preserve malformed existing files. Restore updates the index cache and rejects old sync tokens after replacement. |
| #335 | All sensitivity entry points validate the requested ship and hostile before admitting work or creating a job. |
| #336 | Native fractional ship accuracy bonuses scale the ship's raw accuracy rating instead of adding a tiny flat rating. |
| #338 | Partial heuristic seeds can fill the ship's unlocked below-decks seats before legality checks. |
| #339, #365 | Ship selection, tier and level remain consistent with the actual request; changing ships resets incompatible tier and level state on both PvP sides. |
| #342, #343 | Leaving a loop or sensitivity view cancels its work. A stale response cannot restart the departed loop or retain the CPU slot indefinitely. |
| #344 | Defender below-decks capacity comes from the defender's own ship, tier and level in the UI, simulation and optimization paths. |
| #346 | Startup no longer deletes profiles merely because their IDs resemble scenario-test IDs. Tests own their fixture cleanup. |
| #347 | Research subset and inspection modes cannot silently overwrite the full production catalog. |
| #348 | Empty or truncated upstream building summaries fail before deleting tracked building data. |
| #349 | A malformed existing opaque allowlist is preserved with an error; valid handwritten reasons survive generated additions. The malformed-file case reproduces even though the valid-file merge was already safe. |
| #351 | Defender regeneration cannot resurrect a combatant already killed during the round. |
| #353 | Unrecognized or malformed conditions remain false with diagnostics instead of disappearing and widening the effect. |
| #354 | Genetic benchmark budgets include the final Monte Carlo confirmation pass; infeasible budgets fail explicitly. Adaptive scouting accounts for actual coarse and rerun work within the nominal request ceiling. |
| #356 | Refinement does not append a second copy of an existing finalist. |
| #357 | A failed detail refresh can continue from an ID-matched, valid cached file with a stale-data warning. Missing or corrupt cache entries still fail. |
| #358 | The legacy normalizer preserves modern hostile registry and index metadata; malformed details cannot become synthetic zero-hull hostiles. |
| #359 | One malformed ship-ability catalog entry does not discard valid sibling entries. |
| #360 | Benchmark gates fail when a configured metric or required random-control lane is missing. |
| #361 | Concurrent uploads use unique, exclusively created temporary files with cleanup on failure. |
| #362 | Name-and-level lookup rejects materially different hostile variants. Explicit IDs still resolve; variants with identical combat data remain interchangeable. |
| #364 | A failed profile load is visible and prevents Save from overwriting the server profile with uninitialized values. |
| #367 | Windows drive-relative paths cannot escape the SPA asset directory. A harmless temporary sentinel reproduced the former file read. |
| #368 | The HTTP boundary checks Host, Origin and JSON MIME type before mutating API handlers. Native ingress and supported non-JSON imports retain their explicit paths. |
| #369 | Configured API-key protection covers remote profile-list and profile-export reads that contain sync tokens. Reverse proxies must disable trusted-loopback bypass and authenticate clients. |
| #370 | Restore streams bounded entries with entry-count, per-entry, total-expansion and index limits, cleans staging on errors, and retains the previous tree as a backup. Export rejects symbolic links and Windows reparse points. |
| #371, #372 | Actions are pinned to immutable revisions; the compiler is pinned. Build and refresh producers have read-only tokens, checkout does not persist credentials, and separate publisher jobs receive only their required write permissions. |
| #373 | Release verification checks main ancestry and runs the dependency audit. Published assets receive build-provenance attestations. Documentation distinguishes download integrity from authenticity and identifies the historical unsigned release accurately. |

## Reports that did not reproduce

- [#331](https://github.com/pggpgg/kobayashi-stfc/issues/331): missing and empty
  research catalogs already return a guarded error without panicking. The
  regression runs with a resolved scenario so it actually reaches that branch.
- [#337](https://github.com/pggpgg/kobayashi-stfc/issues/337): profile critical-damage
  floors and per-weapon inheritance were already preserved. Both paths passed.

Both issues received comments explaining the checks and were closed. Neither is
counted as a new production fix. Other issues remain open pending publication;
draft comments for reproduced issues have not been posted automatically.

## Evidence and coverage limits

Private command logs and exact restoration receipts are retained under
`local-analysis/github-remaining-issues-20261024/`, outside version control.
Tests and sanitized regression fixtures are public. For #315, #323, #324 and
#353, some original behavior was rechecked by temporarily restoring the original
implementation hunk and then verifying exact restoration of the fixed file.
These are controlled original-implementation rechecks, not claimed clean initial
red runs. The later #315 resolver-level timing regression was run before its
additional mapping correction.

The compiled legacy research catalog changes from **802 to 276 projects**, with no
adjacent tenfold rank drops. The final strict-unit pass removes 24 projects whose
percentage units are unverified; it reports `percentage_unit_unverified` and
records the affected BID/RID pairs. Earlier condition checks also exclude rows
whose compound gates are unsupported. This improves correctness at the cost of
coverage: omitted player buffs can still understate a ship's stats. Another 67
unitless mappings whose upstream percentage flag is false remain a separate
legacy path and were not silently reinterpreted by this fix. The separate native
setup manifest supports 226 of 1,177 selected buff definitions and quarantines
951, mostly for unsupported conditions. Project counts and buff-definition
counts are different measures and cannot be added together. These are explicit
coverage limits, not evidence that excluded buffs are absent in the game.

Research source checks compare build-273 static definitions with a traced
build-272 client consumer and exact matching upstream rank arrays. That is
client-side evidence, not proof of server combat behavior. Unsupported canonical
event families likewise remain modeling gaps, with explicit drop diagnostics.

The profile-store gate serializes writers in one server process. It does not
coordinate multiple independent servers sharing a profile directory. Family
savepoints remain separate transactions. Long-running optimizations can finish
after a restore; numerical history still requires an exact fingerprint match,
while generation-specific learned-prior isolation remains a future improvement.

Source review also found two regressions during integration: Seska's renamed
critical-damage channel had lost its duration, and Kras's rank table had lost the
negative source sign. Both have behavioral regressions and fixes. Kras retains
negative LCARS values and normalizes them to the positive opponent-penalty
magnitude required by the typed runtime boundary. The #322 synthetic fixture was
also corrected to activate a real bridge ability: the valid pre-fix test observed
`0.08 × 0.50 = 0.04`, and the fixed percentage pool produces `0.58`.

Combat behavior version **12** invalidates earlier persisted optimizer metrics.
Tom Paris's Defense source and stacking remain unresolved; the frozen combat
observations and rejected candidate formulas are unchanged.

## Integrated validation and publication

- Full Rust run: **1,774 passed, 1 failed, 1 ignored**, including 135 compiled
  targets, a pure doctest and the separately executed Windows-junction check.
  Tests run from a new runtime with synthetic profiles, one test thread and
  four Rayon threads. The live player index remains byte-identical.
- The sole failure is the existing `gorn_evisc_log_vs_sim_calibration`. Its
  numerical output is unchanged across the integration runs; its real-log bands
  were not relaxed. Full CI is not claimed green.
- All-target Clippy with `-D warnings`, full formatting checks and the locked
  release build pass. The release executable validates the officer catalog,
  retaining warnings for unsupported or partial mechanics.
- Frontend: **331 tests pass**, TypeScript and production build pass. After the
  dependency update, npm audit reports **zero findings**. The lock resolves Vite
  8.3.5 and Vitest/coverage/mocker 4.1.12.
- Maintenance scripts: **96 tests pass**, also checked in the publication tree.
  Optimizer OpenAPI regeneration is idempotent; response, request and schema
  parity checks pass.
- A refreshed RustSec audit reports **zero vulnerabilities** under the existing
  `cargo audit` policy. It retains six informational warnings: one unmaintained
  dependency and five unsoundness advisories. A policy that denies every warning
  would still fail; no warning suppression was added.

The independently reviewable workflow and dependency changes are in
[draft PR #378](https://github.com/pggpgg/kobayashi-stfc/pull/378). Its exact
branch passes actionlint, three workflow regressions, frontend Biome,
TypeScript, **286 frontend tests**, production build and npm audit. Those test
counts differ because the application changes are a separate draft. GitHub CI
and Benchmark regression both pass at its `d0ec7a9d` head.

Application publication starts from current `main` at `0e441110`, preserving its
newer upstream data and earlier security fixes. All 340 Rust/MJS source files
match the validated development tree after newline normalization; all 24 embedded
assets also match. Private profiles, client binaries, raw analysis caches and
local command logs are excluded. Generated frontend `dist` files are excluded
because their hashed assets are not tracked.

One redacted TSV received line-ending and trailing-empty-row normalization for
publication. Its provenance retains the previous and current sanitized hashes;
every nonempty TSV row and every numeric cell was verified unchanged.

The merged publication data was also tested from a separate synthetic-profile
runtime: **1,769 passed, 1 known Gorn failure, 1 ignored**, across 134 executable
targets. The three workflow tests belong to PR #378, and the two supplemental
checks are reported above separately. This clean-checkout run caught a new test
that depended on ignored generated YAML; it now uses the canonical production
loader and the full 1,134-test library passes without that YAML file.

The original development branch and Git index remain unchanged. The release
binary is built; the existing capture server has not been redeployed by this
publication work. Human-assisted fights remain paused.
