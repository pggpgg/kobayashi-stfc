# Realta officer corrections, October 2026

The controlled fight used Realta T4/L20, Ent-E Picard T4 as captain,
Ent-E Data T5 and Ent-E Riker T2 on the bridge, and the user confirms seeing
Harry Kim T5 and Tom Paris T4 below decks. Native OfficerSpecs and the game's
English localization confirm that journal officer ID `497650232` is Harry
Kim, whose below-decks ability is “Ashes to Ashes.” The local catalog had
swapped Harry Kim's and Neelix's source-backed records. That catalog error is
corrected while preserving their stable canonical string IDs. The first
isolated replay therefore used the wrong ability despite selecting the
reported officer name. The original export SHA-256 is
`f0bd6ef13ffa7fb4e0f845f59656acc4baf226a97adc33439c943cbff2c924c2`.
The sanitized fixture is
`tests/fixtures/recorded_fights/realta_scouting_bandit_26_observed.tsv`.

## Source evidence

The frozen client static response has SHA-256
`b7fcb7a021fa982144702a682e81eb6b7a31406522b0154cc77553a2c0776418`.
The extracted public officer BuffSpecs, source byte ranges and individual
hashes are retained privately in
`local-analysis/stfc-client/2026-10-22/officer-fixes/client-officer-buffs.json`.

| Officer / BuffSpec | Verified static fields | Controlled-fight observation |
| --- | --- | --- |
| Ent-E Riker / `2481912459` | Self; trigger 10, enemy takes hit; modifier 9, critical chance; operation 1, add; rank 2 value `0.04`; duration 4 rounds | The `0.04` activation follows the first outgoing weapon and precedes the second. |
| Ent-E Picard / `2171230488` | Self; trigger 10; modifier 707, isolytic cascade; operation 3, multiply-add; rank 4 value `0.1`; duration 3 rounds; hostile and non-armada gates | The `0.1` activation follows the first outgoing weapon and precedes the second. |
| Tom Paris / `3192273655` | Self; combat start; modifier 73, all defenses; operation 5, multiply-base-add; officer-stat attribute 2, Defense; rank 4 coefficient `4`; hostile gate | Combat-start activation reports `4`. This is a coefficient, not a final mitigation probability. |
| Harry Kim, source `497650232` / `1091333371` | Round start; opponent target; add-state modifier; state attribute 2, Burning; duration 1 round; rank 5 chance `1`; hostile and Tal-not-on-bridge gates | Both journals contain this ID, the report names “Ashes to Ashes,” and native English localization identifies Harry Kim. |
| Neelix, source `1458469333` / `568169426` | Round start; state attribute 8, Morale; duration 1 round; rank 5 chance `1`; hostile gate | The old catalog incorrectly attached this definition to Harry Kim; it is not the journal's below-decks officer. |

The frozen client OfficerSpecs response, group 7, directly binds
`497650232` to below-decks BuffSpec `1091333371` (OfficerSpec field 26).
It separately binds `1458469333` to `568169426`. OfficerSpec localization IDs
`55003` and `57002` resolve to Harry Kim and Neelix respectively. Native
`OfficerUtils.GetOfficerLongName` formats `officer_name_{0}` and uses the
game's Murmur3 hash with seed `1701` to look up the `officer_names` category.
The English cache contains Harry Kim at key `638718170` and Neelix at key
`-111529161`. This independently resolves the identity discrepancy in favor
of the visible game setup. The source IDs, localization IDs, ability IDs,
hashes and native method addresses are recorded in
`data/officers/client_identity_bindings_2026-10-23.json`.

The canonical records, numeric-ID registry and generated LCARS now agree
with those source bindings. Imported rosters retain their numeric source ID
and re-resolve it through the current registry, so existing tier and level
values follow the correct officer after this correction. The initial
Morale-based replay remains preserved as historical evidence and cannot
validate this fight's below-decks state behavior.

The captain resolver already assigns both the captain maneuver and the
officer ability to the captain seat. The engine previously rejected an
officer ability in that seat, and its duplicate filter could discard the
second ability because the resolver assigned a separate contribution batch.
Both paths now treat the two abilities as contributions from one occupied
captain slot.

## Implemented behavior

Riker and Picard now grant their bonuses after a successful weapon volley.
The granting weapon receives no benefit. Multiple projectiles in one volley
grant once; later weapons can receive the bonus. Grants stack additively
and expire independently, counting their granting round in the duration.
Weapon scope applies to both triggering and receiving the bonus. A lethal
volley ends the fight without an extra activation. The report verifies
first-round ordering; the longer-duration expiry behavior still needs a
matched multi-round game report.

The canonical source trigger is preserved through the typed effect IR.
Ordinary finite pre-attack critical-chance/cascade bonuses keep their
existing round gate; a finite duration alone does not make a post-hit grant.

