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

## Officer-stat implementation and saved-fight review, 2026-11-09

The runtime now inventories 113 native officer-stat definitions: 109 supported
setup sources and four quarantined research sources. Each officer retains its
identity, rank, level and seat. Captain, bridge and below-decks pools apply before
aggregation. Production breakpoint lookup uses unrounded totals. Per-officer
rounded values are diagnostic display estimates; these fights do not establish
the server's rounding rule.

This is separate from Tom's combat ability. The canonical model marks that
ability unsupported. The former own-officer, after-percentage-pools proxy and its
roughly 0.162 replay are historical diagnostics, not current production behavior.
Operation 5's server source and placement remain unbound.

Native selector evidence establishes the seat membership independently of the
fight results. Build 272 `BuffTarget` initialization at `0x181b73870` binds
SelfCaptain=3, SelfBridge=4 and SelfBelowDeck=11. The selector at `0x181809570`
includes Captain, FirstOfficer and SecondOfficer for target 4, and excludes all
three for target 11. Binary SHA-256:
`a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274`;
metadata SHA-256:
`ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04`.
This establishes client destinations, not the server's operation-5 consumer.

The frozen account's common Defense fraction is 65.87: research 20.55,
buildings 27.67 and global sources 17.65. Captain and bridge sources each add
0.35; below decks adds 1.0. Thus the captain factor is 67.57, ordinary bridge
67.22 and below decks 67.87. These are derived snapshot values, not constants
in the simulator. Native core Defense is 421 for Tom, 19 for Jadzia and 30 for
Mirror Troi. The production evaluator yields crew totals 29,857.10 and 30,600.37;
the sums of individual display estimates are 29,857 and 30,600.

A private harness exercised the production native-source compiler, canonical
core-stat resolver, scoped evaluator and mitigation function. It used each frozen
profile at its battle timestamp, with no profile writes. The proposed Tom
placement remained an explicit retrospective hypothesis, outside production.
The census covered all 25 retained battlelog files, deduplicated to 12 Tom
journals, plus three historical CSV fights and 11 tracked evidence projections.
Three no-Tom profile controls also passed through the evaluator.

| Saved Tom cases | Count | Result for the proposed crew-Defense source before percentage pools |
|---|---:|---|
| Minimal Jadzia/Tom Romulan 31 and Exchange 40 | 2 | Compatible; both near saturation, so weak discrimination |
| Fresh Jadzia/Tom and Mirror/Tom Jem'Hadar 53 | 2 | Both compatible with rounded and unrounded individual sums |
| Older Jadzia/Tom Jem'Hadar 53 | 1 | Incompatible; retained counterexample |
| Harry/full-bridge, Tom-only and Pike/Tom cases | 5 | Source totals evaluated; no complete bound event/context model for a comparable formula score |
| Wrong Tom seat and damaged-target attempts | 2 | Original setup exclusions preserved; command outcomes not inspected |
| Historical Enterprise-D and Revenant CSV fights | 3 | Unscored: contemporaneous levels, full crew, buffs and weapon identity unavailable |

For unrounded individual sums, the fresh primary predictions are
0.7059221878164819 and 0.7067060058283989. Both lie inside their unchanged
intervals above. The older fight receives the same Jadzia prediction and rejects
it. No observed fixture, accounting budget or calibration band was changed.

## Why the older result still disagrees

The retained cross-date equality proof already establishes that the older and
fresh Jadzia fights have byte-identical seven-file profiles, identical 531 deployed
buff signatures, and identical complete recorded player and target ship stat
maps. Initial/max HP, components, tiers, levels and crew also match. No active
buff crossed an expiry boundary. The proof SHA-256 is
`8c79a68f3305e4af6e8161ed4be18e68a0e36fb630ee1375cf66d8e39b1019d8`.
Changed recorded bonuses therefore cannot explain the residual.

The differing ship attributes -5 and -6 are accrued hull and shield damage.
Primary metadata binds them to `ClientModifierType.ShipHullDamage` and
`ShipShieldDamage`; each value independently equals the outgoing damage ledger.
They are not evidence of a different starting defense pool. Their exact write
timing has not been independently recovered.

The mismatch survives removal of the old critical hit and the same HP/channel
accounting. All clean HP losses equal their reported channels; extra channels
are zero. Aligning the old clean rows with the fresh interval requires roughly
8.4–13.3 additional denominator damage, exceeding the existing 2.01 budget.
Float32 and truncation alternatives are too small to bridge the minimum
0.000006742267 interval separation. Assigning Jadzia an invented higher stat
would contradict the native core-stat and officer-display evidence.

The remaining candidates are unrecorded server state or static configuration,
the server's actual operation-5 source/order, and parser/version drift. The old
fight identifies client build 273; the fresh client's build is unbound, and
the retained build 272 parser has no independent fresh-build drift proof. The
saved evidence does not select one cause. Tom remains unsupported rather than
adding an exception for the older battle.

The fixtures publish only selected numeric observations and provenance hashes;
they contain no account identifiers, complete journal, or private paths.

The preregistration SHA-256 is
`4bf7950f339667ebb055ebebe13121a55a7546f99fad6287e4d4635ccf966455`.
Each fixture also retains the raw source, setup check, measurement, paired source
check, and historical-discrepancy hashes.
