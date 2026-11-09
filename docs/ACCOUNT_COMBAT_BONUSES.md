# Account combat bonus source audit

This audit separates native reward definitions from bonuses that apply to a
specific fight. It covers personal Syndicate rewards, the inspected Stella
milestone, Monaveen PvP milestones, and a controlled first NX-01 upgrade.
It does not establish complete coverage of every Archive.

Combat setup uses active imported BuffIDs, their current ranks, and their native
conditions. An account level, an Archive name, or a displayed reward total does
not add another copy of those bonuses.

## Syndicate source coverage

The maintained [native Syndicate catalog](../data/native_syndicate_rewards.json)
contains all 65 tiers and 229 reward references from native `LoyaltySpecs`, group
122. These references contain 229 distinct BuffIDs.

| Origin | Distinct reward BuffIDs | Treatment |
|---|---:|---|
| Global definitions, group 116 | 225 | Preserve rank, operation, target, timing, and conditions |
| Linked building definitions, group 68 | 3 | Require building ranks and Syndicate conditions |
| Missing native definition | 1 | Keep the reference unresolved |

BuffID `3786995299`, referenced at tier 61, collection 3, has no definition in the
reviewed global or building sources. The public Syndicate API also omits it.
No effect or value is assigned to this reference.

The [generator](../scripts/extract_native_syndicate_rewards.mjs) reads only the
selected static groups and records payload hashes and byte ranges. It rejects
unreviewed cache digests. Its `--check` mode compares deterministic output with
the tracked catalog. It does not read player profiles.

The `syndicate_bonuses` CLI uses this native catalog. It reports unconditional
definitions separately from conditional, unsupported, linked-building,
noncombat, and unresolved entries. Without an Ops level, it does not guess a
rank. Its output is an offline definition report, not an active-profile total.
The old spreadsheet calculator remains a compatibility helper; its two sample
levels do not establish actual Syndicate coverage.

## Unlock levels and stat ranks

Three different quantities must remain separate:

- A **Syndicate tier** identifies when a reward becomes available.
- A **global reward rank** selects an Ops-level snapshot. At Ops `L`, the
  reviewed arrays use index `L - 1`.
- A **building reward rank** selects a building-level snapshot. Ops cannot
  replace the building level.

Collection and reward indices order descriptors. They do not select stat ranks.
Rank arrays contain current snapshots, not amounts to sum across earlier ranks.
Only identical trailing values are compacted; the original array length remains
available for range checks.

The three linked building rewards are `995187207` at Syndicate 30, `1974652540`
at 33, and `560958387` at 35. Their native conditions are respectively 203, 204,
and 205. These conditions require the corresponding Syndicate minimum. Their
values also depend on building level, so a Syndicate table alone cannot resolve
them.

Active imports retain BuffID identity. Duplicate entries select the highest
eligible current level once. Expired entries do not contribute. The new reward
catalog and CLI do not merge level-derived totals into runtime profiles.

## Missing global piercing family

The source audit found six unlocked Syndicate piercing rewards outside the
previous global extraction and compilation path. Each is native modifier 74,
`ModAllPiercing`, operation 3, self-ship target 1, and launch trigger 24. Their
conditions, attributes, and unknown-field lists are empty; each chance is 1.

| Syndicate unlock | BuffID | Raw fraction at Ops 61 |
|---:|---:|---:|
| 4 | 1333781883 | 0.40 |
| 9 | 3809461333 | 0.60 |
| 18 | 1085339663 | 0.90 |
| 28 | 675636702 | 1.25 |
| 32 | 609129565 | 1.50 |
| 35 | 2259348690 | 1.75 |
| **Total** | | **6.40** |

At Ops 61, these definitions contribute an additive fraction of **6.40
(+640%) to each of accuracy, armor piercing, and shield piercing**, when their
active BuffIDs are present. They do not add 6.40 to raw mitigation or to a
single generic damage-through value. Native operation-3 values are already
fractions; display percentage flags do not authorize another division by 100.

The correction admits the verified global piercing family through the existing
native source path. It does not add a separate Syndicate total. Other operations,
targets, triggers, attributes, or unresolved predicates retain their diagnostics.
All-defense rewards likewise modify armor, shield deflection, and dodge; they
are distinct from a final mitigation fraction.

## Public source cross-checks

