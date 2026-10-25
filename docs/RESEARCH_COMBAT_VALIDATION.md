# Research correction and real-combat validation

The research correction has client-code and catalog evidence. The historical
combat exports do **not** establish its end-to-end accuracy: none is bound to
a verified, immutable player snapshot from the fight. The October 20 controlled
Realta export now matches the player's stated setup and surrounding hashed
captures, but those captures still have incomplete ship and buff coverage.
It exposes substantial simulation discrepancies rather than establishing
accuracy. The checks below preserve these observations without treating a
passing smoke test as an accuracy measurement.

After the five-effect follow-up, the final full Rust run has **1,560 passed,
2 failed, 1 ignored** across 111 targets. Release build, Clippy with warnings
denied, formatting, 42 Node tests, and frontend TypeScript checking pass.
The two remaining failures are the Gorn outcome assertions described below.

## Available evidence

There are seven tracked game TSV exports in `fight samples/`, dated February 23
through June 20, 2026. Six contain attack events. One is a solo armada, outside
the ordinary-hostile comparison; the Borg Sphere export contains no attack rows.
That leaves five ordinary-hostile exports with attack observations.

Six ships in these exports are grade 4 or 5 according to the **current bundled
catalog**. Grade is not recorded in the exports themselves. The logs record
final fleet statistics and ship levels, but not the unbuffed weapon statistics,
research ranks, component configuration, or a complete player-buff snapshot.
They therefore cannot isolate the grade-gated research multiplier.

The available private sync snapshot is not a substitute for a historical
snapshot. For example, its Gorn is tier 6/level 30 while the recorded Gorn is
tier 10/level 50; its Revenant is tier 3/level 15 while the recorded Revenant is
tier 6/level 30. It has no matching Enterprise-D entry. The bundled `demo`
profile also has different research ranks and is not a fight-bound snapshot.
No private profile contents are committed by this validation work.

## Recorded-stat diagnostic

`tests/research_real_log_diagnostics.rs` contains an explicitly ignored
diagnostic. It uses the fleet statistics **already recorded in each export**
and calls Kobayashi's mitigation functions. It does not load a player profile
or apply research a second time.

For each direction it computes:

```text
observed column ratio = sum(Mitigated Damage) / sum(Total Damage)
residual, percentage points = 100 × (component model − observed column ratio)
```

This is a descriptive column comparison. Per-hit effects, morale, hostile
abilities, and differences between fleet-header and active combat statistics
are not reconstructed. In particular, the Gorn outgoing column ratio slightly
exceeds one; it must not be treated as a universal mitigation probability.

| Export | Grade from current catalog | Outgoing/incoming attacks | Outgoing residual (pp) | Incoming residual (pp) |
| --- | ---: | ---: | ---: | ---: |
| Realta / Takret 10 | 1 | 2 / 2 | +8.756794 | −2.699165 |
| Enterprise-D / Hurak 59 | 5 | 154 / 137 | +2.779069 | −2.578644 |
| Enterprise-D / Xindi Cruiser 51 | 5 | 6 / 51 | +0.082751 | −0.118822 |
| Enterprise-D / Enhanced Xindi Cruiser 51 | 5 | 6 / 41 | +0.536345 | −0.000069 |
| Gorn / Acrocanth 60 | 4 | 1 / 2 | −62.287714 | +15.736428 |
| Borg Sphere / Suppressor 55 | 5 | 0 / 0 | unavailable | unavailable |
| Revenant / Borg Type 03.1, solo armada | 4 | 1,140 / 260 | +47.881399 | −7.091633 |

The local inventory and individual hit observations are kept under the ignored
directory `local-analysis/stfc-client/2026-10-20/research-validation/`.
The inventory records SHA-256 hashes of the source logs. The table is a
baseline discrepancy report, not a pass/fail tolerance or a before/after
improvement claim.

The compiled Rust diagnostic was executed on October 20, 2026 and saved
`recorded-stat-residuals.json` in that directory. All attack counts, catalog
grades, and residuals agree with the independent log-inventory calculation;
the largest absolute residual difference was zero percentage points. The
table rounds to six decimals; the Enhanced Xindi incoming residual is
`−0.00006865004146261455` percentage points in the report.

The diagnostic also reports the alternative arithmetic that includes
`pierce_damage_through_bonus`. For the simplest incoming Realta hits, the
observed fractions are `89/133` and `99/148`, with weighted ratio `0.669039`.
The recorded-stat component model gives `0.642047`; adding that extra
damage-through bonus implies `0.552559`. This does not resolve the combat
formula, but it exposes a discrepancy that a winner-only replay would miss.
The alternative is not a claim that every production combat path uses it.

## A regression supported by the raw log

The normal integration test
`realta_log_preserves_raw_damage_and_actual_health_loss_as_distinct_observations`
checks exact observations in the existing Realta export:

- Each of its four attack rows satisfies
  `Total Damage + Total Isolytic Damage = Hull Damage + Shield Damage + Mitigated Damage`.
  This identity is scoped to this export; its mitigated-isolytic and apex columns
  are zero.
- The two incoming hits remove 17 hull and 76 shield, exactly matching the
  player's summary health changes.
- Raw outgoing `Total Damage` sums to 12,958, while the parser's
  `FightExport.total_damage` is 830, the enemy's lost hull plus shield.

This prevents confusing raw attack output, isolytic output, overkill, and actual
health loss when judging future simulations. It verifies log interpretation,
not research compilation or a complete combat replay.

The dedicated integration-test binary was compiled by Cargo and executed:
the normal run had **1 passed, 1 ignored**; the explicit diagnostic run had
**1 passed**. Diagnostic completion verifies that the comparisons can be
produced, not that the combat formula meets an accuracy threshold. These are
targeted results, not a claim that the complete repository test suite passed.
There are still **zero** verified historical player snapshots matched to
these seven exports.

## What the research change can establish

The native evidence described in [the client investigation](STFC_CLIENT_REVERSE_ENGINEERING.md)
supports selecting one ranked entry, and a separate catalog mapping identifies
the three grade-gated piercing statistics. For `4-star Direct Hit`, rank 2 is
`0.24`, rather than the sum `0.21 + 0.24 = 0.45`.

Compiler tests verify this selection, preserve explicitly incremental
authored effects, reject the grade gate for lower/unknown grades, and apply
armor piercing, shield piercing, and accuracy before mitigation in aggregate
and per-weapon paths. A base-stat multiplier is different from adding a generic
damage-through bonus.

Those changes can alter predicted attack/health totals and effective mitigation
in profile-based simulations. They deliberately do not change this recorded-stat
diagnostic: its inputs are already final fleet statistics. Historical log
residuals cannot identify which research rank produced those numbers.

## Initial rank-correction validation (before the five-effect follow-up)

The initial full integration-test run after the research correction reported
three failing targets, recorded locally in `cargo-test.txt` in the validation
directory. Two were combat tests; the third was the server API timing estimate
test, whose two estimates both hit its `0.1` display clamp.

The final `cargo test --quiet --no-fail-fast` run, after the canonical snapshot
correction and fixture repairs below, had **1,542 passed, 1 failed, 1 ignored**
across 110 test targets. Its output is retained in `cargo-test-final.txt`.
All 997 library tests, the 18 research profile tests, the three Hunt tests,
and all 60 server API tests passed. The ignored recorded-stat diagnostic was
also executed explicitly and passed as described above.

`cargo build --release`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, all 38 script tests, frontend TypeScript checking,
and Biome checking of the changed API types passed. The complete Rust suite
is **not green**: the unchanged Gorn calibration assertion remains failed.

The Gorn log calibration remains an unresolved mismatch in the final run. Its
deterministic demo-profile replay won in three rounds with `350,347,181` applied damage; the
historical log won in one round. Across 2,000 demo-profile seeds, the win rate
was `1.0`, round-one kill rate `0.0`, and average remaining hull fraction
`0.8718`. The failed assertion required a round-one kill rate of at least
`0.0001`. The existing damage bands and ability-activation checks passed before
that assertion. Neither the calibration bands nor its Monte Carlo floor have
been relaxed or disabled.

The fixture explicitly documents that the previous round-one floor came from
one qualifying demo seed out of 2,000, rather than a measured historical
probability. Both failing combat tests loaded that demo profile. The rank
correction changes its research contributions substantially; examples from
the tracked demo ranks and catalog are:

| Research | Demo rank | Previous sum of rank values | Selected rank value |
| --- | ---: | ---: | ---: |
| Isolytic Barrage | 6 | `1.05` | `0.30` |
| Critical Fusion Impact | 14 | `6.55` | `0.90` |
| Hostile Tactics | 15 | `5.46` | `0.80` |

These are per-project fractional bonuses, not the ship's final totals.
`4-star Direct Hit` at demo rank 4 also changes from `1.05` accuracy to `0.33`
for each of accuracy, armor piercing, and shield piercing on eligible ships.
Thus the old demo-based outcome distribution cannot serve as a fixed accuracy
target after correcting research compilation. The missing fight-bound snapshot
still prevents deciding how closely the corrected inputs represent the player
in the historical Gorn fight.

The old Hunt the Hunters test compared total isolytic damage against Acrocanth
L60 and Texas-class Supply Ship L45. It observed `124,928,599` versus
`202,206,490` and assumed the tagged enemy must always produce the higher total.
That comparison changes enemy class, defenses, incoming damage, abilities,
health, and potentially the number of shots survived. It does not isolate the
Hunt the Hunters condition.

Its replacement uses the actual catalog's level-50 Hunt ability with matched
combatants and one fixed noncritical shot. The only differing scenario input is
the `gorn_hunter` tag. It checks ability activation and absence, the exact shot
isolytic component (`60,000` versus zero from a `1,000` base shot), and the
corresponding health loss. This is an ability-condition regression, not a
substitute for the unresolved historical Gorn calibration.

The preexisting edits in `src/combat/damage.rs` and `src/combat/engine.rs` were
preserved. Source review establishes that research inputs changed and that the
old Hunt comparison was confounded; it does not attribute a numerical share of
the outcome change to each edit without an isolated before/after replay.

