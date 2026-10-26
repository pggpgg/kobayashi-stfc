# Snapshot replay results, October 2026

These four runs replayed the frozen no-Tom and Tom controls against the current
public catalogs. They compare the simulator's **scoped incoming base-stat
mitigation** with independently recorded observation intervals. They do not
score complete fight accuracy. The game RNG state is unknown; seed-zero outcome
and duration fields below are diagnostic simulator outputs, not historical
outcome checks.

| Control | Observed scope and interval | Unrounded prediction | Comparison | Seed-zero diagnostic |
| --- | --- | ---: | --- | --- |
| Exchange baseline, no Tom | Pooled clean hits across weapons and rounds: `[0.16193309728365343, 0.16194246568951853]` | `0.16193819137435395` for both modeled weapon indexes | `within_observed_interval` (one pooled comparison) | Loss; 14 rounds |
| Exchange with Tom | Pooled clean hits across weapons and rounds: `[0.7119816444766566, 0.7120187293432847]` | `0.16271136903449057` for both modeled weapon indexes | `mismatch` (pooled) | Loss; 15 rounds |
| Jem’Hadar baseline, no Tom | Observed weapon component, round 1: `[0.16170428083163155, 0.16170522158252099]` | `0.16170472129512503` | `within_observed_interval` | Loss; 1 round |
| Jem’Hadar with Tom | Observed stronger weapon component, round 1: `[0.7059304997243507, 0.7059335298548556]` | `0.16199191092664578` for both indexed comparisons | `mismatch` for both comparisons | Loss; 1 round |

The two no-Tom controls fall inside their observed intervals. Both paired Tom
controls remain far below their observed intervals. This preserves the known
Tom discrepancy for review; it does not establish the final source or placement
rule for Tom’s defense effect. The Exchange comparisons use pooled observations
and do not constitute separate per-weapon validations.

The seed-zero losses and round counts are diagnostic outputs only. They do not
show that the in-game fights were losses, nor do they establish damage-roll,
end-health or full-fight accuracy. These runs validate only the named mitigation
measurements against their recorded scopes.

## Replay identity

All four final reports used base revision `884c4701a5d5`, the same instrumented
simulator binary (`5fe8c7ae7ce8ec0e72c53d5b5af6631ace019e168d468b92c29c271e994a5488`),
and the same current public-catalog fingerprint
(`a9043299a2e413f2101b0b325cd3bc5d8ee2e53f4e7bbf03a1a12c246a346a85`). The
replay module SHA-256 was
`a2ac48e14726de14f00228293709d0364d78370f84cad460663922721add2cc8`; the CLI
SHA-256 was
`187846304b5e332b1d7f8ad5ca4b799fbbcb3745d6d61a435705b953bc642861`.

| Control | Manifest SHA-256 |
| --- | --- |
| Exchange baseline, no Tom | `a696bc812c23464205cae3a544e21f6e0207128684a99929b8f45217e1a8a6cf` |
| Exchange with Tom | `b5f6f37df57142557548c83c6e7b8deff3d57a6118708ecd35c0f4bd04eacdfe` |
| Jem’Hadar baseline, no Tom | `5c236fa0cbb04c260befb09790dc47fa3d307e9a95b9a27fa9d9182834950005` |
| Jem’Hadar with Tom | `e105614577fe4241ed281e1fb4783c465ecac258caffc7c5d70681f9338f4360` |

The three reports with available v1 predecessors (Exchange baseline and both
Jem’Hadar controls) preserve every simulator outcome-summary field exactly,
including outcome, rounds, damage and ending health. Trace event totals and
per-index event counts are unchanged: 346 events (28/26), 34 (3/2), and 35
(3/2). The legacy six-decimal mitigation values also remain the same: `0.161938`,
`0.161705`, and `0.161992`, respectively. The final reports add the unrounded
mitigation values used in the comparisons. Exchange baseline’s comparison is
now one pooled-fight result instead of two per-index result rows because the
observation is pooled; this changes the evidence presentation, not the replay.
The Jem’Hadar-baseline v1 report used a different public-catalog fingerprint
than its final report, so its unchanged output is a parity observation, not a
strict one-variable instrumentation experiment. The Exchange-baseline and
Jem’Hadar-Tom v1/final pairs used the same catalog fingerprint. No v1 report was
available for the Exchange-with-Tom control.

## Verification

The accompanying run logs record 112 Node tests passing, 17 focused Rust tests
passing across the engine golden-master, native combat defense and trace
invariant suites, and Clippy exiting successfully. The full private replay
reports and frozen profile inputs remain outside the repository.
