# Native global account and consumable buffs

`src/data/native_consumable_buffs.rs` compiles a reviewed subset of synced global
buffs from static native group 116, `ConsumableBuffs`. This group contains permanent
account bonuses as well as timed boosts; its name does not imply a temporary item.

The source catalog is `data/native_consumable_buff_catalog.json`. Its 285 selected
BuffSpecs cover HP, weapon damage, critical, isolytic and launch defense stats. Of these, 114 support the
current deterministic setup contract: 28 hull, 31 shield, 22 weapon, five
additive critical-damage, 12 isolytic-damage, ten isolytic-defense and six
all-defense definitions.
The remaining 171 are quarantined by the setup compiler: 167 have
unsupported conditions, three have unsupported targets, and one has a chance
that is not always one. Research (66), building (68) and forbidden-tech (141)
IDs are explicitly excluded during extraction so they cannot enter both their
existing profile path and this global-buff path.

The CombatStart compiler also accepts 21 offensive definitions with the exact
NPC/non-Armada predicates 27/131: nine modifier-2 MULT weapon bonuses and twelve
modifier-10 ADD critical bonuses. These remain outside the 114 setup definitions.
Only active, nonexpired imported IDs contribute, at their current ranks. The
weapon additions share the base percentage pool. Critical additions enter the
base critical factor before timed critical multipliers. They are unboostable
ship seats and do not receive officer-only Assimilation scaling. Armada and PvP
contexts exclude them. This extends the reviewed predicate/compiler coverage;
it does not prove that all 21 definitions are active in a particular account.

The setup compiler accepts only self-targeting scalar buffs at launch or combat start,
without attributes or unknown fields, and with no predicate or one verified
ship-class predicate. Unknown grade, faction, ship, status and other conditions
remain excluded from that path. A separate CombatStart defense compiler handles
the specific typed gates described below. Native operation 1 adds flat HP or directly adds to critical
chance/multiplier; operation 3 enters the percentage pool. Raw double values are
preserved without display-based scaling. Current imported levels select one full
rank snapshot; explicit zero ranks stay zero. Duplicates count once at their
highest active level. Expiry timestamps are Unix seconds, as exported from
`Timestamp.seconds()` by the mod.

`bonuses_for_imported_buffs(imported, attacker_ship_class, now_unix_seconds)` returns
the additive profile-pool contributions. Its reviewed catalog is embedded in the
binary. `bonuses_from_catalog` provides an explicit-catalog variant. A catalog
refresh requires rebuilding the binary; the optimizer fingerprint includes the
catalog bytes and imported buff state.

Re-extract into a new file for review:

```powershell
node scripts/extract_stfc_consumable_buff_semantics.mjs <cache.bin> <new-output.json>
npm run test:scripts
```

The extraction reads only explicitly selected static BuffSpec maps, excludes
player state, records hashes and byte provenance, and creates its output
exclusively. The reviewed source is the frozen October 20 build 272 cache. The
client later updated to build 273; those two native metadata/address sets must
remain separate.

## Historical journal rank contract

The automatically captured historical Realta journal stores `ActiveBuff.Ranks`
as **zero-based indices**. They cannot be passed directly as imported buff levels.
The retained build 272 `FleetDeployedData.UpdateBuffs` (`0x1819f0510`) reads the
first active-buff rank, adds one, and calls `BuffData`'s constructor
(`0x181980ff0`). That constructor stores `currentRank - 1` in the internal rank
field; `get_Value` (`0x1819804a0`) indexes the raw rank array with that field.
Conversely, `GetValueAtRank` (`0x181980c30`) takes an external one-based level and
subtracts one itself. These are separate wire/API contracts.

For the October 20 Realta fight, the source-matched historical global buffs and
current imported levels both select these group-116 totals:

| Profile pool | Native contribution |
| --- | ---: |
| Hull HP percentage | 54.9 |
| Shield HP percentage | 59.9 |
| Weapon damage percentage | 2.6 |
| Critical multiplier addition | 0.69 |
| Isolytic baseline addition | 0 (active group-116 isolytic rows have unsupported predicates) |

They explain previously missing permanent player buffs. Group-116 membership
alone does not establish whether a particular future conditional buff applies;
the conservative runtime predicate checks still govern compilation.

The historical journal also permits independent source reconstruction of its
precombat stats, using native `(base + ADD) * (1 + MULT)` pools:

```text
hull   = (179.36 + 3400) × (1 + 54.9 + 75.15 + 49.15 + 1 FT + 1 officer)
       = 652159.392 → displayed 652159
shield = (212.38 + 110 ship bonus + 4000) × (1 + 59.9 + 65.15 + 45.12 + 1 FT + 1 officer)
       = 748506.5446 → displayed 748507
critical chance     = 0.1 + 0.12 = 0.22
critical multiplier = 1.5 + 1.45 research + 0.69 global = 3.64
```

Weapon damage similarly gives `88 × (1 + 18.05 research + 24.05 buildings + 2.6
global) = 4021.6`. That arithmetic alone cannot distinguish aggregate weapon base
88 from a base-44 weapon with a separate officer multiplier of two; the weapon
calculation path requires its own native proof. None of these equalities establishes
round-level officer timing or critical damage after dynamic combat effects.

## Native building families

