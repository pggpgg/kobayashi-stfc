# STFC client investigation with REA

This investigation supports three goals: account for the player's combat buffs,
reproduce recorded fights, and discover crews that perform well in the game.
Client inspection supplies evidence for the first two goals. A client method or
configuration value alone does not establish the server's combat behavior.

## Source identity

The first inspection was performed on 2026-10-20 with
[REA](https://github.com/morluto/rea) source commit
`7d6f90485ee078834ce64656770a34947bb81803`. The published npm package `6.7.0`
did not expose `inspect-il2cpp-metadata`; that command required the source build.
The source checkout also identifies itself as `6.7.0`, so record its commit,
not only its package version.

The installed game's `.version` marker changed from `&game=261` to `&game=272`
during the investigation. The retained REA evidence describes the latter:

| Artifact | SHA-256 |
| --- | --- |
| `global-metadata.dat` | `ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04` |
| `GameAssembly.dll` | `a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274` |

Local copies under
`local-analysis/stfc-client/2026-10-20/client-snapshot/` have matching hashes.
Their provenance record checks the source before and after copying. These are
local analysis inputs, excluded from Git. Evidence was first collected under
`target/rea-analysis/` and then copied to this separate directory so that
`cargo clean` does not delete it. Original paths in evidence remain historical
source locations.

The local `netniV/stfc-mod` checkout was at
`f981b07b5e14131b8423c7136ebf9652ec439ff3` (2025-12-25), 190 commits behind
upstream `main` at `e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0` (2026-09-22).
Fetching the remote refs did not update that checkout or the installed mod.
The `BattleConfig`, `MitigationCapsSpec`, and `GlobalDamageReductionConfig`
protobuf fields and their entity-group IDs agree between those revisions.
The mod schema is a decoding aid; the selected game artifacts remain the evidence.

## What REA established

The selected metadata has version 31, 160 assembly images, 27,042 types, and
201,322 methods. REA linked 185,458 methods to native code addresses and
identified 15,864 zero method-pointer slots. A zero slot is not proof that a
generic method has no runtime implementation.

Unless stated otherwise, the evidence filenames below are relative to
`local-analysis/stfc-client/2026-10-20/`.
The complete local evidence is `metadata-linked-evidence.json`, record
`ev_9249c2d505dfc18fe750b6ed25b8dffdb533956ea5f7b15730a94147f536b335`.
The compact summary is `metadata-linked-summary.json`; `il2cpp-seeds.tsv`
contains the native addresses and names. Addresses apply only to the binary
hash above. Names and structurally matched addresses do not demonstrate execution.

Useful initial targets include:

| Client method | Address | Question |
| --- | --- | --- |
| `Digit.Prime.Research.ResearchManager.ApplyBuffsFromResearchProjects` | `0x180bf2070` | Does research use only the owned level's value or accumulate earlier levels? |
| `Digit.PrimeServer.Services.BuffService.ApplyBuffModifiersToValue` | `0x1817c2450` | How are modifier buckets combined? |
| `Digit.PrimeServer.Services.BuffService.GetBuffModifierValueMult` | `0x1817c2470` | Which operations contribute to each multiplier? |
| `Digit.PrimeServer.Services.BuffService.AddBuffDataToModifierValues` | `0x1817c2620` | How are buff values classified and accumulated? |
| `Digit.PrimeServer.Models.StaticFleetSpecs.CalculateMitigationInternal` | `0x181a2ef80` | How do the defense/piercing ratios and configuration values produce mitigation? |

The filtered inventory `combat-metadata-targets-v2.json`
retains 22 selected types and 638 methods, including native addresses and
parameter/return types. The metadata confirms `BattleConfig` has equation and
constant map fields and that `BuffService.BuffModifierValues` has distinct
`AdditionVal`, `MultVal`, and `ScalarVal` fields. Their names do not determine
the order of operations.

## Recovered native behavior

REA located the functions; a focused Ghidra 12.1.4 pass recovered their native
instructions and pseudocode from the frozen binary. The initial REA-managed
Ghidra pass spent over twelve minutes in broad analysis and was intentionally
interrupted and closed. The successful fallback used the same REA addresses
with Ghidra's default auto-analysis disabled, a four-CPU limit, and a
60-second decompilation limit per function. Its first five functions completed
in 33 seconds. Additional queries reused that project.

This distinction matters: the native results are supplemental Ghidra evidence
guided by REA, rather than successful output from REA's native analysis call.
Pseudocode types and indirect calls need interpretation. The evidence includes
instructions, decoded constants, logs, tool versions, and the binary hash.

The final `focused-native/analysis-report.json` records five findings and
hashes for 30 instruction/pseudocode files covering 15 functions.
`focused-native/results/` contains those files;
`focused-native/decoded-constants.json` identifies constant bytes and offsets.
All 30 files and five pass logs were hash-verified after preservation.
`focused-native/cleanup.json` records the unchanged input and no remaining
owned analysis processes. `native-session/` retains the interrupted REA
attempt, including its close receipt. The reusable Ghidra project remains
in build scratch under `target/rea-analysis/focused-native/project/`; its
location in the manifest is historical and it is not part of the durable copy.
The portable toolchain manifest is preserved at this evidence directory's root.

### Buff arithmetic

`BuffService.ApplyBuffModifiersToValue` at `0x1817c2450` computes:

```text
(input + AdditionVal) * (1 + MultVal)
```

The addition occurs before multiplication. This method does not read
`ScalarVal`. In `AddBuffDataToModifierValues` at `0x1817c2620`, operation codes
1/2 add/subtract the selected buff value in `AdditionVal`, 3/4 in `MultVal`,
and 7/8 in `ScalarVal`. This establishes these methods' bucket arithmetic,
not the entire buff system: eligibility, value selection, alternate consumers,
and the role of the scalar bucket still need tracing.

### Mitigation curve

`StaticFleetSpecs.CalculateMitigationInternal` at `0x181a2ef80` contains:

```text
cap / (1 + 2^(-2 * (ratio - offset)))
offset = float32(1.1) = 1.100000023841858
```

The constants were read from the hash-checked PE's `.rdata` section, not inferred
from the cache. With the same offset this is algebraically
`cap / (1 + 4^(offset - ratio))`, matching the curve shape in
`src/combat/mitigation.rs`. The unnamed native power helper was inspected;
identifying it as a power operation is an interpretation of its implementation,
not a recovered managed method name.

This is a client stat-calculation path. Its arguments include a fleet and an
opponent hull type, rather than a complete opponent combatant. The dodge
coefficient helper obtains a comparison stat through
`GetMarauderStat(level, stat, hull)`. The curve therefore corroborates a client
calculation without establishing exact fight damage against arbitrary hostile
stats. Nor does it establish bit-for-bit equivalence: the client path uses
float operations, and the ratio helpers, cap selection, zero handling, and
final aggregation also affect the result.

### Ranked values

`BuffData.GetValueAtRank` at `0x181980c30` selects entry `rank - 1`; it does
not sum earlier ranks. It returns zero for ordinary ranks below one and
uses the last available entry when the rank exceeds a nonempty list.
An empty list does not provide a valid last entry. Physically, this method
delegates to `BuffSpec.GetValueAtRank` at `0x181b75bb0` through a tail jump;
Ghidra's pseudocode includes the delegated body.

The research setter provides a more specific connection:

```text
ResearchProject.SetResearchLevelIndex(index)  [0x181b24120]
  -> each project buff: UpdateRank(index + 1) [0x181980d00]
  -> _lookupRank = newRank - 1
  -> BuffData.get_Value()                    [0x1819804a0]
  -> one rankedBuffValues[_lookupRank] entry
```

The native call's `index + 1` is visible in the instruction listing even
though Ghidra's inferred call signature omits that argument. This is direct
evidence for selecting one entry in this client research path. The server
research-level map to this internal index has not been traced natively.
Hand-authored delta records and imported ranked snapshots must not be
conflated. The implementation follow-up below records Kobayashi's explicit
catalog semantics and the verified sync-ingress behavior separately.

`ResearchManager.ApplyBuffsFromResearchProjects` follows a
dependent-research mapping and adds one current-upgrade buff value per listed
project. That sum across projects is distinct from summing all earlier ranks
within a project; it does not establish the complete combat aggregation path.

## Cached configuration

A local `ServerRequestCache` response contains static combat groups:

| Group | Message |
| --- | --- |
| 38 | `StaticSyncBattleConfResponse` |
| 52 | `StaticSyncMitigationCapsSpecResponse` |
| 53 | `StaticSyncGlobalDamageReductionConfigResponse` |

The selected cache's SHA-256 is
`6cc5375f6cfd59f382ed67c88f5f7e629b933ce9c32aa99f68c431b78ea32f37`.
Its relationship to game build 272 and the current server configuration is
unverified: the response encodes neither `serverVersion` nor `serverTime`.
Preserve its identity separately from the client binary.

The cached `BattleConfig` contains timing and other scalar configuration, but
its `equations` and `static` maps have no entries. The schema's map declarations
therefore do not establish that combat equations are available in this cache.
The scalar `combatLengthSeconds` must not be interpreted as a round limit or
weapon schedule without tracing its use.

The decoded configuration also contains:

- Mitigation caps named `dodge`, `plating`, and `absorption`. The primary
  coefficient is approximately `0.55`, with `0.20` secondary coefficients for
  destroyer/explorer/battleship keys. Survey, defense, and armada-target keys
  use approximately `0.30` in each channel. These names come from the pinned
  mod's `HullType` enum; values and integer keys come from the cache.
- Separate officer-bonus multipliers: primary `1`, secondary `0`, and `0.30`
  for survey/defense/armada-target keys. Trace their use rather than assuming
  they share the mitigation-cap formula.
- `damageReductionIncrease = 2` and
  `damageReductionOffset = 1.100000023841858`. `damageReductionMin` is not
  encoded; the schema's ordinary protobuf default is zero. An absent field
  must not be reported as an observed encoded zero.

The retained extraction is `combat-config.e0b8552-envelope.json`.
Each selected group includes its source byte ranges and content hash. The
mitigation-cap message occupies `[5118349, 5118707)` and the global reduction
message `[18531108, 18531118)` in the selected cache.

To inspect another explicit cache with the same known schema:

```powershell
node scripts/inspect_stfc_combat_config.mjs 'C:\path\to\cache.bin' `
  --output target/rea-analysis/combat-config-new.json
```

This helper reads only the three static combat groups, omits other payloads,
does not evaluate equation strings, and refuses to replace existing outputs.
It does not fetch data or communicate with the game. Run its regression checks
with `npm run test:scripts`.

## Concrete research discrepancy at initial inspection

Research `2999437` (**4-star Direct Hit**), buff `1994654654`, is a useful
end-to-end test case. Its cached double rank values exactly match the
repository's upstream research file:
`0.21, 0.24, 0.27, 0.33, 0.39, 0.45, 0.53, 0.61, 0.69, 0.80`.
At initial inspection, the generated catalog copied those ten values unchanged.

The upstream description (translation `37022`) specifies three base stats:
armor piercing, shield penetration, and accuracy, restricted to ships of
grade 4 or higher. The generated catalog then contained **only accuracy
with no grade condition**. This was a concrete omission relative to the
project's own source data, independently of the rank-aggregation discrepancy.
Cached modifier code `74` also matches `MODALLPIERCING` in the
pinned mod enum; the actual client consumer still needs tracing.

The correction requirements identified at that stage were:

- Preserve ship grade through normalization and resolution, then evaluate the
  minimum-grade condition against the selected ship.
- Apply three distinct base-stat multipliers before mitigation. The existing
  generic `pierce` path adds to damage-through and is not interchangeable with
  armor-piercing or shield-piercing stat multiplication.
- Apply explicit ranked-snapshot semantics to imported values after connecting
  the profile's owned level to the client index. At rank 2, selecting the entry
  gives `0.24`, whereas summing gives `0.45`; at rank 10, `0.80` versus `4.52`.
  The recovered client setter/accessor path selects one entry.
- Check grade 3, 4, 5, and unknown-grade ships, and both aggregate and
  per-weapon stat paths.

Local evidence and source references are retained in
`research-2999437-static.json` and
`research-2999437-comparison.json`. Those files predate the native trace above;
their rank hypotheses describe the earlier investigation stage. Those
artifacts predate the rank, stat-routing, and grade-condition implementation
recorded below and remain unchanged as historical evidence.

## Questions that affect Kobayashi

1. **Research level semantics.** Imported snapshot and authored increment
   semantics are now explicit in Kobayashi, as recorded below. The recovered
   client setter/accessor selects one ranked entry. The remaining native
   question is how the server's research-level map supplies the internal
   zero-based project index.
2. **Buff compilation.** Resolve operation semantics, activation conditions,
   recipient/target scope, and the sources that feed effective ship statistics.
   In Kobayashi, persistence of synced global buffs is not yet connected to
   scenario compilation, and syndicate calculations are only exposed through
   a separate utility.
3. **Weapon timing and randomness.** The client schema exposes warmup, cooldown,
   minimum/maximum damage, shot count, and critical parameters. Establish their
   actual use before treating an average-damage weapon as equivalent.
4. **Combat authority.** Distinguish stat calculation, report formatting,
   presentation, and a complete fight simulation. Finding combat-related names
   does not prove that the authoritative fight loop ships in the client.
5. **Validation against the game.** Confirm recovered rules against fights with
   matching player inputs. The existing calibration scoreboard contains
   synthetic checks and no snapshot-bound recorded fights; passing it does not
   establish game accuracy.

For each accepted rule, retain the artifact hash, method or configuration
location, observed behavior, interpretation, unresolved alternatives, and a
focused regression test. Validate on separate recorded fights before using the
rule to claim better crew recommendations. Monte Carlo confidence intervals
measure sampling uncertainty within the simulator, not model accuracy.

## Reproduce metadata inspection

Use the pinned REA source build and its
[IL2CPP workflow](https://github.com/morluto/rea/blob/7d6f90485ee078834ce64656770a34947bb81803/docs/il2cpp-metadata.md).
On Windows, its documented source-build recovery can restore the matching
published package's native addon before `npm ci` and `npm run build:cached`.
No MCP registration is needed for the CLI.

From the Kobayashi root, with the local tools and snapshot already prepared:

```powershell
node target/rea-source/scripts/rea.mjs inspect-il2cpp-metadata `
  local-analysis/stfc-client/2026-10-20/client-snapshot/global-metadata.dat `
  --binary local-analysis/stfc-client/2026-10-20/client-snapshot/GameAssembly.dll `
  --detail summary `
  --evidence_output target/rea-analysis/metadata-recheck-evidence.json `
  --ghidra-seeds target/rea-analysis/metadata-recheck-seeds.tsv `
  --json
```

Choose unused output names: REA refuses to overwrite evidence. Preserve full
evidence locally and query selected types/methods rather than repeatedly
decoding the game. Deep native inspection additionally requires a supported
Ghidra and full JDK. A Ghidra import can be expensive even for one requested
function; retain the session for related queries.

Portable tools and build scratch stay under ignored `target/`. Selected client
binaries, full metadata evidence, extracted static configuration, and findings
are preserved under ignored `local-analysis/stfc-client/`. Whole server caches
are not copied. The committed material is the inspection tooling and this
finding ledger.

## Implementation follow-up — 2026-10-20

The recovered observations above motivated a bounded correction to research
compilation. They do not establish a complete client combat simulator or
validation against historical fights.

- `ResearchRecord.level_value_mode` now distinguishes imported `"snapshot"`
  values from authored `"cumulative"` increments. All catalog merge paths
  share rank selection. Level zero contributes nothing; above-maximum values
  cap at the last catalog rank; empty or missing snapshot ranks contribute
  nothing. Canonical effects retain their independent `snapshot_by_level`
  contract and priority over catalog contributions.
- A follow-up audit matched all eleven bundled canonical arrays to their
  referenced upstream ranks: seven directly, four with their existing
  `0.01` unit conversion. At that stage they were marked `snapshot_by_level: true`;
  absent/false remains cumulative for authored overrides. Burning Isolytic
  research `2580836593` therefore selects `0.06` at rank 2 instead of `0.11`.
  The audit also found five source-description mismatches in existing
  canonical modifiers or conditions. The subsequent tracing and corrections
  are recorded below; the exact historical RID/buff/loca evidence remains in
  [the canonical source audit](research_conditional_routing.md#canonical-source-audit-2026-10-20).
- The mod's pinned `ResearchTreesState.researchprojectlevels()` producer sends
  each server level unchanged. Kobayashi ingress stores it unchanged; profile
  compilation retains positive levels, clamps to `u32`, and uses the maximum
  when duplicate IDs are supplied. There is no plus/minus-one conversion in
  that observed path. The importer names `buff.values[0]` as catalog level 1.
  The remaining native server-map-to-project-index connection is still
  unverified.
- Research `2999437` now explicitly maps buff `1994654654` to `accuracy`,
  `armor_piercing`, and `shield_piercing`, each with `attacker_min_grade: 4`.
  Rank 2 selects `0.24` per stat and rank 10 selects `0.80`. Ship grade is
  preserved through normalization and ship resolution. Known grade 4 and
  above matches; grade 3 and unknown grade do not.
- Scenario compilation applies these separate base-stat fractions before
  aggregate and per-weapon mitigation. They do not enter the generic
  damage-through `pierce` channel. Ordinary flat/faction merges and runtime
  seats exclude grade-gated rows. The new path supports only grade-only gates
  on the three listed stats; compound grade conditions remain unapplied.
  Scenario-aware research summaries follow the same restrictions.
- Catalog regeneration retained all 802 existing projects. Every generated
  record now declares snapshot semantics; 94 empty ranks across 38 projects
  are retained. Ignoring mode fields and empty ranks, the only changed
  project's bonuses are Direct Hit's three gated channels.

Regression coverage includes rank 0/1/2/maximum/above-maximum, missing and
empty ranks, legacy cumulative defaults, faction routing, canonical override
priority and modes, grade 3/4/5/unknown, and aggregate/per-weapon piercing.
Importer tests compare Direct Hit against the checked-in upstream entry.
These checks establish the implemented behavior, not in-game accuracy.
See [research_conditional_routing.md](research_conditional_routing.md) for the
current routing contract and [RESEARCH_COMBAT_VALIDATION.md](RESEARCH_COMBAT_VALIDATION.md)
for limits of the retained combat logs.

An independent importer review also exposed and corrected an existing edge
case: the shortest buff array, including unmapped buffs, previously limited
the project's emitted ranks. The importer now uses the declared project
maximum (or project level count) and clamps each nonempty buff array at its
own final entry, matching the recovered ranked lookup. Empty lists
contribute nothing and explicit trailing zeroes stay zero. When no project
bound exists, the longest available buff array supplies the rank count.
Mixed-length regression fixtures cover this behavior. None of the current
802 mapped projects was truncated by the old rule.

## Five research mappings verified against static client definitions

The later investigation decoded only the five discrepant projects and their
linked buff, target, and trigger specifications from the same cache hash.
The tracked selective fixture is
[client_verified_research_2026-10-20.json](../data/research/client_verified_research_2026-10-20.json).
It retains per-message offsets and hashes, raw ranked doubles, attributes,
condition codes, and source identities; no player-state payload is included.

| RID | Buff ID | Modifier | Operation | Trigger | Single status condition |
|---:|---:|---|---|---|---|
| 2047743532 | 679782642 | 73 / `ModAllDefenses` | 3 / `MultiplyAdd` | 18 / round start | 38 / target Burning |
| 535909811 | 1142346453 | 73 / `ModAllDefenses` | 3 / `MultiplyAdd` | 18 / round start | 33 / self Morale |
| 1233598019 | 4033066544 | 73 / `ModAllDefenses` | 3 / `MultiplyAdd` | 18 / round start | 37 / target Hull Breach |
| 3288570685 | 2286920377 | 16 / `ModShieldHpRepair` | 3 / `MultiplyAdd` | 18 / round start | 33 / self Morale |
| 851540444 | 3573127943 | 67 / `ModCargoProtection` | 1 / `Add` | 24 / self launch | Codes 1 and 0 retained without further interpretation |

All five target code `1`, whose static target specification names
`targ_self_ship`. Trigger `18` names `trig_round_start` with battle schema
`1`; trigger `24` names `trig_self_launch` with deployment schema `2`.
The retained game's `BuffCondition..cctor` at `0x181b71610` assigns the
Morale, target Hull Breach, and target Burning fields to `33`, `37`, and `38`.
These checks establish more than the translated descriptions alone.

Native `StatDataUtilities.GetStatTypes` at `0x181e9b9e0` maps modifier
`73` to static `StatType` fields Dodge, Plating, and Absorption (offsets
`0x2c`, `0x24`, and `0x28`); modifier `67` maps to CargoProtected
(`0x4c`). Metadata type `10855` identifies those fields.
`StatType..cctor` at `0x181e98e50` writes those offsets through the same
class global (`0x185cf49d8`), independently linking them to their numeric
stat IDs; the selected evidence fixture records its hash and assignments.
`ApplySingleBuffToStatContainer` at `0x1817fe0c0` adds an unchanged
ranked double to the ADD bucket for operation `1` and to the MULT bucket
for operation `3`, with no division by 100. The previously recovered
`ApplyBuffModifiersToValue` at `0x1817c2450` applies
`(input + AdditionVal) * (1 + MultVal)`. These addresses identify the
retained binary hash above; the fixture records the selected decompilation
hashes.

The three defense buffs contain raw double rank values
`0.5, 0.75, 1.15, 1.6, 2.05, 2.6, 3.15, 3.75, 4.35, 5`.
Their previous canonical arrays divided these values by 100 and assigned
unrelated damage effects. The corrected canonical and generated fallback
rows use `base_mitigation_stats_bonus`, preserve those raw fractional values,
and apply the single recovered status gate at round start. The repair buff
uses `officer_shield_regen_max_fraction`, preserving its `0.01` through `0.1`
rank values and Morale gate. The deployment-only cargo buff now has an empty
combat override and an explicit importer exclusion.

Incoming mitigation now uses the raw-stat curve whenever nonzero raw defense
context exists, including rounds with no qualifying research bonus. Status
activation changes the base-stat multipliers within that same calculation.
Missing or all-zero raw defense context retains the legacy fallback.

The maximum-shield repair denominator is the simulator's existing channel
choice. Modifier `16` is absent from the inspected `GetStatTypes` dispatch;
these traces establish the source's repair modifier, values, status, and
round-start trigger, but do not establish the authoritative server's repair
denominator or complete timing order.

The importer uses an explicit `value_unit: "fraction"` for these four verified
combat mappings, bypassing its older magnitude-based heuristics. The catalog
now has 806 projects: the four verified combat rows are newly represented.
The prior 802 projects are unchanged by this follow-up. Ten canonical combat
effects retain snapshot selection; the eleventh entry is the noncombat
exclusion. Tests compare canonical and fallback compilation, guard rank and
gate behavior, and ensure a stale catalog cannot resurrect the excluded buff.
The earlier cache freshness and server-authority limitations still apply.

### Fresh static-cache comparison

A second cache was frozen after login at `2026-10-20T18:04:30.399Z`, with
client build `272` observed in that capture. Its size is `24,991,479` bytes
and SHA-256 is
`b7fcb7a021fa982144702a682e81eb6b7a31406522b0154cc77553a2c0776418`.
Selective extraction found byte-identical payloads for all five research
projects, their five complete linked buff definitions, and all three selected
target/trigger specifications. The tracked fixture retains the original
provenance and adds the fresh offsets, fragment hashes, capture time, and
comparison result.

The fresh envelope still has no `serverTime` or `serverVersion`. These
observations strengthen the source's relevance to the current local capture;
they do not establish an encoded cache-to-server/build binding or remove the
shield-repair denominator limitation.

## Mod synchronization gaps exposed by the controlled fight

The October 20 Realta report was recovered by manual export after no automatic
journal reached Kobayashi. The latest inspected mod source, commit
`e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0`, dispatches protobuf groups 219
(`BattleResultHeaders`) and 221 (`BattleReport`) to placeholder functions in
`mods/src/patches/parts/sync.cc`. It has no dispatcher/schema support for the
current client's group 251 (`Ships`). The older local source checkout was left
unchanged; the newer commit was inspected directly from Git.

The retained client's native registry dispatch at `0x1817008f0` has concrete
parse branches for all three group numbers. Metadata links the battle service's
protobuf registration to `OnBattleResultHeadersMessage` and
`OnBattleReportMessage`. Its JSON registration also remains present, so these
findings alone do not identify the actual format delivered during this fight.

The minimal header wire shape is independently recoverable:

```protobuf
message JournalHeaderProto {
  int64 id = 1;
}
message BattleResultHeadersResponse {
  repeated JournalHeaderProto headers = 1;
}
```

This intentionally omits unrelated fields. `BattleResultHeadersResponse.MergeFrom`
at `0x18196d9a0` handles tag 10, the repeated message field 1.
`JournalHeaderProto.MergeFrom` at `0x18196abf0` handles tag 8 and calls the
metadata-linked `CodedInputStream.ReadRawVarint64` at `0x1833d31f0`; it stores
the signed 64-bit ID without zigzag decoding. Metadata field-number constants
agree. The implemented compatibility fix decodes header IDs without guessing
the wire type and connects JSON and protobuf receipts to the same retry queue.
Further metadata/native verification established `JournalResponse` field 1 as
an ordinary `int64`, and `PlayerShipsResponse` field 1 as
`map<int64, PlayerShipData>`. The selected ship fields are id, tier, hull ID,
signed component IDs, level and float32 level progress; manual packed and
unpacked wire fixtures test exact IDs above 2^53 and malformed-message
atomicity. Group 221 deliberately extracts only its ID and obtains the full
journal through the existing authenticated `/journals/get` JSON endpoint.
That endpoint returned complete historical reports during the live 219 check
below; the 221 receipt route itself has not yet been observed.

Private native evidence is retained under `mod-protobuf-native/`. A bounded
inspection of saved response caches found none of these three groups and no
response saved at or after the fight. The candidate's message-type-only live
instrumentation can distinguish the active path without collecting payloads
or credentials.
The candidate also retains failed fetches/deliveries and records successful
history only after enabled destinations return HTTP 2xx. Pending work remains
memory-only and needs rediscovery after a game exit.

The release build and all three native regression executables passed. Its
source is base commit `e0b8552` plus patch SHA-256
`fd36339ffd54f4da49bbe15b7d1f56b2af6dccc8a7af65d6968b389242fc42ae`, preserved
in the local mod candidate package. On October 23, with the game verified
closed, candidate DLL SHA-256
`a1ce8edc5e4dad6eaadc8a401e2a6fdba3124d6471269328f979fd7ef2f10328`
was installed. The previous official DLL remains available for rollback;
settings, successful journal history and `version999.dll` hashes are unchanged.
The local [installation proof](../local-analysis/stfc-client/2026-10-23/journal-capture-installation/installation-proof.json)
and [rollback-capable package installer](<C:/dev/stfc-mod/journal-capture-artifacts/2026-10-22/candidate-v1/Install-Candidate.ps1>)
record the result. The local proof directory contains source records without
the DLLs; installation and rollback use the complete package. The installer
was corrected to pass an explicit .NET null
string to atomic `File.Replace`; a local replacement regression passes.

The subsequent launch auto-updated the game to build 273. The candidate's loaded
module path/hash and unique startup marker were verified, and its 219/251 hooks
received live messages. Two historical journals were fetched and acknowledged by
Kobayashi; one exactly matches the October 20 Realta export, including initial
and final hull/shield values. A relaunch with the local server ready refreshed
58 ships; the Realta's tier, level and all 14 component IDs match the historical
journal. The complete native command stream now provides separate officer
firing/application markers and fractional mitigation channels.

Build-272 native evidence remains bound to that historical fight. Build 273's
binary, metadata and version marker are frozen separately under the ignored
`target/client-snapshot-273/` with stable source/copy hashes. Local
[receipt proof](../local-analysis/stfc-client/2026-10-20/fresh-capture/after-custom-mod-relogin-2026-10-23/live-receipt-proof.json)
records the historical result. A new controlled Realta fight then arrived
automatically through 219 at literal journal time `2026-10-23T05:43:52`, with
two rounds, seven attacks and exact final-health accounting including `28.64`
round-two hull repair. Its immutable source and
[fresh-fight receipt proof](../local-analysis/stfc-client/2026-10-20/fresh-capture/post-fight-custom-mod-2026-10-23-01/fresh-fight-receipt-proof.json)
verify the endpoint and Kobayashi delivery for a new fight. The unobserved 221
route remains a separate check; the other enabled destination is unacknowledged.
Ordinary state upload lacks the journal queue's retry behavior, so an initial
login before the server became ready required the relaunch. See
[the fresh combat findings](RESEARCH_COMBAT_VALIDATION.md#fresh-realta-fight-observed-result-and-remaining-errors).

## Retained native officer core-stat source

The static-cache and IL2CPP schema audit binds group 7 to `OfficerSpecs` and
group 18 to `OfficerCoreStatSpecs`. OfficerSpec fields 15, 16 and 17 are
integer Attack, Defense and Health weights; field 11 selects rarity.
`OfficerService.CreateCoreStats` at `0x181884360` selects the level's
float32 growth value by rarity, multiplies each integer weight in float32,
and stores the rounded result in an integer CoreStats array. Its positive
rounding helper at `0x1816f7300` uses `floor(float32(value + 0.5))`.

The two retained cache copies agree on the selected rows. Tom level 20 uses
Defense weight 10 and growth `42.060001373291016`, yielding float32 product
`420.6000061035156`, then integer `421`. Jadzia level 5 uses weight 1 and
growth `19.1299991607666`, then integer `19`. The upstream values `420.6`
and `19.13` describe pre-round products. This client path applies the same
rounding pipeline to both officers and supplies no native justification for
the post-observation mixed-precision candidate.

The private source proof
`native-officer-core-stat-source-proof-v1.json` under
`local-analysis/stfc-client/2026-10-23/tom-paris-effective-stat-native/`
retains group and field constants, PE getter offsets, cache row byte ranges,
source hashes and the reproducible extraction. Its SHA-256 is
`bf08caaa05f618a667e90a8dd685c85b1bf66701bbc7dfc7bce3da170a2b2157`.
This establishes the retained client's CoreStats construction, not the value
selected by an alternate UI getter or the server's operation-5 combat consumer.
Build-272 metadata and the retained cache are identified explicitly.

A separate build-273 inspection confirms the same construction in
`BaseOfficerService.CreateCoreStats` at `0x1818848c0`, using the same integer
field offsets and float32 multiplication. Its rounding helper is now at
`0x1816f7870`, with exact float32 `0.5` at `0x1855bd554`. The inspected
GameAssembly SHA-256 is
`a9ab2edbc143cfba3aff35665e129006f73853070dbff206884fda04a235ab94`,
and metadata SHA-256 is
`2b586d6f23eb130c210ddca13d8ebdd21f09f807e2eb94f4972bf321b5f821da`.
This verifies the newer client's algorithm, not a newly captured set of static
data rows or the server combat consumer. The individual officer UI trace finds
widgets binding a precomputed `OfficerStatDataContext.StatNumber`; its producer
and the server's officer-stat aggregation remain unresolved.

The subsequent deployed main Officers-menu screenshots show Jadzia Defense
`1284` and Tom Defense `28573`, with green upgrade previews excluded. These
match the native integer bases with the selected captain and below-decks
officer-stat factors (`19 × 67.57` and `421 × 67.87`, rounded for display).
The screenshots reject the separate post-observation `1293` Jadzia candidate
as an explanation of her displayed stat, but do not bind the server combat
getter. Kobayashi now compiles integer CoreStats only for exact hash-bound
upstream officer files; refreshed/custom files and hand-authored LCARS retain
their source behavior. See [the validation and freshness limits](RESEARCH_COMBAT_VALIDATION.md#native-integer-officer-inputs-compiler-correction).
