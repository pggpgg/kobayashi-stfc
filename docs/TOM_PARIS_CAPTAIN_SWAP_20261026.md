# Tom Paris captain-swap observations, 2026-10-26

A controlled captain swap changed the measured incoming mitigation. The registered
sum-of-individual-effective-Defense prediction fits both fresh absolute intervals.
This is evidence for a hypothesis, not a verified server formula or a production fix.

Both fights used a fully repaired Realta tier 4, level 20, with Tom Paris level 20,
rank 4 alone below decks. The captain changed from Jadzia Dax level 5, rank 1 to
Mirror Troi level 1, rank 1. Both ordinary level 53 Jem'Hadar Battleships entered
with full hull and shields. Components, target stats, player ship stats, and all
531 deployed player buff signatures matched. Realta maxima were 652,159 hull and
748,507 shield; target maxima were 33,515,867 hull and 25,136,732 shield.

The independent source check passed before the replacement fight's damage was
inspected. The original global-equality check remains failed because a separately
classified impulse-speed buff expired. No combat difference was accepted on that
basis. An earlier Mirror attempt with a damaged target was excluded before its
damage was inspected.

| Captain | Battle time, recorded literal | Clean noncritical hits | Mitigation interval, fraction |
|---|---|---:|---|
| [Jadzia Dax](../tests/fixtures/recorded_fights/realta_jemhadar_jadzia_tom_20261026_observed.json) | 2026-10-26T03:31:48 | 4 | [0.7059202670752873, 0.705923757457368] |
| [Mirror Troi](../tests/fixtures/recorded_fights/realta_jemhadar_mirror_tom_20261026_observed.json) | 2026-10-26T05:35:45 | 4 | [0.7067040722947582, 0.7067077762553273] |

Each primary interval uses weapon `2806302352`. Both fights contain nine incoming
hits: four clean, two shield-depleted, and three terminal. There are no clean
critical hits. The latter five rows do not enter the primary intervals.

For each eligible hit, `D = hull + shield + mitigated + apex-mitigated damage`.
The interval is `[mitigated / (D + 2.01), mitigated / (D - 2.01)]`. All additional
isolytic and critical-mitigation channels equal zero. The existing denominator
budget of 2.01 is unchanged; it is an accounting diagnostic, not a proved server
rounding bound. The raw channels remain distinct from actual health removal.

Mirror minus Jadzia is **[0.0007803148373901614, 0.0007875091800400025]** in fraction
units, or **[0.07803148373901614, 0.07875091800400025] percentage points**. Zero is
outside this interval. Tom is recorded once with ability `3192273655`, value `4`,
before the first attack in each fight. Neither captain has a recorded ability
activation, consistent with their declared Solo Armada or PvP restrictions.

The following points were frozen before the pair. They were not fitted to these
observations. "Compatible" means that the registered point lies inside the
diagnostic interval; it does not mean the server formula is established.

| Registered source hypothesis | Jadzia point | Mirror point | Both absolute intervals compatible | Delta compatible |
|---|---:|---:|---|---|
| Sum individual effective Defense before percentage pools | 0.705922078 | 0.7067056406 | Yes | Yes, 0.0007835624 |
| Raw crew Defense times Tom's context before percentage pools | 0.705928613 | 0.7067148581 | No; both miss | Yes, 0.000786245 |

Thus the delta alone does not distinguish these two aggregation orders. An
own-Tom-only source predicts zero change only if other defense pools remain
unchanged. Raw officer Defense changed from 440 to 451. The recovered client path
uses individual effective Defense above the bonus cap, but the server's bonus
input and threshold remain unbound. A server breakpoint change is not excluded
by the source check alone. Two unchanged deployed buff definitions also remain
unbound.

The [older Jadzia/Tom observation](../tests/fixtures/recorded_fights/realta_jemhadar_tom_20261024_observed.json)
has a clean interval of **[0.7059304997243507, 0.7059335298548556]**. It does not
overlap the fresh Jadzia interval under the same channel accounting and budget.
Both registered Jadzia point predictions miss that older interval. Its cause is
unresolved and the fresh pair does not remove the counterexample.

## Implementation gap

The current runtime scales Tom's own native base Defense, 421 at level 20, and
adds the resulting points after ship percentage pools. Its replay prediction is
about 0.162 for both fresh setups, far below these measurements.

The bundled catalogs do not yet provide the complete native per-officer modifier
56–59 inventory. Profile channels used for ship officer-bonus breakpoints are
not a substitute for separately evaluating each officer's applicable modifiers
and rounding. A faithful implementation needs that source calculation and an
explicit operation order for flat points before percentage pools. The displayed
values in this experiment must not become hardcoded account-wide constants.

The server's combat operation 5 remains unbound. The current client build is
unknown, and the retained build 272 parser has no independent current-build drift
proof. No source coefficient, stacking rule, tolerance, or combat code changed.
The fixtures publish only selected numeric observations and provenance hashes;
they contain no account identifiers, complete journal, or private paths.

The preregistration SHA-256 is
`4bf7950f339667ebb055ebebe13121a55a7546f99fad6287e4d4635ccf966455`.
Each fixture also retains the raw source, setup check, measurement, paired source
check, and historical-discrepancy hashes.