The verified NS Burning Damage research BuffSpec `1898558353` now contributes
its raw ranked fraction to the native base weapon percentage pool. For a
compiled profile pool of `44.7`, rank 8 adds `6.75`, changing the base factor
from `45.7` to `52.45`; it does not multiply the already buffed weapon by
`7.75`. The typed correction is restricted to that source BuffSpec. Existing
unverified pre-attack multipliers retain their behavior. The arithmetic
regression establishes Burning before RoundStart and does not claim to
settle the order of simultaneous round-start state applications.

NS Burning Isolytic Damage (`1855248137`) also now preserves its verified
RoundStart trigger and Burning condition. Its omitted canonical trigger had
defaulted to AttackPhase. Ranked additive fractions are unchanged. The
runtime regression establishes Burning before RoundStart, so it verifies
the phase and gate without asserting unproved same-phase ordering.

Tom's verified BuffSpec now uses raw points added to armor, shield
deflection and dodge after each stat's multiplicative pool. It does not
add a percentage to final mitigation, and its native coefficient is not
divided by 100. A controlled `420.6` Defense input at rank 4 therefore adds
`1682.4` points to each defense stat. This regression establishes units; it
does not establish that `420.6` is the game's effective combat input.
When the required officer stat is missing, the effect is omitted rather
than using the dimensionless coefficient as a number of defense points.

The correction is restricted to the verified Tom Paris BuffSpec. Other
officer-stat-based all-defense rows retain their existing mapping pending
source verification. Raw additions require the raw defense/piercing combat
context; inputs that lack it do not reinterpret the value as a probability.

## Remaining limits

The game client's inspected preview modifier evaluator does not implement
operation 5, so it does not settle whether Tom's source is his own effective
Defense, total effective officer Defense, or a differently buffed value.
The current compiler preserves its existing own-officer source while fixing
the known units and destination. The incoming damage discrepancy remains
unresolved until that source and missing profile buffs are verified. No
coefficient was fitted to the recorded mitigation.

Harry Kim's corrected state is Burning, not the initial replay's Morale.
The original one-round report cannot validate subsequent repairs or three/four-round
officer expiration. The later automatically captured two-round fight supplies
additional repair and shield-routing evidence.

The fresh fight's `28.64` hull repair is explained by Premium Hostile Combat
Hull Repair research, not an officer or a fraction of maximum hull health.
Native BuffSpec `810035059` has modifier `225` (Hull Repair), operation `3`,
RoundEnd timing and ranked fractions `[0.04, 0.06, 0.08, 0.12, 0.2]`. The
owned rank is 3. Three hostile attacks in round one inflict `123 + 118 + 117
= 358` hull damage; `358 × 0.08 = 28.64` exactly matches the repair before
round two. The compiler now models this with the previous-round received
hull-damage channel at the next RoundStart. A lethal round has no subsequent
repair. Shield damage and maximum hull health are excluded from the basis.

Native English localization explicitly limits this research to non-Armada
hostiles. The override excludes PvP, Armada hulls and group/solo/outpost
Armada engagement tags. Native condition `224` remains unnamed by the
inspected executable; its inferred non-solo-Armada meaning is not presented
as native code proof. The localized scope and exact observed ordinary-hostile
repair support this limited implementation. The evidence boundary is retained
in `data/research/client_hull_repair_2026-10-23.json`.

`tests/officer_post_hit_timing.rs` covers captain eligibility, shared-slot
duplicate handling, post-volley ordering, projectile grouping, stacking,
independent expiry, weapon scope and combat termination.
`tests/officer_raw_defense.rs` covers raw units, rank selection, missing stat
handling, the nonlinear mitigation path and the absence of a raw context.
`tests/officer_native_identity.rs` binds the native identity manifest to the
registry, canonical source records and resolved Burning/Morale behavior.
`tests/research_conditional_defense.rs` also verifies the native weapon pool
arithmetic and preserves the separate legacy multiplier channel.
`tests/research_hull_repair.rs` verifies all three received hull hits, the
exact repair, lethal-fight termination, and PvP/Armada exclusions.
The optimizer fingerprint includes a captain/post-hit canary and behavior
version 5, preventing reuse of results computed before these corrections.
The defense canary also exercises a raw additive officer-defense amount.

An intermediate October 23 diagnostic, before the identity correction and
the later native building/profile changes, reran the production scenario compiler
and public replay function against the frozen isolated profile with the
initially reported Harry Kim crew. It now compiles both
Picard's and Riker's timed abilities, and the individual weapons receive
profile bonuses. After the research catalog refresh, critical chance is
`0.22`, agreeing with the recorded fleet header. The replay still kills the
target with its first outgoing weapon, whereas the game required two. It
therefore does not validate the actual profile's post-hit trace. Controlled
mechanic tests establish the timing correction separately from the remaining
stat-compilation discrepancy. The private output, source/catalog hashes and
temporary diagnostic helper are retained under
`local-analysis/stfc-client/2026-10-23/realta-validation/final/`;
the temporary source suffix was removed with an unchanged original prefix.
