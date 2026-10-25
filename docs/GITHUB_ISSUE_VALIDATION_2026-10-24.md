# GitHub issue validation — 2026-10-24

This batch checks the current `codex/combat-log-fidelity` working tree against
the open issues in `pggpgg/kobayashi-stfc`. The initial checkout was already
modified and two commits behind its tracking branch. Existing work was
preserved. No branch update, commit, push, issue comment, or issue closure was
performed. Human-assisted game calibration is on hold.

The issue bodies and empty comment threads were read from GitHub. Reports from
the automated audit are treated as hypotheses until the current code or a
reproduction supports them. Private source snapshots and test logs are under
the ignored `local-analysis/github-issue-validation-20261024/` directory.

## Results

| Issue | Finding and change | Verification |
| --- | --- | --- |
| [#316](https://github.com/pggpgg/kobayashi-stfc/issues/316) | Genetic finalists now use the same resolved scenario as evolution, retaining ship tier, level, profile and defender inputs. | The old final pass reported a 0.0 win rate where the requested tier-10/level-50 registry scenario reported 1.0. The regression compares final rows with that registry scenario. |
| [#318](https://github.com/pggpgg/kobayashi-stfc/issues/318) | Simulation and both optimization clients retain all eight defender fields. | Real request builders feed the real API functions; tests inspect serialized fetch bodies. The old client dropped the fields. This verifies transport, not live-game PvP combat accuracy. |
| [#328](https://github.com/pggpgg/kobayashi-stfc/issues/328) | Genetic crossover tracks inherited below-decks officers before refill. Repair also removes duplicate seats. | A legal parent pair reproduced a duplicated `D2` child at seed 0; a separate repair regression also failed before the fix. The crossover test checks 128 seeds. |
| [#330](https://github.com/pggpgg/kobayashi-stfc/issues/330), [#341](https://github.com/pggpgg/kobayashi-stfc/issues/341) | Explicit `exhaustive` reaches the server. Unknown or empty explicit strategies are rejected instead of selecting exhaustive. Omission retains auto-routing. | Request serialization reproduced the dropped strategy; backend validation reproduced acceptance of `auto`, `teired`, and empty strings. These reports overlap. |
| [#333](https://github.com/pggpgg/kobayashi-stfc/issues/333) | Result selections belong to one recommendation array and reset when a new result set arrives. | Rendered UI tests reproduced the shorter-list crash, same-length wrong selection, and return to an older array. |
| [#340](https://github.com/pggpgg/kobayashi-stfc/issues/340) | The async optimizer client preserves local refinement, the history cache key, and history anchors. | Real workspace and PvP request paths are checked at fetch serialization. |
| [#352](https://github.com/pggpgg/kobayashi-stfc/issues/352) | One malformed or unreadable optional upstream stat file emits a warning and leaves only that officer's optional stat rows unavailable. Other officers and canonical abilities still load. | A malformed temporary Tom file previously aborted the whole model. The regression preserves valid Jadzia stats and both officers' abilities, without inventing Tom stats. |
| [#363](https://github.com/pggpgg/kobayashi-stfc/issues/363) | A comparison response can update only the request, selection, scenario and result set that created it. | Deferred real fetch tests reproduced stale successes, failures, and overlapping requests before the fix. |
| [#366](https://github.com/pggpgg/kobayashi-stfc/issues/366) | The data fingerprint includes building JSON records, the building index, and starbase-module translations. | Source inspection confirmed the omission. The regression mutates each temporary input and checks the full data-segment assembly. A clean pre-fix failing test was not captured for this issue. |
| [#374](https://github.com/pggpgg/kobayashi-stfc/issues/374) | Estimates count candidates without constructing the full crew vector, run on `spawn_blocking`, and share CPU admission. | The original handler returned 200 with a saturated semaphore. The regression now requires 503 and verifies permit release on success and validation error. Small bounded generation checks preserve count parity across all three below-decks modes. No deliberate out-of-memory test was run. |
| [#375](https://github.com/pggpgg/kobayashi-stfc/issues/375) | Seed names accept only 1–64 byte ASCII stems, requests have at most 32 entries, and repeated names are loaded once. The loader rejects path traversal and non-regular files. | A temporary sentinel outside the seed directory reproduced the read. Tests also cover unsafe advertised names, request bounds and duplicate generation. No private player files were used. |

[#331](https://github.com/pggpgg/kobayashi-stfc/issues/331) is already fixed in
the starting working tree: a resolved research-summary scenario is guarded by
catalog availability. A new regression confirms that missing and empty catalogs
return the existing error without a panic. It is validated here, not counted as
a new production fix.

## Boundaries

The catalog fingerprint is cached per process. Restart the simulator after
editing building catalogs or translations. This fixes #366's reported
edit-then-restart case; live catalog edits during a running process are not
covered by the cached fingerprint.

The new malformed-file handling preserves the ability catalog but cannot recover
unknown stat rows. Warnings identify omitted files. No guessed stats or fitted
combat constants were added.

Combat behavior version 11 invalidates optimizer metrics that may have been
scored against the wrong genetic scenario. It does not claim that Tom Paris's
effective Defense source or stacking is solved. Frozen combat observations and
their rejected predictions are unchanged.

The remaining GitHub issues were not all validated in this batch. No status on
GitHub was changed.

## Integrated checks

- Rust library: **1068 passed**.
- Full Rust suite: **1681 passed, 1 failed, 1 ignored**, across 127 test targets.
  The only failure is the previously recorded
  `gorn_evisc_log_vs_sim_calibration`; its calibration bands were not relaxed.
- Frontend: **299 passed**, across 39 files. TypeScript and the production
  frontend build pass. TypeScript also passes after regenerating the OpenAPI
  client types for the request limits, strategy enum and estimate 503 response.
- `cargo fmt --all -- --check` and all-target Clippy with `-D warnings` pass.
  Incremental-cache hard-link warnings fall back to file copies on this host.
- Changed frontend source files pass Biome. The full frontend Biome check finds
  93 formatter-only errors in untouched CRLF files; non-format checks pass.
  No blanket formatting of those files was applied. The rebuilt HTML entry
  artifact was normalized to LF, and `git diff --check` passes.
- The earlier dependency audit still has existing `zip` advisories. This batch
  changes no package versions. Full CI is not green.

Before-fix frontend reproductions failed nine tests. Backend red logs retain
the incorrect genetic win rate, duplicate crew seats, malformed officer failure,
unsafe seed read, invalid request acceptance, and estimate CPU-admission bypass.
The temporary scaffold compile failures are kept separately and are not counted
as reproductions.

## Local runtime

The release binary builds successfully. The local server now runs behavior
version 11 with executable SHA-256
`e0df5f0e1aca2b77184cb0b31ee458efca0d6c5383c53a140258eb593d813604`.
Health and HTTP smoke checks passed at `2026-10-24T10:17:39Z`: 290 officers
loaded, a capped estimate returned 17 candidates, and invalid strategy and
unsafe seed requests both returned HTTP 400 before execution. The CPU permit
was returned after each request.

The frozen battle-journal file remains unchanged across the server replacement,
with SHA-256
`4dee4ed472ec5c8345a2f1c355d22d32f36c05b44b94f6b6241258a8c6cb0130`.
The server retains four Rayon threads, one concurrent CPU job, loopback binding,
and Windows below-normal priority. The rebuilt frontend assets are available
from the same local server. No game or mod restart was required.