`data/native_building_buff_catalog.json` separately selects 65 native
`StarbaseBuffs` group-68 definitions. The building compiler reuses the same raw
unit, expiry, rank and class rules. It additionally preserves the known account
predicates 203/204/205 as minimum Syndicate levels 30/33/35. Their code assignments
come from `BuffCondition..cctor` (`0x181b71610`), offsets `0x128/0x12c/0x130`,
linked to metadata fields 56035/56036/56037 named
`CondSelfLoyaltyLevel30/33/35`. An absent Syndicate level cannot satisfy them.
`BuffService.IsLoyaltyBuffConditionMet` (`0x1817c50d0`) independently verifies
signed greater-than-or-equal comparisons at 30/33/35; these are minimum thresholds,
not exact-level tests. Its retained decompilation SHA-256 is
`536536011adfad0201d5fd40d51d8237aa5a85f833aa784f4163aa8b88b4c378`.
Other account, grade, opponent, ship and status predicates remain quarantined.

This accounts for 40 supported definitions and 33 quarantined definitions: 30
with unsupported conditions and three with unsupported targets. The 40 supported
definitions comprise eight weapon, ten shield, eight hull, three additive
critical-damage, three isolytic-damage, one isolytic-defense, four all-defense
and one each of plating, absorption and dodge definitions. Their actual contribution still depends on the
specific imported buff IDs, current levels and applicable predicates.

The native modifier 808 is `ModIsolyticDefense`. Only its operation-1 ADD records
enter the setup `isolytic_defense` pool. The historical Realta active-buff ranks
select five research values (`0.12 + 0.20 + 0.38 + 0.10 + 0.63 = 1.43`) and one
building value (`0.73`), totaling `2.16`. The research `0.20` row requires an
Explorer; it applies to this Realta. The current global snapshot contains a
group-116 `808/ADD` value of `1.5` behind unsupported condition 28, so it is
quarantined. All six deployed specs and rank curves match byte-for-byte in the
retained build-272 and build-273 caches. Journal ranks are zero-based; global
levels are one-based. No inferred defense modifier is installed.

Native `ModAllDefenses(73)` operation-3 MULT enters each of the armor, shield
deflection and dodge percentage pools. `ModPlating(12)`, `ModAbsorption(13)` and
`ModDodge(11)` enter those pools individually. The frozen client's `GetStatTypes`
and `EnumToKey` bind these exact destinations, and its launch stat calculation
uses `(base + ADD) × (1 + MULT)` before integer display rounding. The matched
Pike-only Realta launch snapshot selects research, building and global fractions
of `10.05 + 7.84 + 6.5 = 24.39` for all defenses; individual totals are `3.4`
plating, `4.4` absorption and `3.8` dodge. With native ship bases `3/22/1` and
the Explorer OSR shield addition `1`, these reconstruct launch armor `86.37`,
shield deflection `677.38` and dodge `29.19` (displayed `86/677/29`). Only
deterministic self-targeting launch (trigger 24) rows with known class gates are
compiled into these launch pools. Combat-start trigger 25 uses separate typed
ship-ability seats; unknown predicates remain quarantined.

For the matched Pike-only Realta against a non-Armada NPC Explorer, the selected
`73/MULT` CombatStart seats contribute `10` research (`1.75` unconditional,
`4.5` NPC/non-Armada, `3` self-Explorer/NPC/non-Armada and `0.75` against a
Hostile Explorer) plus `3` from nine NPC-gated group-116 account rows. No
building CombatStart row is supported in this snapshot. The source-backed
additional pool is `13`, entering the same base percentage calculation before
rounding: armor `3 × (1 + 24.39 + 3.4 + 13) = 125.37`, shield deflection
`22 × (1 + 24.39 + 4.4 + 1 Explorer OSR + 13) = 963.38`, and dodge
`1 × (1 + 24.39 + 3.8 + 13) = 42.19`. The native stat helper rounds these
nonnegative values to `125/963/42` before the mitigation curve. Simulator
rounding is enabled only when active selected native launch or CombatStart
defense sources exist; synthetic/manual-only defense contexts keep fractional
arithmetic. The three hostile-class condition codes 4/5/6 are admitted only
for the exact research buff IDs bound to their native localized project text.
Other IDs with those codes remain quarantined. The nine CombatStart group-116
rows are separate from the 114 supported setup definitions counted above. This
selected source proof is
for the attacker-side raw-defense path; it does not establish a PvP defender-side
CombatStart implementation or a general interpretation of conditions 4/5/6.

`native_building_buffs::building_bonuses_for_imported_buffs` returns `None` when
there is no active recognized source, allowing a legacy fallback. `Some`, including
an empty map, is authoritative: unsupported native scope must not revive an older
description-based bonus. The scenario integration replaces only the corresponding
legacy building families, preserving manual, research and other building sources.

At the current supplied Syndicate level 45, the controlled Realta snapshot yields
building pool contributions **49.15 hull**, **45.12 shield** and **24.05 weapon**.
At level 29 those same imported building records yield 42, 36.82 and 16.05, because
the three account gates are inactive. The historical journal independently
contains the six gated building rows, establishing that its relevant thresholds
were already met for that fight.

```powershell
node scripts/extract_stfc_building_buff_semantics.mjs <cache.bin> <new-output.json>
```

Imported file hashes remain part of the optimizer reuse fingerprint. The profile
fingerprint also hashes active/expired membership at each buff's timestamp. It
changes at an expiry boundary even if the game/mod is offline, and stays stable
between boundaries instead of invalidating caches on every clock tick.
