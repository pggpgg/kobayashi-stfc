# Research HP, weapon, critical, isolytic, defense and penetration stat mapping

The importer now uses selected `BuffSpec` definitions from the retained build 273
static cache, interpreted through the pinned build 272 client consumer, to resolve research HP, weapon, critical, isolytic, launch defense and the source-reviewed penetration rows. Display flags,
localized descriptions and project names do not decide their units or scope.
This is evidence about client configuration; the cache does not encode a verified
binding to the combat server build.

## Evidence and regeneration

- [`client_buff_semantics_2026-10-20.json`](../data/research/client_buff_semantics_2026-10-20.json)
  contains 1,138 static research definitions, their ranked double curves, chances,
  target/trigger/condition codes, byte spans and payload hashes. It excludes player
  state. Modifiers 0/1/2/9/10/11/12/13/60/61/73/707/808 are selected by family;
  the penetration extension selects only its 17 audited buff IDs. The manifest
  records both cache and consumer builds and does not bind either to server behavior.
- [`client_buff_coverage_2026-10-20.json`](../data/research/client_buff_coverage_2026-10-20.json)
  lists every selected definition with public research project IDs/names, its
  supported mapping or exclusion reason, and its original predicates.
- The native operation proof is retained in
  [`client_verified_research_2026-10-20.json`](../data/research/client_verified_research_2026-10-20.json).
  `ApplySingleBuffToStatContainer` (`0x1817fe0c0`) puts an unchanged operation-1
  value into ADD and an operation-3 value into MULT. `ApplyBuffModifiersToValue`
  (`0x1817c2450`) computes `(base + ADD) * (1 + MULT)`.
- `GetStatTypes` (`0x181e9b9e0`) maps modifier 9 to `StatType.CriticalChance`
  (static offset `0x30`) and modifier 10 to `StatType.CriticalDamage` (`0x34`).
  These are fields 63848/63849 of metadata type 10855. Thus a native critical
  damage ADD bonus adds directly to the base multiplier. The new
  `crit_damage_flat` key preserves that meaning; the older curated
  `crit_damage` key retains its historical multiplication semantics.

Extract the manifest from an explicitly selected local `ServiceResponse` cache:

```powershell
node scripts/extract_stfc_research_buff_semantics.mjs <cache.bin> <new-output.json>
```

Extraction creates its output exclusively. Inspect a newly extracted file before
replacing the tracked manifest. It retains exact raw double values and compacts
only identical trailing values, recording the original count. Explicit zero ranks
remain zero. Unsupported or unknown protobuf fields quarantine the affected buff.

Regenerate the catalog and coverage report using cached public project details:

```powershell
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
npm run test:scripts
```

The manifest is required for this importer. Missing evidence fails the command
instead of reviving historical unit/scope guesses. For selected families, an
unsupported native row also blocks all explicit-map and prose fallbacks.

## Supported and excluded definitions

The general setup compiler accepts only self-targeting deterministic scalar
effects (self launch, code 24, or combat start, code 25) with no attributes and
either no predicates or one verified ship-class predicate. Three separately
verified one-round RoundStart mitigation rows are the sole exception. Class codes
0/1/2/3/116 mean explorer/interceptor/battleship/survey/sentinel; they survive as
`attacker_ship_class` conditions. Exact-grade, faction, target, weapon subtype,
duration and other predicates are not silently discarded.
The new defense families 11/12/13/73 require launch trigger 24; their
combat-start trigger 25 rows remain outside this setup selection.

