# Native V'ger Gorn Hunter evidence

The native supplement identifies **V'ger Gorn Hunter, level 62**, without a
name-only catalog match. HullSpecs group 1 contains ID `2978535446`, named
`Hull_L62_Explorer_M86_Gorn`. Its first 13 ordered components match the saved
fight, including four kinetic weapons `3804869128` and two energy weapons
`404883461`. The final static Special component is absent from the journal.

[The native snapshot](../data/native_hostile_static_snapshot.json) preserves the
selected source records. [The importer](../scripts/import_native_hostile_snapshot.mjs)
reads selected static groups only. It does not read player profiles or journals.
The supplement covers this reviewed identity; it does not establish other levels.

## Source identity

| Source | SHA256 |
|---|---|
| Build 274 GameAssembly.dll | `6b9e080b693ca6de2e2ae12d3ffc5220c116c68672439f579bd8a1f56e1179e5` |
| Build 274 IL2CPP metadata | `abd0233dfb3c66eb6a8b70789e0c5d1e5a4b77b7977378768b9eeb497f62601f` |
| Installed static cache, 24,991,479 bytes | `6df4bfd9a33ff70304264d0abf0d39db2bc31be553b04e96c0d0b64c8a53d35e` |
| Retained build 273 static cache | `b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf` |
| English localization | `e68228ccde494b6f0713946ffb91ace7a938648e7fb800cab013df7edccaadcb` |
| Selected HullSpec | `3fa46293f393ea412aa1f117715476bc34c7f617457a1a913791dea14f201b85` |
| Saved fight fixture | `f8e585985baa30e5e4ec95eee342f99bb1ac3c50c52a789ba9b12aa6bbe68a64` |

The installed cache and the retained build 273 cache have 98 identical static
entity-group payloads out of 99. Only CosmeticSpecs group 87 differs. The relevant hull,
component, passive-buff and activated-ability bodies are unchanged. A different
whole-cache hash does not establish new combat configuration.

Native component HP uses float32. The supplement retains those values instead
of replacing them with journal HP. The small HP differences therefore remain
visible and do not become fitted stat overrides.

## Skills and linked native rows

HullSpec field 35 links nine BuffSpecs in ShipBonus group 51. Field 28 links two
ActivatedAbilitySpecs in group 128. Native target 1 is self; target 6 is the enemy.
Trigger 25 is combat start; trigger 18 is round start.

Localization binds two displayed skills:

- `ship_ability_name_86111`: **Replicated Plasma Cannon**. Its description lists
  Isolytic Damage and all three piercing statistics at combat start. It also
  lists an enemy shield-mitigation reduction each round.
- `ship_ability_name_86112`: **Replicated Counter-Intel**. Its description lists
  Isolytic Defense, a restriction to Isolytic Damage received, Cutting Beam
  immunity and an officer-dependent Apex Barrier increase.

The seven mechanical rows with loca 9999 have no individual localized title.
Their presence on this hull is exact; their grouping under either displayed
skill is not another explicit title-to-row link.

| Buff ID | Native modifier | Operation | Target and time | Raw entry at index 61 |
|---|---|---|---|---:|
|3215268814|2, all damage; loca86111|MULT3|Self, combat start|0.01|
|1761765323|2, all damage; loca86112|MULT3|Self, combat start|0.01|
|4196543321|707, Isolytic Damage|ADD1|Self, combat start|14.91|
|1029835177|74, all piercing|MULT3|Self, combat start|5.68|
|1844469045|65002, no bound client enum name|MULT3|Self, combat start|1|
|4173861882|76, shield mitigation|SUB2|Enemy, round start|0.01|
|4194583694|808, Isolytic Defense|ADD1|Self, combat start|1.9|
|2026725568|67001, Apex Barrier|ADD1|Self, combat start|3400|
|1421423212|67001, Apex Barrier|ADD1|Self, combat start; condition86001|10000000|

All nine rows use Value mode 0, 90 ranked doubles, chance 1 and no attributes.
Their first and last entries are 0.01. No implicit division by 100 is justified.

**Index 61 is source data, not a general level-to-rank rule.** Native BuffData
indexes rank-1 and clamps an excessive index to the last entry. But the client
passive-view constructor passes rank 1. It therefore displays index 0. The server's
active rank requires separate evidence for each admitted mechanic.