The rank audit also found five canonical effects whose modifiers or conditions
disagreed with their referenced source descriptions. The subsequent
[client-verified corrections](research_conditional_routing.md#client-verified-corrections)
replace those definitions using selective static records and native modifier,
condition, and arithmetic traces. The three defense effects now use raw
defensive-stat fractions, the Morale effect repairs shields, and the
deployment cargo effect contributes no combat bonus.

The raw-defense implementation uses the same component curve with an inactive
or active research condition. It preserves the distinction between fractional
research/passive bonuses and absolute officer-stat defense additions, including
per-round officer-stat changes. Older inputs without usable raw defense context
retain their fallback path. This changes profile-based simulation outcomes; it
does not establish that the full server combat algorithm has been reproduced.

## Fresh capture on October 20

A local ingress server with singular Community Mod message compatibility was
started, and the player launched the game. An immutable `after-login` snapshot
was saved under the ignored `fresh-capture/` directory with hashes. It contains
1,452 research entries, 280 officers, 36 forbidden-tech entries, and 2,133 global
buff entries. The four corrected combat projects are owned at ranks 1, 1, 1,
and 8 respectively; the last project's raw repair fraction is `0.075`.
The live research-summary API was checked against these newly synced ranks:
it reports the three `0.5` defense fractions under their separate status gates,
`0.075` Morale-gated repair, and the owned rank-1 Forge reward project as
`non_combat`. The filtered response is retained privately in
`research-validation/fresh-research-summary-selected.json`.

The client still identifies as build 272. Its static cache refreshed during
this login and was preserved for a second selective source comparison.
At the initial login, the ship and building imports remained old, and no battle
journal arrived. The installed mod reported failed hooks. With the player's
authorization, the official mod was upgraded from 1.1.4.0 to 1.1.8.0 after
verifying the release archive checksum and backing up the installed DLL.
Settings were preserved. The second login refreshed 104 building entries and
the research, roster, forbidden-tech, and global-buff imports. Ship imports
still date from April 5 despite opening ship details and deploying. This is a
**partial fresh capture**, not a complete fight-bound profile.

Global buff sync is currently persistence-only: the scenario compiler does
not consume `buffs.imported.json`. Capturing these records makes the missing
inputs auditable but does not make them simulated effects. Existing imported
buffs also need expiry/reconciliation handling before they can be relied on
as active combat inputs.
All four corrected research buff IDs also occur in the fresh global list with
matching ranks and no expiry. This list therefore includes research-derived
effects as well as timed buffs. Future integration must reconcile sources and
conditions before applying it, or it would double-count existing research.

### Controlled Realta preflight

The player confirmed a fully repaired, deployed Realta at tier 4, level 20,
with maximum upgrades and warp range 33. Its setup is Ent-E Picard as captain,
Ent-E Data and Ent-E Riker on the bridge, Harry Kim and Tom Paris below decks,
Ablative Armor tier 4/level 20, and no chaos tech. The immutable deployed
snapshot retains these manual confirmations separately from the older ship
import. UI health values are rounded displays, not exact server statistics.

| Maximum health | Deployed game display | Compiled replay input |
| --- | ---: | ---: |
| Hull | 652.15k | 31,001.12048 |
| Shield | 748.5k | 37,103.63552 |

The compiled inputs are only about 4.8% and 5.0% of the displayed maxima.
This comparison already rules out treating the current profile-based replay
as a faithful reconstruction. It does not identify one missing multiplier
or establish which source accounts for the full difference.

An isolated replay profile selects only the player's stated equipped
Ablative Armor and no chaos tech; it leaves the live profile unchanged.
Exact input inspection also found that the aggregate attack (`29,211.512`),
critical chance (`0.97`), and critical multiplier (`31.755`) receive profile
bonuses, but each weapon retains attack `44`, critical chance `0.1`, and
critical multiplier `1.5`. The actual outgoing trace uses those weapon
values. This is an independently identified compilation gap, not a failed
condition on the five corrected research mappings.

The Ablative discrepancy now has a native-code explanation. In the frozen
client, `ForbiddenTech.UpdateBuffsRank` (`0x181a31f50`) passes the stored level
unchanged into `BuffData.UpdateRank`; the stored index is `level - 1`.
`GetActiveBuffsByTier` (`0x181a323b0`) selects the first `tier` lists, and
`ResolveReferences` (`0x18183bcc0`) binds those lists to the static buff IDs.
`FleetPlayerData.EvaluateForbiddenTechBuffs` (`0x181a1a400`) uses that selection.
For this equipped T4/L20 item, hull buff `488045092` and shield buff
`3207768523` both select `rankedBuffValues[19] = 1.0`: +100% in the respective
multiplicative-additive stat channels. The existing compiler instead uses
fixed +5% catalog values. This investigation records the defect without
changing forbidden-tech compilation or inventing an overall health multiplier.
Native outputs and their hashes are retained privately under
`forbidden-tech-native/analysis-report.json`.

Other explicit limits are Tom Paris's raw-defense ability being routed through
legacy mitigation and the unconsumed global-buff import.
Harry Kim at the synced rank activates
Morale every round, so this crew cannot provide an inactive-Morale control.
The replay does activate both corrected Morale research effects. The first
controlled target was Scouting Bandit level 26 in Sigma Tama.

### Fresh Realta fight: observed result and remaining errors

The player completed the requested fight and exported its report. The CSV is
tab-delimited; its SHA-256 is
`f0bd6ef13ffa7fb4e0f845f59656acc4baf226a97adc33439c943cbff2c924c2`.
It records Realta level 20, the three specified bridge officers, Scouting
Bandit level 26 with strength 54,559, and Sigma Tama. The export's timestamp
is `2026-10-20 7:45:45 PM`; no timezone is inferred from that text alone.
Its health maxima agree with the earlier rounded UI readings.
Tier, components, below-decks seats, and equipped tech rely on the separately
recorded user confirmations and imports, rather than being invented from the
export. The pre/post imported-file hashes are identical; that establishes
unchanged received data, not successful synchronization of every game change.

| Quantity | Game export | Frozen profile-based replay |
| --- | ---: | ---: |
| Outcome | Victory in 1 round | 36-round limit reached; hostile still alive |
| Starting hull | 652,159 | 31,001.12048 |
| Starting shields | 748,507 | 37,103.63552 |
| Critical chance in fleet header / compiled aggregate | 22% | 97% |
| Critical multiplier in fleet header / compiled aggregate | 3.64 | 31.755 |

The game records two outgoing and two incoming attacks. The incoming rows
remove exactly 228 hull and 1,073 shields, matching both summary deltas.
All four rows reconcile regular damage plus isolytic damage against reported
hull damage, shield damage, regular mitigation, isolytic mitigation, and apex
mitigation. Apex mitigation is nonzero on the incoming hits: 48 and 55.
Outgoing raw regular damage totals 31,494 and raw isolytic damage totals
99,942. Reported outgoing damage includes overkill; actual hostile health loss
is 22,200. These quantities are kept separate in validation.

The component-mitigation diagnostic using the recorded fleet headers gives
66.879521% outgoing mitigation versus the observed weighted 66.425351%, a
residual of +0.454170 percentage points. A separately labeled assumption of
10% Morale piercing gives +0.28028 points. Neither calculation recompiles
player buffs or proves the whole fight. The incoming header-only calculation
gives 29.495006% versus observed 71.217712%, but the log explicitly activates
Tom Paris's raw-defense ability at combat start. Those combat-time additions
are absent from the header-only inputs, so that residual cannot be attributed
solely to the mitigation formula.

The export also places Riker's `0.04` and Picard's `0.1` ability rows after the
first outgoing attack and before the second. The current replay applies
Riker's increase before its first hit and emits no Picard activation. These
are additional timing/coverage findings, separate from the five research
mapping corrections. One round provides no observation of subsequent-round
shield repair, no inactive-Morale control, and no Grade 4+ validation on this
Grade 1 ship. No fitted multipliers or loosened tolerances were introduced.

The original automatic capture failed: the healthy local server received no
new ingress after login, and no `battlelogs.imported.json` was created. The
post-fight snapshot and manually exported report are preserved separately.
In the inspected upstream mod source at
`e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0`, protobuf entity groups 219
(`BattleResultHeaders`) and 221 (`BattleReport`) dispatch to placeholder
handlers. The current client also defines group 251 (`Ships`), absent from
that mod's dispatcher and schemas. Native client dispatch supports these
groups. These are concrete compatibility gaps; the working research sync
does not exercise them.

They are not yet a proven explanation of the exact live response from this
fight. A bounded cache inventory found 472 response files, of which 11 parsed
as protobuf envelopes; none contained groups 219, 221, or 251, and no response
cache file was modified at or after the fight. Other formats and the cache
index were not decoded. Both JSON and protobuf registration paths exist in
the client.

The subsequent capture fix implements all three verified protobuf routes,
shares exact journal IDs with the JSON queue, retries failed fetches and
deliveries, and saves successful history only after destination HTTP
acknowledgement. Count-only diagnostics cover hook responses, entity groups,
queueing, fetching and delivery. Three native regression executables pass:
`battle-capture-tests`, `capture-schema-tests` and `spud-relocation-tests`.
The release DLL was built from base commit `e0b8552` plus the preserved patch
SHA-256 `fd36339ffd54f4da49bbe15b7d1f56b2af6dccc8a7af65d6968b389242fc42ae`.

On October 23, the game was verified closed and the candidate was installed
with SHA-256
`a1ce8edc5e4dad6eaadc8a401e2a6fdba3124d6471269328f979fd7ef2f10328`.
The previous official DLL, settings, journal history and `version999.dll`
were preserved; before/after hashes confirm the latter three did not change.
Local [installation proof](../local-analysis/stfc-client/2026-10-23/journal-capture-installation/installation-proof.json)
records the replacement and rollback hash. With the game closed, the reviewed
[package installer](<C:/dev/stfc-mod/journal-capture-artifacts/2026-10-22/candidate-v1/Install-Candidate.ps1>)
supports `-Rollback` to restore the preserved official DLL. Run the installer
from that complete package; the local proof directory contains source records
without the DLLs.

The subsequent launch exercised protobuf 219 and 251 and automatically
recovered two complete historical journals into Kobayashi. One is the exact
October 20 Realta fight: its battle-time text, hull/level, opponent level and
all six initial/final hull/shield values match the immutable CSV. Its deployed
fleet contains 533 active buff records, component IDs and full stat dictionaries.
The native integer command stream also preserves fractional mitigation values
and separate officer firing/application markers omitted by the formatted CSV.
Local [receipt proof](../local-analysis/stfc-client/2026-10-20/fresh-capture/after-custom-mod-relogin-2026-10-23/live-receipt-proof.json)
and immutable raw snapshots record this result without exposing account data.

The game updated to build 273 during launch, after the earlier build-272 hash
check. Both binaries/metadata were frozen separately; the historical fight's
native analysis still uses build 272. With the capture server ready, a relaunch
delivered 58 ships and refreshed account imports. The Realta remains tier 4,
level 20, with all 14 component IDs matching the historical journal; the 280
officer import rows are unchanged from October 20.

One new controlled fight was then captured automatically on build 273 at the
literal journal time `2026-10-23T05:43:52`. The same Realta/crew/components and
level-26 target produced two rounds and seven weapon attacks. Initial hull and
shield remain `652159` and `748507`; final values are `651722.64` and `746312`.
The journal records `465` incoming hull damage, `2195` incoming shield damage,
and `28.64` hull repair before round two: both final-health identities reconcile
exactly. Its 264-entry command stream is preserved with source hashes in the
local [fresh-fight receipt proof](../local-analysis/stfc-client/2026-10-20/fresh-capture/post-fight-custom-mod-2026-10-23-01/fresh-fight-receipt-proof.json).

This verifies new-fight capture through 219, the complete JSON endpoint and
delivery into Kobayashi. Group 221 remains unobserved and another enabled
destination remains unacknowledged.
Pending jobs retry within the process but are not a durable outbox across exits.
Ordinary ship/account sync does not retry a failed initial upload, which is why
the relaunch was needed after the first login preceded server readiness.

The sanitized fixture is
`tests/fixtures/recorded_fights/realta_scouting_bandit_26_observed.tsv`, with
adjacent hash/redaction provenance. Only 23 player/alliance text cells were
changed; all 123 numeric cells were preserved. Its new observation regression
in `tests/research_real_log_diagnostics.rs` verifies the nonzero apex channel,
summary health losses, and separation of outgoing overkill from actual lost
HP. The targeted crate reports **2 passed, 1 existing diagnostic ignored**;
targeted Clippy with warnings denied and formatting also pass. No production
behavior changed after the earlier full-suite run. This passing regression
validates log handling, not the simulator's failed prediction.

## Five-effect follow-up validation

The first complete follow-up run (`cargo-test-five-mappings.txt`) completed
111 targets: **1,559 passed, 3 failed, 1 ignored**. All 1,006 library tests,
eight new conditional-defense integration tests, and 20 research profile tests
passed. The failures were the golden trace fixture and the two Gorn targets
described below.

The golden fixture differed only by 48 added trace fields: three diagnostic
keys in each of 16 existing counter-fire events. The fixture update adds only
those expected zero/false fields. Removing the additions reproduces the
previous file byte-for-byte; no damage, outcome, RNG seed, or tolerance changed.
The final full rerun (`cargo-test-five-mappings-final.txt`) passes this fixture
and reports **1,560 passed, 2 failed, 1 ignored** across 111 targets. Both
remaining failed targets are listed in the following table. The temporary
diagnostic was removed before this final run.

| Remaining outcome assertion | Follow-up observation |
| --- | --- |
| Gorn / Acrocanth historical calibration | Still wins in three rounds, `350,347,181` applied damage; 2,000 demo seeds yield win rate `1.0`, round-one kill rate `0.0`, average remaining hull `0.8651`. The round-one floor fails. |
| Gorn with Quark alone / Acrocanth | 5,000 demo seeds yield win rate `0.0`; the existing assertion expects greater than `0.99`. |

The Quark test cites the Pike/Moreau/T'Laan game export, which does not record a
Quark-only fight. Both tests use the bundled demo profile rather than a matched
player snapshot. These failures remain visible; their outcome assertions and
calibration bands have not been loosened or disabled.

A temporary diagnostic replay isolated the solo input at seed 42. Its
component-resolved base defenses are armor `28,121`, shield deflection `29,014`,
and dodge `54,475`; the effective defenses are `357,305.426`, `376,195.524`,
and `836,790.475`. The first incoming trace reports raw-stat mitigation
`0.206797`, plus `0.3` flat reduction, for effective mitigation `0.506797`.
The derived pierce term is `0.198301`. The legacy aggregated value is `1.0`:
the old path could saturate from research fractions alone, before buildings
and raw officer contributions. That was not complete immunity because the
separate pierce term still allowed damage through.
The trace also reconciles the officer contribution: Quark's Defense rating
`638.4 × (1 + 16.55 + 4.8605) = 14,306.8632` selects the `+300%`
breakpoint, adding `163,425` raw dodge exactly once. For example, the final
dodge is `54,475 × (1 + 8.94 + 2.421 + 3) = 836,790.475`.

The replay loses in three rounds with `136,091,027` applied outgoing damage.
This identifies a concrete model change behind the solo regression; it does
not prove the new solo outcome matches the game. Its output is preserved in
`gorn-solo-defense-diagnostic.txt`; the temporary diagnostic does not replace
or disable the failing assertion.

The real-log diagnostic was executed again and saved as
`recorded-stat-residuals-five-mappings.json`. It completed successfully, and
every recorded-stat residual in the earlier table is unchanged, as expected
for inputs that are already final fleet statistics. This is reproducibility
of the observations, not an accuracy pass.

All **42** Node script tests and frontend TypeScript checking pass. The new
Rust checks cover selected ranks, each single status gate, per-weapon inputs,
round-start sampling/expiry, additive defense stacking, officer raw-stat units,
monotonic defense benefit, legacy fallback, and shield-repair amount/capping.
The source-class regression also keeps research defense and repair values
unchanged under officer-only Assimilation scaling.

Still unverified are the authoritative server's shield-repair denominator and
full timing order. Inbound opponent Morale retains the existing scalar-pierce
handling, and PvP defender research/dynamic-accuracy behavior is not fully
symmetric. These corrections do not establish complete combat fidelity.

## Reproduction and the remaining capture

Run the observation regression:

```powershell
cargo test --test research_real_log_diagnostics
```

Run the diagnostic explicitly, retaining its JSON locally:

```powershell
$env:KOBAYASHI_REAL_LOG_DIAGNOSTIC_PATH = 'local-analysis/stfc-client/2026-10-20/research-validation/recorded-stat-residuals.json'
cargo test --test research_real_log_diagnostics -- --ignored --nocapture
Remove-Item Env:KOBAYASHI_REAL_LOG_DIAGNOSTIC_PATH
```

To validate the research correction in combat, capture a fight together with
the exact research ranks, ship tier/level/components, all officer ranks and
seats, forbidden tech, buildings and other permanent bonuses, active temporary
buffs, and game/data version. Save the inputs immutably and bind the export by
hash. For the grade gate, include known grade 3 and grade 4+ ships with their
unbuffed and displayed piercing statistics. Repeated noncritical hits against
the same hostile help separate random damage from a persistent stat error.

The immediate comparison should first reconcile compiled fleet statistics with
the captured in-game statistics, then compare per-hit raw and mitigated damage,
and finally compare survival and crew rankings. Missing inputs should remain
explicit instead of being absorbed into fitted damage multipliers or broad
outcome bands.

## October 23: automatic journals and matched Realta compilation

The installed capture patch has now delivered both the historical October 20
report and a fresh controlled fight automatically. The fresh report arrived
through command 219 and was acknowledged by Kobayashi. Its journal records two
rounds, three outgoing and four incoming attacks, with the same Realta tier,
level, components, crew source IDs and Ablative Armor ranks. No second manual
CSV export was needed. Command 221 remains unobserved, and another configured
mod destination still fails independently of Kobayashi's successful delivery.

Journal ingress deduplicates exact integer journal IDs, including IDs above
JavaScript's safe-integer boundary. Retried delivery is acknowledged without
adding a duplicate report. Existing duplicate rows are collapsed on the next
ingress. Unidentified legacy payloads retain their original ordering.

The user's below-decks identification was correct: the native OfficerSpec and
English localization bind `497650232` to **Harry Kim**, whose ability burns the
opponent, and `1458469333` to **Neelix**, whose ability grants Morale. Kobayashi's
two source identities were reversed. The corrected catalog retains stable
canonical IDs while replacing their source-backed abilities and stat tables.
Imported rosters now retain source IDs and resolve them through the current
registry. The two affected rows in the active and isolated validation profiles
were migrated with private backups and explicit reconstruction provenance.

The production scenario compiler, using the isolated captured profile and
the user's confirmed Syndicate level **45**, now produces:

| Starting statistic | Compiled | Recorded |
| --- | ---: | ---: |
| Hull HP | 652159.392 | 652159 |
| Shield HP | 748506.5446 | 748507 |
| Critical chance | 0.22 | 0.22 |
| Native critical-damage stat, before combat effects | 3.64 | 3.64 |
| Each weapon's buffed average attack | 4021.6 | Not independently separated from the journal's fleet statistic |

These are compiled from source contributions, without displayed-stat overrides
or fitted correction factors. Flat research HP and the Realta's level-selected
110 shield points precede the shared percentage pool. Officer Health joins that
pool. Verified native research, building and consumable families use current
active buff IDs, source ranks and raw operation units; known Syndicate minimum
conditions remain attached. Inactive owned Fleet Commander research therefore
cannot enter the hull pool merely because the player owns it. Expiring buff
membership also invalidates persisted optimizer results at its expiry boundary.

The fresh journal independently establishes shield exhaustion behavior. One
outgoing attack reports 9481 shield-assigned damage against 1442 remaining
shield HP, while hull falls by only its direct 2752-point share. Excess shield
damage is discarded. The simulator now follows that rule and sends an entire
hit to hull only when shields were already empty before that hit.

The exact private production replay is retained under
`local-analysis/stfc-client/2026-10-23/realta-validation/source-verified/`.
Matching starting HP and critical statistics does **not** establish accurate
round-level combat. Dynamic weapon stacking, critical-damage interpretation,
isolytic contributions, officer-stat scaling and a fresh round-two 28.64-point
hull repair still require separate validation. Historical diagnostics remain
immutable and their earlier residuals must not be read as current accuracy claims.

## Additional source checks and replay, October 23

The native ThreatConfig DPR equation is `baseDPR × (1 + criticalChance ×
criticalDamage)`. Native critical damage is therefore an additional bonus
in the client rating model. Source-marked ship and hostile records now convert
that raw bonus to a final multiplier at the combat boundary: base `1.5`
becomes `2.5`, and the verified flat profile additions produce `4.64` from
the recorded native stat `3.64`. Public synthetic `Combatant` inputs and
explicit legacy records retain final-multiplier semantics. The native RPN
formulas and their source hashes are in
`data/client_threat_formulas_2026-10-20.json`. This establishes the source
units; it does not independently prove every server-side critical modifier.

NS Burning Damage's verified `6.75` contribution now enters the native base
weapon percentage pool. With profile pool `44.7`, it changes the base factor
from `45.7` to `52.45`, rather than multiplying a compiled weapon by `7.75`.
NS Burning Isolytic Damage also preserves its native RoundStart trigger and
Burning gate. Ordering among simultaneous round-start state applications is
still unresolved; no new sampling-order rule was introduced.

The fresh round-two hull repair is now explained without fitting. Premium
Hostile Combat Hull Repair research is rank 3, selecting native fraction
`0.08`. Round one contains three incoming hull hits, `123 + 118 + 117 = 358`;
the following repair is exactly `358 × 0.08 = 28.64`. Its conservative
canonical override uses previous-round received hull damage at the following
RoundStart and excludes PvP and Armada hulls/engagements. The native localized
scope, ranked curve and the remaining condition-224 inference are retained
in `data/research/client_hull_repair_2026-10-23.json`.

A new production replay is retained privately under
`local-analysis/stfc-client/2026-10-23/realta-validation/native-pool-critical-repair/`.
Starting HP, shield HP, critical chance and the two `4021.6` average weapon
values remain as reported above. The replay's seed zero still ends after
two outgoing attacks in one round; the fresh game fight takes three outgoing
attacks over two rounds. The compiler at that point produced static isolytic
damage `6.685` and isolytic defense `4.06`, while the launched native buff
audit contributes `1.18` and `2.16` respectively. Remaining runtime sources
and legacy mappings must be reconciled. Simulated incoming mitigation is
`0.599309`, compared with approximately `0.712` reconstructed from the game
journal. Tom's effective officer-stat source remains unverified. These
residuals prevent an accurate-combat or reliable-crew-ranking claim.

The new critical-unit, native weapon-pool, RoundStart gate and previous-round
repair regression targets pass. Whole-workspace formatting and all-target
Clippy with warnings denied pass. The updated golden master, Galaxy morale
and Breen routing tests pass. The legacy Gorn calibration still fails its
old damage bands; it uses an unmatched demo profile and different damage
accounting assumptions. Its bands were not widened to hide that failure.

## Fresh export cross-check and isolytic source correction

The user exported the automatically captured two-round fight as
`2026-10-23 05-43-52.csv` (tab-delimited despite its extension), SHA-256
`c05ac56dfad6dbcf1e4a7319e1832d89199bc0479472fded74e29f714128717a`.
All seven attacks agree with the decoded automatic journal: 49 damage-component
cells, seven regular-damage totals, critical flags and round numbers match the
client's integer truncation. In particular, outgoing regular damage is
`3937`, `3452`, `32008`; the low second hit is present in the game export too.
The numeric repair event is omitted from the CSV, and its final HP is truncated.
A sanitized observation fixture preserves every numeric cell and distinguishes
465 gross incoming hull damage from 437 displayed net hull loss.

Native `ModIsolyticDamage(707)` ADD now supplies the selected research,
building and consumable baseline families. MULT remains a separate cascade
channel. Source membership, rank, expiry and known class/account conditions are
retained; unknown conditions stay quarantined. This replaces the legacy `6.685`
isolytic baseline with source-backed `1.18` for this Realta: `0.305` research,
`0.875` buildings and no supported consumable contribution. Grade-5 FKR,
Augment-Exile and other unrelated conditional research cannot become
unconditional bonuses for a grade-1 Realta against a Scouting Bandit.

Native `ModIsolyticDefense(808)` ADD now follows the same selected-source
contract. The matched active-buff list has five research rows totaling `1.43`
and one building row contributing `0.73`; the current synced snapshot selects
the same six one-based ranks, so the source-backed setup defense is `2.16`.
One research row is Explorer-gated. The active group-116 `1.5` defense row has
an unsupported condition and remains excluded. Its static definition, like all
research and building 808 definitions, is unchanged between retained client
builds 272 and 273. The legacy unconditional `4.06` no longer enters this
profile's selected setup pool. This source correction does not establish the
game's server-side isolytic damage formula or the unresolved incoming mitigation.

The production public replay API was rerun for sample seeds 0, 1 and 42 with
the unchanged isolated profile. All three now win in two rounds. Their private
outputs are in `realta-validation/source-verified/replay-native-isolytic.json`.
This agrees with the fresh fight's round count, but does not reproduce both
recorded fights or establish reliable crew rankings. Per-hit damage still
differs. An inferred effective isolytic baseline of `1.805` from the recorded
ratios has an unexplained `0.625`; neither number is installed as an override.
The retained static-source audit found no additional proven source after
checking every declared BuffSpec-bearing path present in both client caches.
Two journal IDs still have no matching static definition, and client caches
do not establish every server-side runtime effect.

Incoming mitigation, Tom's effective Defense source, simultaneous RoundStart
ordering, Court of Q condition 57 and the server's damage-roll distribution
remain unresolved. Native weapon minimum/maximum are 40/48 in both retained
client builds; the existing simulator still fires their average. No fitted
variance was introduced to hide the observed below-bound hit. These limits
remain material even though starting hull/shield and several mechanics now
reconcile exactly.

The converged full Rust run reports **1,617 passed, 1 failed, 1 ignored** across
117 targets. The sole failure is the unchanged legacy Gorn calibration described
above. An optimizer regression exposed by the new physics was corrected:
priority-queue deeper scouting now uses the same random panel as uniform
scouting. Its original 95% finalist-recall, winner agreement and reduced scout
work assertions pass; no threshold was relaxed. All 60 script tests and 286
frontend tests pass, as do the frontend build, formatting, all-target Clippy with
warnings denied, and the release build.

The checked release is running on the existing local capture endpoint. The
next Tom-only control arrived automatically after restart, with four stored
journals and four distinct exact IDs. Its journal confirms unchanged Realta
maxima and Tom as the only below-decks officer. The actual mod-routed player
profile now carries the supplied Syndicate level 45; the separate old default
profile was left without that account-specific context.

## Tom Paris controls, October 23

Two further fights arrived automatically at literal journal times
`2026-10-23T15:45:25` and `2026-10-23T15:57:27`. The first has Tom Paris alone
below decks; the second adds classic Pike. All 532 launched buff definitions,
including ranks and attributes, all components, tier, level and maximum HP
match between the controls. The bridge remains Ent-E Picard, Data and Riker.
The journals identify the expected officer source IDs, including classic Pike
`329940464`; the earlier Harry Kim identity correction is independent of this
comparison.

Incoming regular mitigation reconstructed from the damage components is
approximately 71.196% in the Tom-only fight and 71.185–71.212% in the Pike
comparison. Integer hull/shield damage and rounded apex damage limit that
reconstruction. Both effective-own and effective-crew Defense hypotheses,
using the fixed native coefficient four, saturate near the formula's 71.2%
asymptote against this target. Adding Pike therefore does not distinguish
those two hypotheses. The instance officer getter and native ability value
alone do not establish Tom's combat stat source; no fitted multiplier or
effective-source override was added.

The Pike fight independently confirms the research repair denominator and
timing: round-one received hull damage is `103 + 115 + 115 = 333`, followed by
exactly `333 × 0.08 = 26.64` repair at round two. Maximum hull/shield remain
652159/748507. Unlike the Tom-only critical kill, all three outgoing Pike-control
hits are noncritical. Their isolytic/regular ratios are approximately
2.22678, 2.45790 and 2.68779, with successive increases around 0.23 after
Picard's post-hit applications. The seven launched 707/ADD source rows still
sum to 1.18 in both controls. These observations narrow the unresolved runtime
damage behavior; they do not identify an additional native source or justify
changing the baseline to a ratio-derived value.

Immutable snapshots, decoded events and independent comparisons are retained
under the ignored `local-analysis/stfc-client/2026-10-23/` directories. A
Pike-only control against the same hostile was subsequently captured to measure
mitigation without Tom before attempting to distinguish effective-stat sources
on a stronger target.

## Native defense phases and independent controls

The Pike-only journal at `2026-10-23T16:11:41` and Tom-only level-31 Romulan
Patrol journal at `2026-10-23T16:36:02` both arrived automatically. The latter
uses a battleship in Gradientes, whose two native weapons independently select
armor piercing 3300, shield piercing 23100 and accuracy 2888. It is not the
previously proposed level-33 target, which was unavailable in the system.

The native stat-type dispatch binds modifiers 11/12/13 to dodge/armor/shield
deflection and modifier 73 to all three. Research, buildings and consumables
now retain their separate source families and ranks for these launch pools.
For the controlled Realta, their all-defense pool is `10.05 + 7.84 + 6.5 =
24.39`. Individual armor/shield/dodge pools add `3.4 / 4.4 / 3.8`; the Explorer
officer-stat breakpoint contributes one further shield-deflection fraction.
The resulting launch points `86.37 / 677.38 / 29.19` round to the game export's
`86 / 677 / 29`. Numeric journal dictionary keys were not used to assign stat
names.

CombatStart modifier-73 MULT sources form a separate persistent pool. Four
applicable research contributions are `1.75 + 4.5 + 3 + 0.75`; nine NPC-scoped
consumable contributions total `3`, producing `13` in these controls. The
source compiler keeps CombatStart timing and typed opponent gates, and the
engine now includes that phase alongside live RoundStart defense effects.
Enemy-class conditions 4/5/6 are enabled only for three individually verified
research IDs whose native project-to-localization bindings identify hostile
Explorer/Interceptor/Battleship scope. Their server predicate bodies were not
recovered; these narrow scope bindings remain explicit in the compiler.
Unknown conditions on other sources remain excluded.

The native point getter rounds `(base + flat) × (1 + percentage)` away from
zero at midpoints. Source-backed scenario compilation now enables integer
defense points before the inbound mitigation curve; synthetic contexts retain
fractional arithmetic by default. For the Pike-only control, the combat pool
adds `3/22/1 × 13`, yielding `125.37 / 963.38 / 42.19`, then `125 / 963 / 42`.
These independently selected values predict mitigation `0.36344661780219967`.
The recorded mitigated damage `876.269782228722` divided by nearest-integer
reconstructed raw damage `2411` gives `0.36344661228897635`, a difference of
approximately `5.5e-9`. Raw damage is reconstructed from rounded components,
not a direct journal field. This validates this defense path, rather than
every server-side rounding stage or the complete combat model.

All five incoming level-31 Patrol hits have mitigated damage equal to exactly
`0.712 ×` their nearest-integer reconstructed raw hits. Round-two repair is
also exact: `(490 + 438 + 1170) × 0.08 = 167.84`. This stronger target disfavors
the current launch-own-effective Tom Defense hypothesis under the current
additive raw-point placement: it predicts about 71.04%, below the recorded
71.2% cap. Effective crew Defense reaches the cap, but native operator-5
placement and the combat officer-stat source remain unbound. The fight
constrains their combined behavior; it does not prove crew-total sourcing.
Tom's source has therefore not been changed to fit these observations.

The added launched buff in the Patrol control is independently identified as
impulse velocity, while an expired global warp-speed buff disappears. Crew
progression, components, forbidden technology and maximum HP match. The
snapshots are not described as completely identical. No additional active
CombatStart officer-stat source was found in the bounded native cache audit.
Weapon-roll distribution, remaining isolytic/APEX behavior and simultaneous
officer timing still prevent a reliable crew-ranking claim.

The redacted observation fixture
`tests/fixtures/recorded_fights/realta_defense_controls_20261023_observed.json`
retains these incoming damage components and repair events with original capture
hashes. It is an observed-accounting fixture, not a whole-fight accuracy gate.
The production replay API independently returns integer defense `125/963/42`,
CombatStart pool `13` and mitigation `0.363447` for Pike-only. Its sampled fight
still takes three rounds versus the observed one-round critical kill. Tom-only
still predicts approximately `0.633176` against the Bandit, below observed
`0.712`; neither residual is hidden by the successful defense calculation.

The integrated Rust run after the source compiler changes reports **1,634 passed,
1 failed, 1 ignored** across 118 targets. The failure remains the unchanged legacy
Gorn calibration. Subsequent focused checks pass for the redacted observations,
the fractional native-rounding canary and all 61 HTTP API tests. Live replay
also exposed an API bug that duplicated the first supplied officer into omitted
slots. Missing slots now stay empty; an end-to-end one-below-decks replay
regression passes. All-target Clippy with warnings denied, formatting and the
release build pass. The earlier frontend checks and 67 script checks remain
applicable; no frontend or script changes were needed for this final integration.

## Independent ship-detail screenshots

Three user-supplied Realta screenshots independently display armor `86`, shield
deflection `677`, dodge `29`, isolytic damage bonus `118%` and isolytic defense
`216%`. These agree with the native launch compilation above, including the
corrected isolytic fractions `1.18` and `2.16`. Displayed hull `652.15K` and
shield `748.5K` agree at the UI's abbreviated precision with compiled maxima;
the screenshots do not establish their exact unit values. The displayed DPR
`7401` also matches the prior game export's rating, without resolving the
remaining per-hit weapon discrepancy.

The ship's aggregate Defense `3960` and Officer Defense Bonus `100%` are distinct
from Tom Paris's individual officer Defense. These screenshots validate setup
stats, but do not identify the runtime stat or operator placement used by Tom's
ability. The images and a transcription with hashes are retained privately under
`realta-validation/ship-detail-screenshots/`; no simulation coefficient changes
follow from this observation alone.

The subsequent individual-officer screenshot used the ship-assignment popup
after returning to dock; that access path was unavailable while deployed. It
shows Tom at level 20 with Attack/Defense/Health `16889 / 28847 / 33769`, and
crew totals `133418 / 140635 / 147415`. A separate native source audit reproduces
all six integers exactly, rounding base stats before percentage pools and each
effective officer stat before summation. Tom's native rounded bases are
`252 / 421 / 505`; the matching percentage factors are `67.02 / 68.52 / 66.87`.
The matching crew calculation includes the captain in bridge-target bonuses.
These are source-audit predictions, not a claim that the production compiler
already reconstructs every officer modifier.

The Defense prediction `28846.92`, rounding to `28847`, was retained before
this screenshot as a candidate including research condition 231. Native project
and English-localization bindings now identify its Defense source as Bajoran
Inspiration, with Attack counterpart Rally the Resistance; both are described
as applying during an Assault. Their matching docked presentation does not prove
ordinary-hostile applicability. The screenshot therefore resolves the displayed
individual-versus-crew numbers, while Tom's combat-time source and operator-5
placement remain unresolved. No displayed-stat override was installed.

The officer-source audit also exposed an independent Orlop Defense compilation
error. Native buff `1895868262`, project `282179003`, has modifier 57, fractional
MULT operation, target 11 (below-decks officers), Launch timing, and no conditions.
Its final owned rank is `1.0`, whereas display-flag inference had reduced it to
`0.01` and merged it as crew-wide `officer_defense`. The importer now requires
the exact native source, project identity and rank curve, and emits
`officer_defense_below_decks`. Runtime aggregation applies that percentage only
to below-decks Defense before the ship breakpoint lookup. It does not change
Tom's ability source or directly add shield mitigation. The public source record
is `data/research/verified_orlop_defense_2026-10-23.json`.

Tests cover both end ranks, the research-to-profile path, moving identical total
Defense between bridge and below decks, acceptance by profile validation, and
the absence of direct ship-mitigation changes. A sixth optimizer canary exercises
the new scope and behavior version 7 invalidates older results. The full Rust
run reports **1,639 passed, 1 failed, 1 ignored** across 119 targets; the sole
failure remains the legacy Gorn calibration. All 68 script tests and all-target
Clippy with warnings denied pass. The source-audit match for the docked officer
display is separate from these runtime tests.

The user then accessed Tom through the Officers menu while he was deployed.
This screen displays current Attack/Defense/Health `16725 / 28573 / 33769` and
strength `395335`; green upgrade-preview increments are separate. All three
values exactly match the previously derived native calculation excluding the
Assault condition-231 rows: `252 × 66.37 = 16725.24`, `421 × 67.87 = 28573.27`,
and `505 × 66.87 = 33769.35`, rounded to integer points. Relative to the docked
ship-assignment popup, Attack is 164 lower, Defense 274 lower and Health unchanged.
This is consistent with omitting the existing conditional `0.65` Attack/Defense
contributions. Both the menu and deployment state changed, so this pair does not
isolate the cause of that presentation difference. It strengthens the independent
effective-stat check while leaving Tom's combat consumer and operator placement
unresolved; the displayed Defense was not installed as a combat override.
As a diagnostic, applying native coefficient four to the observed individual
Defense `28573` after the existing ship percentage pools predicts Patrol
mitigation `0.7104044108188787`, compared with `0.712` across the five recorded
incoming hits. This leaves the combined source/placement model unresolved;
the independently checked officer-display arithmetic alone does not fix it.

The next proposed discriminator removes the large bridge stat contribution.
The user confirmed that an empty captain prevents deployment, so the initially
proposed all-empty bridge is invalid. The revised legal control uses owned
Jadzia Dax level 5/rank 1 as captain, both other bridge seats empty, and Tom alone
below decks, against the same available level-31 Romulan Patrol in Gradientes.
Jadzia's retained native buffs target self piercing and weapon damage, with
Solo-Armada scope; neither changes defenses. Her native rounded base Defense is
19. Together with Tom, the raw crew candidate is 440 and the independently
derived effective crew candidate is 29857, excluding Assault research.

Predictions are fixed before observing this control. Applying raw crew Defense
before the ship percentage pool gives mitigation near 69.76%; applying effective
crew Defense afterward gives 71.082623%; an effective-stat contribution before
the full pool still saturates near 71.2%. The complete private hypothesis matrix
also retains alternative source, rounding, and breakpoint/placement policies.
These are experimental predictions, not installed model changes. Native weapon
IDs, Tom coefficient four, officer slots, source buffs, and health maxima must
be checked in the new journal before interpreting the result.

## Minimal legal crew: Jadzia and Tom

The automatic journal for the requested control arrived at literal battle time
`2026-10-23T18:00:38`. All 18 setup checks pass: Jadzia is level 5/rank 1 in the
captain slot, both bridge seats are empty, and Tom is level 20/rank 4 alone below
decks. Realta tier/level/components, Ablative Armor, full starting health
`652159/748507`, and the level-31 Patrol's hull/components/weapons match the prior
control. The deployed buff list removes only Picard's captain reward buff;
relevant source membership and ranks are unchanged. A noncombat impulse boost's
expiry was refreshed. Native raw crew stat keys 56/57/58/59 become
`501/440/773/1714`, matching the independently calculated officer sums.

The fight lasts seven rounds and records 31 damage hits, including 17 incoming
hits, plus three separate weapon-charge actions. The older diagnostic decoder
rejected those shorter actions. Native `BattleLogParser.ProcessAttackData`
branches on marker `ATTACK_CHARGE` (`-95`) and fills `ChargePercentage` instead
of damage fields. The private decoder now preserves this verified branch;
five recent decoded fixtures remain byte-identical, and the older historical
fixture retains the same damage-hit projection.

All 17 incoming hits reconstruct to mitigation `0.712` under the previously
stated nearest-integer raw-damage hypothesis. Allowing a conservative diagnostic
error budget of one point for each integer hull/shield component and `0.01` for
apex gives a joint mitigation interval of `0.7119496762–0.7120462222`. This
rejects both own and crew effective-Defense additions *after* the percentage
pools (`0.71040441` and `0.71082623`), as well as the raw-Defense flat additions
before those pools (`0.69463177` and `0.69764794`). Twelve of the 24 recorded
source/placement candidates remain compatible, including effective-stat flat
additions before amplification and raw-stat multipliers on completed defenses.
The experiment therefore narrows the alternatives without identifying Tom's
stat source or proving one operation-5 interpretation.

The running simulator's same-crew replay reports defense `1808/2646/1725` and
mitigation `0.223498`, retaining the known unbuffed-own-stat, post-pool model.
This is a confirmed model discrepancy, not a successful Tom calibration. The
replay uses a diagnostic seed rather than the server's random seed. The fidelity
note now records the larger minimal-crew mismatch, and no fitted stat override
or speculative stacking change was installed.

The longer control independently checks two existing mechanics. All six round
repairs equal `0.08` times the previous round's received hull damage. Shield
absorption is `0.825` in rounds 1–2 and `0.8` in rounds 3–7, within integer
rounding. Owned KSG Shield Mitigation, native project `2392190200` and buff
`1478241687`, supplies `0.025` for two rounds: self target, CombatStart, ADD,
modifier 76, duration attribute 2. Its native curve and localization agree with
the existing compiled `0.025/2` research mapping. This timing is independent of
Tom's unresolved Defense source.

The redacted recorded-fight fixture retains the new control, and a focused
observation-accounting regression checks the seven-round cap, all six repairs,
the two-round shield split and charge-versus-hit separation. All five
`native_combat_defense` tests pass. These checks preserve measured evidence;
they do not assert that the current Tom simulation matches it.

A further bounded client trace finds that generic buff application handles
operations 1/2/3/4/7/8 but does not evaluate operation 5. It confirms the generic
`(base + flat) × (1 + percentage)` path for those supported operations, without
binding Tom to that path. Enum naming alone does not establish combat semantics.
Another fight should be chosen only after checking that its piercing separates
the remaining capped models and that its actual target/setup is available.

The locally listed next candidate is level-40 Exchange Transport `3053281684`
in Laramie, warp 24. Its Survey hull excludes the enemy-Battleship research
contribution `0.75`, leaving the previously supported CombatStart pool `12.25`.
It is also an Eclipse-faction target, not a faction-neutral substitute. A
further native source, project `3260398513` / buff `1238121830`, supplies
CombatStart AllDefenses `7.0` at the owned rank, with condition 17 and attribute
8 containing the target's exact faction ID. Its localization explicitly names
Eclipse enemies. This gives a narrowly source-supported additional candidate
pool `19.25`; the condition consumer remains a separate verification step.
The parallel Stella-only source also requires condition 150 and is ineligible
to Realta. Neither source was promoted into production from an experimental fit.

The cached target has roughly 17.9 million combined hull/shield health and can
destroy the Realta over a full fight. Conservative catalog critical bounds are
below the Realta's full hull for an individual projectile, but do not establish
full-fight survival or rule out unmodeled conditional modifiers. At the scouting
stage, the user was asked only to check which Transport levels are actually visible, with no attack
requested. The September-28 cache establishes local map/catalog consistency;
it does not prove current spawn availability.

The user subsequently confirmed a level-40 Transport is visible. The next
requested control removes Tom, retaining only Jadzia as captain, with the bridge
and below decks empty. The likely Realta loss was disclosed before this request;
early nondepleted projectile records are the intended measurement. With native
point rounding and ship officer Defense bonus bracketed from zero to one, the
baseline pools predict mitigation `0.1618875443–0.1618935504` versus
`0.1619321873–0.1619381955`. Their gap exceeds the combined conservative
fractional-ratio uncertainty at reconstructed raw damage of at least 30,000.
Random weapon rolls are not reconstruction error when the fractional mitigated
numerator and all damage components are available. Hits with additional damage
channels, critical handling, depleted shields or death require separate accounting.

The unchanged minimal Jadzia/Tom follow-up also has predictions recorded before
its request or result. Across both Eclipse pools, source rounding and officer
bonus brackets, the 12 surviving hypotheses form 11 disjoint accounting bands
at the stated damage resolution. A capped result still leaves six hypotheses;
a noncap band can isolate one of the other source/placement candidates within
this finite model set. This does not make the set exhaustive. Private freezing
and comparison helpers preserve exact journal bytes and explicit uncertainty;
no combat coefficient changes follow from the experimental design.

## Exchange baseline receipt

Automatic capture received the no-Tom baseline at literal battle time
`2026-10-23T18:59:03`. All 25 independent setup checks pass: Jadzia alone at
level 5/rank 1, identical Realta tier/level/components and tech, full starting
`652159/748507` health, exact level-40 Exchange Transport component sequence,
faction `1750120904`, and armor/shield/accuracy piercing `382000/382000/382000`.
The only relevant snapshot change is expiry of the noncombat impulse-speed
boost. Fourteen roster rows gained source-ID metadata without progress changes;
Tom remains level 20/rank 4.

The Realta loses in round 17. The receipt contains 82 damage events, including
50 incoming hits, plus eight distinct charge actions and 16 hull repairs.
Twenty-six incoming noncritical shots retain both hull and shield and have zero
additional damage channels. Their conservative reconstructed-damage intervals
intersect at `0.1619330973–0.1619424657`, excluding the previously compiled
`12.25` CombatStart pool and overlapping the source-derived Eclipse `19.25`
pool. All 26 nearest-integer raw hypotheses yield the same ratio,
`0.1619379182638594`. This supports the narrow Eclipse source mapping, without
proving the server semantics of condition 17 for other definitions.

One small precision discrepancy remains. Rounding the full compiled defense
sum gives `144/1101/48` and mitigation `0.1619381955`; `144/1100/48` gives
`0.1619379224`, much closer to the observed ratio. Native client launch rounding
is independently proven to round positive midpoints away from zero. The combat
server's phase boundaries and float accumulation are unbound; a separate
CombatStart rounding step or a value just below a half point are hypotheses.
Neither a tie-to-even rule nor a fitted shield-stat correction was installed.

All 16 repairs equal `0.08` of the previous round's reported incoming hull
damage. Clean shots retain the `0.825` shield split in rounds 1–2 and `0.8`
thereafter. Shield depletion in round 11 and two reported shots in the lethal
round 17 do not obey a naive remaining-health subtraction ledger; they remain
explicit exceptions and are excluded from the baseline ratio measurement.
The new redacted fixture and observation tests preserve these distinctions.

The next requested fight adds only Tom below decks, with the same captain,
target, ship and tech after full repair. Its predictions were frozen before
requesting it, selecting the source-derived `19.25` pool supported by this
baseline. A noncap result could distinguish additional source/placement
families; a capped result still cannot resolve all remaining models. The likely
Realta loss was disclosed, and no CSV export was requested.

## Paired Exchange fight with Tom

The requested follow-up arrived automatically at literal battle time
`2026-10-23T19:11:30`. All 29 independent setup checks pass. Adding Tom
level 20/rank 4 below decks is the sole verified change: captain, empty bridge,
full repair, ship/tech/components, target stats and components, progression,
and all 2,134 global and 531 deployed buff entries match the baseline.

The fight lasts 47 rounds and records 235 damage events, including 142 incoming
hits, plus 23 distinct charge actions and 46 hull repairs. Eighty-two clean
incoming shots yield a joint diagnostic interval
`0.7119816445–0.7120187293`. The nearest-integer raw hypotheses give ratios
`0.7119999988188029–0.7119999988188032`. This eliminates the raw-source
completed-defense multipliers, the effective-source percentage-pool variants,
and the variants amplified only by CombatStart. Six pre-recorded effective-stat
models remain: own or crew source, each with pre-all flat addition, flat addition
excluding the ship officer multiplier, or multiplication of completed defenses.
These remain observationally compatible at the conservative resolution.

A fixed arithmetic exploration finds a more specific tracing lead. Applying
the independently derived officer percentage pools to unrounded native base
Defense gives `420.6 × 67.87 = 28546.122` for Tom and
`19.13 × 67.57 = 1292.6141` for Jadzia. Rounding final officer stats gives crew
Defense `28546 + 1293 = 29839`. The crew-source, pre-all placement predicts
`0.7119999988188030`, reproducing the apparent ratio. Several intermediate
precision routes produce the same integer, so this does not identify every
rounding boundary.

This close match is exploratory rather than a recovered server rule. Exact
mitigation `0.712` with raw damage slightly below each reconstructed integer
also fits every shot within the documented component error budget. The retained
client parser proves how fractional source fields are displayed, not the
server's raw-roll/reduction precision. No raw-damage field or server operation-5
consumer is bound. The new fixture and observation tests retain this uncertainty;
no displayed officer-stat override or Tom source/stacking change was installed.

The Eclipse research mapping is now implemented and deployed with a narrow
source guard: native group 66, buff `1238121830`, its exact row hash, complete
rank/chance/target/trigger/attribute shape, and raw defender faction
`1750120904`. The one-based active rank selects a single value (`7.0` at rank
10). Generic condition 17 and other faction rows remain unsupported. Static
research import explicitly omits this source, preserving its faction scope and
avoiding a duplicate of the native CombatStart seat. The public evidence record
is `data/research/verified_eclipse_defense_2026-10-23.json`; behavior version 8
invalidates prior optimizer reuse.

Focused validation passes: 36 native-source Rust tests, 12 integration tests
across native defense/Orlop/both Exchange observation crates, and all 69 script
tests. All-target Clippy with warnings denied, formatting and the release build
also pass. These are focused checks, not a rerun of the full historical suite.
The new server's live API reports the Exchange pool changing from `12.25` to
`19.25`, defense `144/1101/48`, and mitigation `0.161938`, inside the recorded
diagnostic interval. The non-Eclipse Pike control remains `125/963/42`, pool
`13`, mitigation `0.363447`. All ten journals remain intact after the restart.

No further fight was requested after the pair. A local feasibility scan finds
14 reachable ordinary Exchange Transport entries through level 40, all too
close to saturation to robustly distinguish the remaining models. The stronger
Secure Lockup is explicitly an Armada target and was excluded. The leading
crew-Defense arithmetic remains an inference requiring independent source or
more discriminating data; the current Tom production model retains its known
mismatch.

## Planned control beyond the Realta's own warp range

The next target search preserves the audited Realta instead of immediately
switching hulls. The account snapshot has Discovery Summoning unlocked; Scopely's
[Summoning documentation](https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/5133-u-s-s-discovery---summoning/)
allows summoning a ship regardless of that ship's warp range. The player reports
Discovery Warp Range approximately 584. No new fight has been observed for this
plan.

The useful cached candidate is ordinary level-55 Klingon Patrol battleship
`3293456586`, in Aeryndor, Nimrith or Rylothor at warp 230. Its individual weapon
components have penetration/modulation/accuracy `385053/627163/366717`. Two
energy weapon entries each fire three projectiles at catalog damage
`784351..958663`; the kinetic entry fires three at `1876342..2293322`. All warm
up at round 1. The component list does not establish server firing order. There
are no ship-level or component ability rows in the cached definition; actual
target identity and availability still require a fresh journal.

An independent native-source audit selects CombatStart AllDefenses pool `13`
against this NPC battleship. The faction-matching Klingon Analysis source
`3378637523` is explicitly against Klingon **player ships** and is excluded.
The Eclipse-only `+7` source is also excluded. A local baseline replay confirms
the compiled counters `125/963/42`, pool `13`; this is a simulation diagnostic,
not a new in-game observation.

With Jadzia captain and Tom alone below decks, the four surviving flat-addition
families predict mitigation approximately `0.7119671..0.7119846`, whereas the
completed-defense multiplier families remain approximately `0.712`. This is
below-cap discrimination at native journal precision, not a visibly large UI
percentage difference. Predictions must retain both officer-stat precision
routes and the ship officer-bonus assumptions. The catalog damage range is not
a proven server raw-roll floor, given the unresolved weapon-damage discrepancy.
Only captured, noncritical hits without shield depletion, overkill or unbound
damage channels may be compared; whole-volley survival is unnecessary.

All three cached systems have enabled level-1 Magnetic Field hazard
`3095062369`. They must not be described as hazard-free. Scopely's general
[hazard guidance](https://scopely.helpshift.com/hc/en/19-star-trek-fleet-command/faq/7756-the-hazardous-road-to-the-top-hazards/)
says hazard damage stops in combat, but does not bind this hazard's specific
effect. Battle-entry hull/shield and active source changes therefore require
validation before using the result. The target, native pool, hazard audit and
pre-observation predictions are retained privately with source hashes. Tom's
production model remains unchanged pending discriminating observations.

### Live scout correction and critical-hit accounting

The player scouted Aeryndor and reported a level-52 Klingon Patrol battleship,
with no level-55 battleship visible. Cached system membership/counts are possible
target leads, not a guarantee of a currently available spawn. The supplied
in-game hazard tooltip identifies **Magnetic Minefield, level 1**, at 1% of
maximum hull per five seconds, 100% of base hazard damage, and 0% base/type
resistance. This supersedes the earlier assumption that the cache's internal
Magnetic Field name establishes its displayed name or exact behavior.

The actual level-52 battleship candidate `2176963853` has weapon axes
`268135/434655/255367`. With the audited ordinary-NPC pool, both Realta and
ECS Fortunate remain too close to their respective mitigation limits for a
robust source/placement comparison at its projectile damage. No replacement
fight against that battleship was requested. Different hull coefficients alone
do not guarantee an informative control.

The retained Exchange controls also support a broader, still bounded hit
selection. Five nondepleted critical incoming hits in the no-Tom control and ten
in the Tom control have zero extra isolytic/critical-mitigation channels. Each
fractional accounting interval
`mitigated / (hull + shield + mitigated + apex ± 2.01)` contains the independently
retained noncritical reference. Critical reconstruction must remain fractional;
rounding it to an integer does not preserve this agreement. The `2.01` bound
remains a conservative accounting diagnostic, not a recovered server numeric
specification. Two additional observation regressions preserve these facts;
both Exchange observation crates pass, six tests total.

The next scout asks specifically about a **level-54 interceptor**, not the
unavailable level-55 battleship. Cached interceptor `835134089` has actual weapon
axes `634183/121014/242027`, which provide a different defense-channel test.
Stella is a possible alternate player hull: its ordinary-Klingon native source
audit selects launch AllDefenses `23.29`, Plating `4.40`, Absorption `3.40`, Dodge
`3.80`, and CombatStart AllDefenses `13.00`. Its large Eclipse-specific flat
defense source is excluded against Klingons. A current crew/tech/component
receipt and officer-bonus check are required before treating those source pools
as a ship-specific prediction. No Stella or level-54 fight result was consumed
when designing this alternative; the Tom implementation remains unchanged.

### Replacing the rare-spawn patrol plan

The player then reported no interceptors in Aeryndor and noted that they are
rare spawns. No fight was requested there. The next candidate is ordinary
level-53 Jem'Hadar Battleship `4156015417`, with level 52 `2049106002` retained
as a separate, weaker alternative. Cached systems include Meygonn at warp 80;
all listed candidate systems have no enabled hazard, wave or surge. These remain
scout leads until live availability is confirmed.

The level-53 hostile has no cached ship/component abilities. It has two energy
weapon entries with axes `803768/1298802/765496`, damage `931853..1138943`, and
two kinetic entries with axes `509053/822575/484814`, damage
`2164274..2645243`. Each fires three shots and warms up at round 1. The normalized
headline piercing vector is the average of these different weapons,
`656410.5/1060688.5/625155`. Both per-weapon and aggregate interpretations are
retained in the frozen predictions; earlier equal-axis controls did not
independently establish which layer the game uses.

The new native source audit checks all 58 active CombatStart defense rows,
their 49 native research-project descriptions, and all active modifier 57/59
officer-stat sources. It selects the same Realta launch pools and CombatStart
pool `13.00`. No active Dominion-specific defense source applies. The active
officer-stat conditions also preserve the earlier Jadzia/Tom source
assumptions. This bounds source selection; it does not assert that every
unbound server predicate is false.

For the stronger energy weapon, the six retained source/placement families now
span roughly `70.329%..71.196%` mitigation. Their full retained source/phase
bands can separate at approximately 184k actual reconstructed raw damage.
A no-Tom baseline can independently distinguish per-weapon piercing from the
aggregate: the corresponding predictions are approximately `0.16170473`
and `0.16172373`. A local diagnostic confirms the compiled no-Tom counters
`125/963/42`, pool `13`, first-weapon mitigation `0.161705`. No new in-game
result was used to choose these predictions. The Realta can be retained, with
the same Jadzia captain, empty bridge and Ablative Armor; a paired control
would remove/add only Tom below decks.

The player has now confirmed an ordinary red level-53 Jem'Hadar battleship in
Meygonn. The requested first fight is the fully repaired no-Tom baseline; the
prediction matrix, source audit and preflight profile receipt were frozen before
requesting that fight. Its observed result is recorded below.

An additional audit of the older Exchange captures finds that all 50 no-Tom
and 142 Tom incoming records support the same bounded physical reconstruction,
including 66 nonlethal shield-depleted records and three terminal records. The
reported damage assignments can therefore exceed the health actually removed
in those captures; they must not be treated as literal health deltas. Two
separate regressions preserve this distinction, bringing the two observation
crates to eight passing tests. This is capture-specific channel evidence, not a
general server rule. New comparisons keep intact-health hits separate from
assigned damage after depletion or death, verify the extra channels, and reject
inconsistent intervals rather than forcing them into a selected formula.

### Observed Jem'Hadar no-Tom baseline and native offset

The automatic capture at `2026-10-24T02:54:29` passed all 17 independent
setup-only checks. Jadzia level 5/rank 1 was captain, Tom was absent, and the
Realta tier 4/level 20, components, Ablative Armor, full starting
`652159/748507` health, and selected native buffs matched the earlier
baseline. Despite the six-hour gap, selected global and deployed source rows,
forbidden-tech rows, and all active buff ranks were unchanged. The target's
positive component sequence, level 53, faction, and full starting health
matched the cached Jem'Hadar battleship. Source hashes and the redacted setup
flags are retained with the observation fixture.

The fight lasted one round. Its three incoming noncritical hits all came from
energy weapon component `2806302352`; no hit from the weaker kinetic component
was observed. Only the first hit retained both hull and shield. Its fractional
assigned-damage reconstruction, with the separately documented conservative
`±2.01` denominator budget, gives mitigation interval
`0.1617042808–0.1617052216`. The frozen per-weapon piercing prediction, using
`803768/1298802/765496` and source-derived combat defenses `125/963/42`, has
`±1` defense-point band `0.1617045550–0.1617048959` and overlaps the
observation. The frozen normalized-aggregate band
`0.1617235218–0.1617239392` does not. This distinguishes the observed weapon
from that aggregate hypothesis for this one clean hit; it does not establish
the piercing layer for every weapon or fight. A nearest-integer raw-damage
reconstruction is diagnostic only: the journal has no direct raw-roll field.

The second hit's assigned shield damage exceeds the preceding shield by
`192239`; the third, terminal hit's assigned hull damage exceeds remaining
hull by `99181`. Their damage components are retained in the redacted
three-hit fixture, while their depleted/terminal HP deltas are excluded from
the primary clean-hit inference. The new observation crate verifies all 17
setup flags, the source-derived defense pools, the clean-hit interval and
per-weapon/aggregate separation, and these two ledger exceptions. It does not
assert full-fight accuracy or resolve Tom's operation-5 source and placement.

An independent native PE/config audit then established that the mitigation
curve offset is the binary32 encoding of `1.1`, promoted to
`1.100000023841858` for calculation, rather than the nearby f64 decimal.
`src/combat/mitigation.rs` now uses `1.1_f32 as f64`, and combat behavior
version 9 invalidates older optimizer reuse. This narrow source correction
does not rewrite the pre-observation prediction bands, which were frozen with
decimal `1.1`; the observation test checks the current engine band and the
unchanged frozen bands separately. It does not prove bit-for-bit server
precision or a server raw-damage integer type.

Focused post-change checks passed: 19 mitigation unit tests, 10 fingerprint
unit tests, and 11 observation integration tests across the three redacted
controls. The broader combat check exposed two reference tests that still used
decimal `1.1`; their expected values were independently recalculated with the
native offset, preserving their `1e-12` tolerance. The synthetic engine golden
fixture changed only hull remaining, shield remaining, and total damage for the
four seeds of `hostile_dynamic_mitigation_morale` (12 numeric values, largest
absolute change `0.000038`). Its structure and all other scenarios were
unchanged; this synthetic fixture update does not alter recorded observations
or frozen predictions. All 96 combat tests, both golden-master tests, and the
three Jem'Hadar observation tests then passed. All-target Clippy with
`-D warnings` and the release binary build also passed.

The running server remained on its earlier version-8 build while the paired
Tom capture was pending; no Tom result is inferred here.

The subsequent receipt at literal journal time `2026-10-24T04:02:16` did not
pass the planned Tom setup gate: its fixed-position officer array places Tom
at bridge index 1, whereas both earlier validated below-decks controls retain
empty bridge indices 1–2 and Tom at index 3. The ship inventory carries no
crew-placement fields that could override this receipt. The eight raw profile
files were preserved separately as an unmatched setup, without decoding its
combat commands or interpreting damage. A corrected below-decks repeat was
requested; this receipt is not a result for the frozen Tom hypotheses.

### Corrected Tom/Jem'Hadar control: no frozen model wins

The corrected receipt at literal journal time `2026-10-24T07:38:32` places
Jadzia at captain index 0 and Tom at below-decks index 3, with both bridge
slots empty. All 20 setup checks pass: ranks, ship components, full starting
health, forbidden tech, selected and all active buff membership, and all seven
non-journal profile snapshot files match the paired no-Tom baseline. The first
validation attempt exposed a helper bug: its unsafe-number reviver also made
ordinary decimal officer stats strings, so adding two officers concatenated
text. A separate version-2 report corrects that parser and independently
derives the captured raw totals `501/440/773/1714`. The failed version-1 report
and original helper remain preserved. No combat events were read until the
corrected setup gate passed.

The one-round journal contains nine incoming records. Energy component
`2806302352` supplies three clean noncritical hits and one clean critical hit;
their independently reconstructed, fractional-raw diagnostic intervals
intersect at `0.7059304997–0.7059335299`. Every one of the six frozen
source/placement families is rejected, both with the primary officer-bonus
assumption and its fallback range. The nearest is total effective crew Defense
added before all percentage pools, whose unchanged upper bound
`0.7059221899` remains below the observed lower bound by `0.0000083098`.
The source-backed float32-offset correction is too small and in the wrong
direction to explain the gap. Increasing the fixed denominator uncertainty to
force a match would not be justified.

Kinetic component `519835226` has three assigned/depleted or terminal records
with conditional interval `0.7117594106–0.7117607641`. That overlaps the
nearest family, but this component was not directly measured in the no-Tom
baseline, and these are not intact-health hits. They cannot override the clean
energy rejection. The redacted fixture retains all nine records, the
fractional critical accounting, shield overassignment, terminal overassignment,
and two later assigned records after zero remaining hull. It does not equate
assigned damage with health removed or claim those records are sequential
server attacks on an already destroyed ship. All 14 tests across the four
observation crates and all-target Clippy with `-D warnings` pass.

A separately frozen **post-observation** hypothesis examines an omitted
source-precision combination. Earlier policies applied the same base-rounding
rule to both officers. The retained deployed Tom screen gives Defense `28573`;
Jadzia's fractional source calculation is `19.13 × 67.57 = 1292.6141`, or
`1293` if rounded only at the end. Their mixed sum `29866` predicts energy
`0.7059322217` and kinetic `0.7117601352`, inside the respective intervals.
Tom plus unrounded Jadzia also fits, so these observations do not determine
final rounding. This new candidate does not amend the frozen six-family
result, prove a native operation-5 consumer, or justify a mixed-rounding engine
rule. A no-fight deployed Officers-menu reading of both officers was requested;
Jadzia candidates `1284` versus `1293` make that a useful independent check.
Production Tom source/stacking behavior remains unresolved.

A subsequent direct native-cache audit finds a shared CoreStats construction:
Tom's float32 weight/growth product rounds to `421`, and Jadzia's rounds to
`19`. It therefore provides no officer-specific precision exception supporting
the new mixed candidate. This binds the retained client CoreStats path only;
the deployed UI getter and server combat consumer remain separate questions.
See [the native source details](STFC_CLIENT_REVERSE_ENGINEERING.md#retained-native-officer-core-stat-source).

After the corrected fight and its source files were frozen, the previously
tested global mitigation-offset correction was activated in the local server
(behavior version 9). Health verification succeeded at
`2026-10-24T07:54:02Z`; the imported journal hash was unchanged across the
restart. This deployment does not change Tom's source or operation-5 placement.

### Native integer officer inputs: compiler correction

The production officer-model loader now consumes the native integer CoreStats
rows at the upstream source boundary. The retained cache provides 302 officer
specifications and 100 growth levels; 290 officers are bound to the reviewed
upstream files. The native float32 multiplication and rounding sequence is
implemented directly, rather than applying generic rounding to decimal stats.
For the controlled crew, the resulting attack/Defense/health totals are
`501/440/773`, exactly matching the captured raw crew totals. Tom's individual
base row is `252/421/505`; Jadzia's is `249/19/268`.

This correction applies only when the upstream file's exact bytes match one of
its reviewed LF or CRLF hashes. A refreshed or customized file, unknown officer,
or unavailable level retains the existing source behavior. Hand-authored LCARS
stats retain their specified fractional values. The extractor rejects a changed
static-cache digest, and the catalog explicitly identifies its build-272 static
rows separately from algorithm verification in build 273. This does not claim
that every cached row has been refreshed from the live game.

Five native unit tests, four loader/resolver integration tests, and five Node
extractor tests pass. The integration tests verify native rows, raw crew totals,
the existing rank-4 Tom coefficient, exact source-byte guards, and fractional
manual-data fallback. Checking the current Tom coefficient against the native
base input is a wiring regression, not validation of his combat source or
stacking rule. Frozen observed intervals and prediction files are unchanged.

After regenerating the ignored LCARS debug artifact, the full Rust suite reports
`1666 passed, 1 failed, 1 ignored` across 124 test targets. The only failure is
`gorn_evisc_log_vs_sim_calibration`: earlier preserved runs already fail its
damage/isolytic calibration bands at the same scale. Those bands were not
relaxed. Formatting and all-target Clippy with `-D warnings` pass. The dependency
audit remains unsuccessful because existing `zip` 0.6.6 and 8.6.0 dependencies
trigger three advisories each; none of the newly added SHA-256 dependency
packages appears in those audit findings. Full CI is therefore not green.

Combat behavior version 10 and the embedded catalog's optimizer fingerprint
invalidate persisted results compiled from the former fractional officer inputs.
Tom's effective Defense source, operation-5 aggregation and stacking remain
unvalidated; no mixed per-officer precision policy or displayed-stat override
was added.

The subsequent deployed Officers-menu screenshots show current Defense
`1284` for Jadzia and `28573` for Tom, excluding green next-level previews.
These match `19 × 67.57 → 1284` and `421 × 67.87 → 28573`, with displayed
sum `29857`. The source images and manual transcription are frozen together.
They reject the post-observation candidate's claim that displayed Jadzia Defense
could be `1293`; a distinct server getter is still unbound. Using the actual
displayed sum in the pre-all-percentage candidate predicts energy mitigation
about `0.705922078`, below the clean observed lower bound `0.705930500`.
The six original frozen families remain rejected.

An owned level-1/rank-1 Mirror Troi is the next proposed captain control. Her
integer base Defense removes Jadzia's fractional upstream growth ambiguity;
her native declared abilities are PvP-gated and inactive against the ordinary
Jem'Hadar battleship. A no-fight deployed Officers-menu reading was requested
first to check the assumed captain Defense `2027`. Conditional on that input,
pre-all-percentage individual-Tom and whole-crew hypotheses predict about
`0.704288574` and `0.706705641`, respectively, separated by `0.002417` and
both below the mitigation cap. This is preparation for a future control, not
an observed result or proof of the ability's source/stacking.

The release build containing the native integer officer inputs passed and was
activated locally after validation. Behavior-version-10 runtime SHA-256 is
`13582a04009007897d8fa38cc5cd65651f2a4f8b7b8b01565d72c7be736c2eb6`.
The server health check at `2026-10-24T08:47:59Z` confirmed 290 officers and an
idle CPU job slot. The imported journal SHA-256 remained
`4dee4ed472ec5c8345a2f1c355d22d32f36c05b44b94f6b6241258a8c6cb0130`
across this restart. This changes the compiler's base officer inputs, not the
unresolved Tom effective-stat selection or stacking rule.