| Supported native family | Operation | Engine key | Buff count |
| --- | --- | --- | ---: |
| ModAllDamage (2) | MultiplyAdd (3) | weapon_damage | 38 |
| ModHullHpMax (61) | MultiplyAdd (3) | hull_hp | 46 |
| ModShieldHpMax (60) | MultiplyAdd (3) | shield_hp | 41 |
| ModHullHpMax (61) | Add (1) | hull_hp_flat | 1 |
| ModShieldHpMax (60) | Add (1) | shield_hp_flat | 1 |
| ModCritChance (9) | Add (1) | crit_chance | 3 |
| ModCritDamage (10) | Add (1) | crit_damage_flat | 10 |
| ModIsolyticDamage (707) | Add (1) | isolytic_damage | 13 |
| ModIsolyticDefense (808) | Add (1) | isolytic_defense | 14 |
| ModAllDefenses (73) | MultiplyAdd (3) | armor, shield_deflection, dodge | 12 |
| ModPlating (12) | MultiplyAdd (3) | armor | 6 |
| ModAbsorption (13) | MultiplyAdd (3) | shield_deflection | 6 |
| ModDodge (11) | MultiplyAdd (3) | dodge | 6 |
| Three verified ModAllDefenses (73) status rows | MultiplyAdd (3), RoundStart (18) | base_mitigation_stats_bonus | 3 |
| ModAccuracy (6) | MultiplyAdd (3) | accuracy | 2 |
| ModArmorPiercing (7) | MultiplyAdd (3) | armor_piercing | 3 |
| ModShieldPiercing (8) | MultiplyAdd (3) | shield_piercing | 3 |
| ModAllPiercing (74) | MultiplyAdd (3) | accuracy, armor_piercing, shield_piercing | 12 |
| ModOfficerStatAttack (56), target 5 | MultiplyAdd (3) | officer_attack | 1 |
| ModOfficerStatDefense (57), target 5 | MultiplyAdd (3) | officer_defense | 2 |
| ModOfficerStatHealth (58), target 5 | MultiplyAdd (3) | officer_health | 1 |
| ModOfficerStatAll (59), target 5 | MultiplyAdd (3) | officer_attack, officer_defense, officer_health | 2 |

The manifest supports 226 of 1,177 selected definitions. It quarantines 951:
872 have unsupported conditions, 74 target an unsupported entity, one has an
unsupported operation, two have unsupported triggers, and two have unsupported
attributes. These counts concern selected research families only;
they describe the catalog importer's setup and three exact RoundStart mappings,
not the separate CombatStart seat compiler or complete player-buff coverage.
A quarantined bonus still exists in the game and can make a simulation
understate its real effect.

The regenerated catalog has 440 projects, compared with 806 before this audit:
403 prior projects are removed and 37 are newly represented. A complete comparison
of nonselected stats at every project rank found five changes, each caused by a
selected native modifier previously mapped to an unrelated stat:

| Project | RID / buff ID | Prior incorrect mapping | Retained native facts |
| --- | --- | --- | --- |
| Aggregation Critical Chance Reduction | 280366371 / 3234296157 | apex_barrier | CritChance (9), Sub (2), target 6, condition 17 |
| Temporal Flux Countermeasures | 462590122 / 2063882577 | isolytic_defense | AllDamage (2), MultiplyAdd (3), conditions 17 and 245 |
| Spectre (EVISCERATOR) | 730418719 / 809520677 | isolytic_damage | CritDamage (10), Add (1), condition 70001 |
| Defensive Tactics | 1914017763 / 2859405133 | damage_reduction | AllDamage (2), MultiplySub (4), target 6, conditions 27 and 131 |
| ▷ Enhanced PvP Crit Dmg Reduction | 3511569176 / 3431838420 | damage_reduction | CritDamage (10), Sub (2), target 6, conditions 86004, 28 and 137 |

All other nonselected project/rank bonuses and their values/conditions are
unchanged. Legacy critical-damage-floor research is outside this selected manifest
and remains a separate coverage limitation.

The later launch-defense expansion changed the then-current 350-project catalog
to 323 projects: eight gained supported defense rows and 35 lost previously
inferred mappings. Three removed projects had old `armor` mappings but
native modifier 7 targets both `armor_pierce` and `penetration`, so those defense
bonuses were incorrect. The runtime relationship between these two native keys
and the simulator's aggregate `armor_piercing` remains unbound. The three separately verified RoundStart mitigation projects then
returned as gated catalog rows, bringing the current catalog to 326 projects.
They require exact native buff IDs, payload hashes, modifier/operation/target/
trigger, one-round attributes, deterministic rank curves and status conditions
33 (Morale), 37 (defender Hull Breach) or 38 (defender Burning). A mismatched
source remains quarantined. Their canonical overrides also remain in
`research_canonical.json` and compile independently of catalog rows.

The bounded #319 cohort adds exact `ModAccuracy` (6), `ModArmorPiercing` (7),
`ModShieldPiercing` (8), and `ModAllPiercing` (74) sources. The build-272 client
`GetStatTypes` path maps them to accuracy, armor-piercing, and shield-piercing
axes; its operation-3 consumer places raw fractions in the MULT bucket. The
catalog uses the exact native class conditions for the selected class-gated IDs.
For `ModAllPiercing`, only the no-condition row and Direct Hit's exact self-grade-4
condition are supported. Other selected ModAllPiercing predicates and attributes
are explicitly quarantined so legacy `pierce` mappings cannot flatten them. These
are client-source mappings carried from the 273 static snapshot through the 272
consumer; server behavior and other condition/attribute combinations remain
unproven.