The requested [STFC.space page](https://stfc.space/syndicate-and-emerald-chain)
loads personal Syndicate and Emerald Chain as separate entity types. Its
[personal Syndicate API](https://data.stfc.space/syndicate/summary.json) contains
the same 65 tier identities and point bounds. All 225 public global reward
BuffIDs match the reviewed native definitions, including every rank value and
chance. The API omits the three linked building definitions. The separate
[Emerald Chain API](https://data.stfc.space/emerald_chain/summary.json) is not a
substitute for personal Syndicate data.

The [Spocks.club Syndicate table](https://spocks.club/syndicate/) matched 334 of
336 checked combat cells. Each advertised Ops band was checked against all
native ranks in that band. Two cells differ:

| Reward | Syndicate tier | Ops band | Spocks.club | Native value |
|---|---:|---|---:|---:|
| Weapon damage, BuffID 3716446006 | 17 | 40–50 | 200% | 20% |
| Shield HP, BuffID 2849989052 | 41 | 40–50 | 10% | 100% |

The native values also match STFC.space's API. The community table therefore
serves as corroboration, not as an unconditional source of runtime bonuses.

Some native Syndicate combat rewards remain excluded. Damage BuffIDs
`831877216` and `2118637918` carry conditions `[117, 236]`. Apex BuffIDs
`1369297542` and `313675470` carry `[28, 137]` and `[74005, 137, 246]`.
Critical-damage BuffID `1464566611` carries `[28, 137]`. Active membership does
not authorize dropping these predicates. Economy, cargo, travel, repair, XP,
reputation, and loot rewards remain separate from fight-stat totals.

## Archives and the Stella milestone

Scopely's [Independent Archives guide, published 8 November
2024](https://startrekfleetcommand.com/news/exploring-the-independent-archives-unlocking-powerful-bonuses/)
describes the five initially available tracks: Botany Bay, Vi'Dar, Vi'Dar Talios,
Monaveen, and USS Voyager. It explains progression and mastery. That initial
list does not describe all later tracks, including the Stella Archive inspected
in the current client.

The retained build-274 claim path uses generic Museum and event objects.
`MuseumManager.ClaimTask` at `0x180e58320` reads a task ID from an event
model and calls `TournamentService.UnlockTask` at `0x180fe4ed0`. This establishes
a generic claim mechanism. It does not establish a complete static mapping from
every named Archive and milestone to server-granted BuffIDs.

For combat, active BuffIDs remain authoritative after a claim. A displayed
Archive milestone does not create an extra bonus beside an already imported
BuffID. Inventory of additional native Apex definitions does not imply that
their conditions are supported.

The first Stella milestone has a bounded source binding:

| Effect | BuffID | Native operation | Native value |
|---|---:|---|---:|
| Apex Barrier | 3061559261 | Modifier 67001, ADD 1 | 100 absolute points |
| Damage | 4041662025 | Modifier 2, ADD 1 | 2,300 flat points |

Both definitions use self target 1, combat-start trigger 25, conditions
`[129, 150, 17]`, and faction attribute 8 with value `1750120904`. Their 99 rank
snapshots preserve these values. Native localization IDs 95003 and 95005 and
the inspected Stella UI corroborate the Eclipse Armada scope. No purchase was
needed for that inspection.

Only the exact first Stella Apex definition is admitted for Stella. The source hash is
`b6e7cca49475cd554ce866576e725b9ab9f54ee15fa5ef1832f37f5b5cc09262`.
Its runtime gate requires the owning hull to be Stella, an Eclipse Armada
opponent, and non-PvP context. Raw condition 17 has no general-purpose predicate
binding; this source-specific proof does not broaden other condition-17 rows.

The damage value is **2,300 flat points, not 2,300%**. Its insertion order in the
damage calculation remains unproved, so it stays unsupported. Later Stella
milestones and other Archive predicates require their own complete bindings.
The 2024 article does not provide a public source for these newer Stella rows.

## Monaveen PvP Apex milestones

The current client shows twelve completed Monaveen milestones with Apex rewards
`[50,50,75,75,100,100,100,150,150,200,200,250]`: **1,500 raw points in total**.
The first milestone tooltip specifies all ships in PvP. This corroborates the
cumulative 1,500-point benefit in Scopely's 2024 guide; it is not a single
1,500-point BuffSpec.

Native localization 72023 links this exact twelve-definition family to that
scope. The IDs are `2199227326`, `2280947091`, `3378417511`, `3663693120`,
`1721910014`, `1763915157`, `3558267162`, `487851441`, `2328144758`,
`610243845`, `1240186710`, and `2956393975`. All use modifier 67001, ADD 1,
self target 1, CombatStart 25, and conditions `[28,137,246]`. Each complete
99-rank value/chance array and source hash agrees between retained builds
272 and 273. Runtime admission binds each exact ID, hash and fixed value.

Condition 246 means **Tal is not assigned captain or bridge on the owning
ship**. The shorter Archive tooltip omits that restriction; the simulator keeps
it. Tal below decks does not violate it. Conditions 28/137 are admitted only for
this exact source-bound family within player ship combat, not as general enum
interpretations. Stations and unrelated Apex definitions are not admitted.

Active milestones sum once under `apex_barrier_vs_player_tal_not_on_bridge`.
The scenario resolves that conditional pool separately for each PvP ship using
its own crew. This also prevents the attacker's Tal assignment from suppressing
the defender's rewards, or the reverse. Unowned or expired milestones do not
contribute; source level selects one snapshot rather than summing rank history.

## Controlled NX-01 reward validation

A single approved first NX-01 milestone cost 1,200 Archive Data Shards. The game
showed the first milestone complete, the next unclaimed, and a balance of 775.
Immutable local before/after captures contained exactly four new BuffIDs, with
no changed or removed buff rows. Raw player captures remain private.

| Reward | Added BuffID | Native interpretation |
|---|---:|---|
| Hostile piercing | 1404701608 | Modifier 74, MULT 3, self 1, CombatStart 25; `[27,131]`; value `0.1` |
| Efficient components | 399536688 | Ceramic Composite cost efficiency on NX-01 components; not a fight stat |
| Maximum cargo | 2872041041 | NX-01 cargo; not a fight stat |
| Xindi Scraps loot | 4023778106 | Xindi-Aquatic loot; not a fight stat |

Native localization 91005 specifies **all piercing stats for all ships against
non-Armada hostiles**. The UI's shorter “Hostile Piercing” label does not expand
the scope to Armadas or limit it to the NX-01 hull. The combat BuffSpec hash is
`7f6ec2a4d07dca9cb9a04d21e8745820a71b8c843dd216375e96729cf78fc9cb`.
Its complete 99 value and chance snapshots match between retained builds 272
and 273 and the retained public definition. Synced rank selects one snapshot.

The simulator now compiles this family after resolving the fixed opponent
domain. The `0.1` enters each existing piercing percentage pool before scaling
raw ship and weapon stats. It does not multiply already buffed stats by 1.1,
add damage-through pierce, or add another total from the archive milestone.
Distinct milestone BuffIDs stack; duplicate imports of one BuffID count once.

The inspected next Mantis milestone, number 9, costs 1,600 shards and gives a
`0.05` critical-factor addition against non-Armada hostiles. Its native modifier
10, ADD 1, CombatStart 25, `[27,131]` family was already supported. Equal-valued
definitions alone do not identify that milestone's exact BuffID; no Mantis
upgrade was made. Neither displayed percentage is a percentage increase in
total damage.

## Retained artifact identities and limits

These hashes identify the retained client artifacts. They do not identify the
current server implementation.

| Client build | GameAssembly SHA-256 | Metadata SHA-256 |
|---|---|---|
| 272 | `a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274` | `ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04` |
| 273 | `a9ab2edbc143cfba3aff35665e129006f73853070dbff206884fda04a235ab94` | `2b586d6f23eb130c210ddca13d8ebdd21f09f807e2eb94f4972bf321b5f821da` |
| 274 | `6b9e080b693ca6de2e2ae12d3ffc5220c116c68672439f579bd8a1f56e1179e5` | `abd0233dfb3c66eb6a8b70789e0c5d1e5a4b77b7977378768b9eeb497f62601f` |

The Syndicate catalog uses static cache
`b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf`.
The 228 resolved reward definitions also match the retained cache
`b7fcb7a021fa982144702a682e81eb6b7a31406522b0154cc77553a2c0776418`.
The catalog preserves the build-274 metadata identity used to check the wire
schema separately from the cache identity.

These checks establish source identity, reward linkage, units, and bounded
eligibility rules. They do not establish full Archive coverage, unknown
predicates, flat-damage insertion order, or parity with unobserved server rules.