## What the saved fight establishes

The native attack parser binds the recorded damage channels. An explicit channel
partition independently gives an NPC Isolytic coefficient from 14.90999876 to
14.91000158 across 26 shots. This corroborates the source entry 14.91 for the
captured level 62 case, within integer emission precision. It does not establish
every passive's rank or a complete damage formula.

The source's 0.01 enemy shield-mitigation subtraction is constant across all 90
ranks. The three observed rounds corroborate that per-round reduction after
separately known player modifiers. The damage restriction in Counter-Intel also
agrees with the negligible physical survivor channel across 20 player shots.

The remaining numeric entries need stricter limits:

- **Isolytic Defense 1.9:** the observed conditional channel ratio is about
  1.89855072. It is not equal to 1.9 within emission precision. Player piercing,
  stacking and channel order must be bound before claiming an exact match.
- **Apex Barrier 3400:** the analogous effective-points diagnostic is about
  3090.90719. That observation does not independently identify a shred amount.
  No shred coefficient is inverted from this fight or added to production.
- **All piercing 5.68:** the curve contains this entry, but the fight alone does
  not separate it from the player's effective defensive statistics. Each weapon
  must retain its own baseline. Kinetic AP/SP/Accuracy is 58523/585230/58523;
  energy is 58523/58523/585230.
- **Conditional Barrier 10000000:** condition 86001 must not become unconditional.
  No executable predicate or crew-seat scope is established for this code.

Suder's native bridge ability is previous-round hull repair, not an identified
Apex Shred source. Neither its rank costs nor its progress-reward fields supply
the missing shred proof. Serialized pre-combat dictionaries are also distinct
from final passively modified combat statistics.

## Conditions and immunity limits

Counter-Intel names SNW Pike, SNW Pelia, SNW Sam Kirk, SNW James Kirk and SNW
T'Pring as officers that trigger a large barrier. This supports the displayed
restriction. It does not bind numeric condition 86001 to exact officer IDs,
captain, bridge or below decks evaluation. The current metadata condition enum
and selected static record search do not define 86001. Unknown scope remains
unsupported and requires a diagnostic.

The activated entries are `2678416666`, `CUTTING_BEAM_IMMUNITY`, type 22, and
`1955293070`, `CHAIN_SHOT_IMMUNITY`, type 78001. Neither links further buff IDs.
The corresponding serialized flags are present. Localization independently
confirms Cutting Beam immunity, but no exact localized Chain Shot immunity title
was bound. The raw duration 10000000 is not asserted to mean combat rounds.

Other similarly named Counter-Intel descriptions mention hull thresholds,
repairs or a larger officer list. They do not join loca 86112 and are not assigned
to this hunter. No new fight, UI action, game attachment or live-state change was
used to obtain this evidence.

## Simulator comparison and limits

The original catalog had no record for this exact hostile ID. A private replay
uses the recorded Enterprise-D tier 12, level 59 and ordered crew, frozen account
imports, and the journal-linked Quantum equipment at its owned rank. Current
imports are not an atomic historical account snapshot. The model's player hull
and shield maxima agree with the recorded maxima to integer precision.

Across the same 256 seed offsets, the supported-skills model wins all fights:
137 end in round 1 and 119 in round 2. The saved fight wins in round 3. Average
net hull loss is about 114.63 million versus the recorded 58.15 million. This is
an unresolved mismatch, not calibrated parity. Event tracing and tracing off
produce identical outcomes for the replayed base seed.

A diagnostic ablation with the same native hull but no hostile skills also wins
all 256 trials: 190 in round 1 and 66 in round 2. Its mean net hull loss is about
4.77 million. That ablation is not the old simulator's prediction: the old
catalog could not resolve this hostile at all. It isolates the effect of the
new skills without making a claim that every observed column is now closer.

The recorded starting shield exceeds its recorded maximum, and the journal
contains additional shield-state transitions. The replay retains the model's
launch shield rather than inventing those transitions. Missing hostile Isolytic
Defense, Apex Barrier, piercing and conditional behavior remain other material
limits. The source-backed skills can be tested independently, but this single
fight is not a validation or holdout set for a complete Hunter model.