The residual #319 audit adds 39 exact BuffSpecs for currently legacy-mapped
catalog rows. Their build-273 values match the upstream values across every
rank. Native modifier and operation fields show that operation-3 values for
ModAccuracy (6), ModArmorPiercing (7), ModShieldPiercing (8), and ModAllPiercing
(74) are fractions consumed by the client MULT path. The selected Mod6/7/8 rows
with unsupported conditions remain quarantined. Of 16 ModAllPiercing rows, 10
are supported with no condition or one recognized ship-class gate; six with
unsupported conditions remain quarantined. Five ModOfficerStat rows (56--59)
with exact `targ_self_all` target 5 map to their corresponding officer stat; two
`targ_self_below_deck` rows remain excluded from crew-wide stats. In total, 15
of the 39 residual definitions receive explicit fraction mappings and 24 remain
visible in coverage with unsupported condition or target reasons. Presence in
the manifest blocks legacy ID/name fallback. The generated catalog removes
fallback-only unsupported rows or retains raw mapped values. This is limited to
the exact selected definitions and supported gates; it does not establish
server behavior or semantics for other rows sharing these modifiers.

Native `ModAllDefenses(73)` CombatStart (25) research takes a different path
from both launch bonuses and RoundStart status rows: active imported buff ranks
compile to conditional, unboostable ship seats. The selected Pike-only Realta
research rows total `10`: unconditional `1.75`, NPC/non-Armada `4.5`,
self-Explorer/NPC/non-Armada `3`, and Hostile Explorer `0.75`. Native localized
project text binds the last hostile-class gate only for buff `2708670536`;
the parallel Hostile Interceptor and Hostile Battleship gates are admitted
only for buffs `266932754` and `455315064`. Condition codes 4/5/6 on any
other ID remain unsupported. The research importer does not flatten these
CombatStart rows into unconditional catalog bonuses. With nine supported
NPC-gated group-116 rows totaling `3`, the matched attacker-side CombatStart
pool is `13`. Native integer rounding applies after the launch and eligible
CombatStart pools have formed the effective defense stats, before mitigation.

The exact Orlop Defense research row (RID `282179003`, buff `1895868262`)
uses native modifier 57, operation 3, target 11 (`SelfBelowDeck`) and launch
trigger 24. Its localized project description specifies officers below deck.
The importer therefore keeps its fractional rank curve under
`officer_defense_below_decks`, separate from crew-wide `officer_defense`.
Rank 10 is native `1.0` (+100%); the old display-flag conversion produced
`0.01`. The selected [source record](../data/research/verified_orlop_defense_2026-10-23.json)
and exact-project/source guards do not extend this mapping to other modifier
57/59 rows. A separate Bajoran Inspiration row has condition 231 and localized
Assault-only text; its `0.65` value remains excluded outside a proven Assault
context.

## Percentage-unit quarantine

Upstream `value_is_percentage` and `show_percentage` describe display metadata;
they do not identify whether a raw rank is a fraction or percentage points. The
research importer therefore requires an explicit mapping `value_unit` for every
percentage-flagged bonus: `fraction` and `absolute` preserve the raw number,
while reviewed `percentage_points` values are divided by 100. Missing units are
omitted from combat snapshots and reported as `percentage_unit_unverified` in
[`percentage_unit_quarantine.json`](../data/research/percentage_unit_quarantine.json).
The strict build-273/272 import quarantines 24 currently mapped BuffSpecs across
24 project records; all 24 had `value_is_percentage=true` and no reviewed unit.
Their values are not assigned a unit from their magnitude or display flag. The
generated research catalog consequently contains 276 projects instead of 300;
those 24 removed records had no other mapped combat bonus. This excludes
unverified catalog data rather than converting it into a plausible bonus.

## Concrete corrections

- Buff 4191797822 contributes **4,000 flat shield HP** at rank 10; buff 434613423
  contributes **3,400 flat hull HP**. Operation 1 is not a percentage bonus.
- Native operation-3 fractions such as `5` remain `5` (+500%). Their magnitude or
  `show_percentage` flag no longer causes division by 100.
- Native ModIsolyticDamage (707) operation 1 contributes directly to the
  isolytic baseline; operation 3 belongs to the separate cascade path. In the
  historical Realta global snapshot, four unconditional research sources at
  active ranks contribute `0.015 + 0.1 + 0.08 + 0.11 = 0.305`. The matching
  three building sources contribute `0.44 + 0.145 + 0.29 = 0.875`. Their
  source-proven baseline total is `1.18`. The `1.805` runtime total inferred
  from two combat logs is diagnostic only: its remaining `0.625` has no
  identified source and is not compiled as an override.
- Native ModIsolyticDefense (808) operation 1 contributes to the isolytic
  defense baseline. In the matched Realta active-buff list, five research
  sources contribute `0.12 + 0.20 + 0.38 + 0.10 + 0.63 = 1.43` and one building
  source contributes `0.73`, totaling `2.16`. The `0.20` research source is
  Explorer-gated. Other active global 808 records with unknown account or
  combat predicates are excluded from setup compilation.
- NS Burning Damage (RID 365419690, buff 1898558353) had a canonical `/100`
  conversion that bypassed the catalog. Its reviewed canonical definition now
  preserves the raw rank values `1..9`, target-Burning gate, and RoundStart timing.
  Its one-round native attributes are retained in the evidence. It is handled by
  that reviewed canonical path rather than the generic setup-only manifest mapper.
- NS HB Crit Damage Reduction (RID 3407808029, buff 981948123) previously omitted
  condition 137 and used different timing. Its canonical override is now empty
  until that complete target/condition contract can be implemented. Keeping an
  empty override prevents catalog fallback from reinstating the incorrect effect.
- The prior five verified defense/repair/cargo fixes and the Grade 4+ Direct Hit
  piercing mapping remain intact.

Tests cover raw and flat units, source-array precedence over conflicting display
data, zero ranks, all five class gates, unsupported predicate/operator/target
quarantine, deterministic extraction, exact byte provenance, malformed protobuf
input, canonical consistency and complete coverage accounting.

## Active eligibility differs from project ownership

Where a recognized global active-buff snapshot is available, selected setup
research families compile through `native_research_buffs` from active buff IDs
and current levels. The generated project catalog remains the fallback when
that source is unavailable. Canonical status/timing seats and nonselected
research families retain their separate paths.

Selected setup families now also include accuracy, armor/shield piercing and
crew-wide officer Attack/Defense/Health. Native rank selection retains ship-class
gates and Direct Hit's grade-4 minimum. Unknown grades cannot satisfy that gate.
The exact hash-bound Orlop Defense row remains below-decks-only; other target-11
officer rows remain unsupported. Replacement removes only the previously merged
research contribution, preserving manual, building and other sources. Scenario
summaries use the same ship grade and active-source selection as combat setup.

NS Hull Breach Isolytic Damage (buff 638902322) now explicitly declares
`round_start`, matching its retained native trigger 18, condition 37 and one-round
duration. Its raw rank values are unchanged. This corrects the former AttackPhase
default without establishing a new order for simultaneous round-start effects.

This distinction is present in the retained native game code.
`FleetPlayerData.EvaluateBuffs` (`0x181a196d0`, metadata method 70893) takes
`globalBuffs: List<BuffData>` and evaluates the entries in that supplied list;
owning a research project does not independently insert it into the list.
`BuffService.OnGlobalActiveBuffsMessage` (`0x1817c1710`) replaces the received
global state and invokes `AddOrUpdateActiveBuffData` (`0x1817c5aa0`) and
`RemoveOldBuffData` (`0x1817c6270`). The mod sync exports that global active state.

For example, Fortified Rampart (RID 1933674629, buff 748660265) belongs to the
Fleet Commander / Scientist tree. Its description says On Duty, but the linked
BuffSpec has no inline condition. The controlled snapshot owns project level 3,
whose raw hull fraction is 0.4; its global active-buff list and historical combat
journal both omit the linked buff. Requiring active membership excludes that
inactive effect generically. The compiler contains no project-name exception.

Using the controlled snapshot's actual active research source reproduces the
historical research pools: 75.15 hull, 65.15 shield, 18.05 weapon, 0.12 critical
chance, 1.45 direct critical-multiplier addition, and 3,400/4,000 flat hull/shield.
Unsupported native conditions remain quarantined even when an active buff ID is
present; membership does not authorize dropping the buff's remaining predicates.
