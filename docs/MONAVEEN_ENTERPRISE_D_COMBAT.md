# Monaveen and Enterprise-D combat corrections

## Evidence and scope

Saved fights exposed errors in ship abilities, launch health pools and incoming
piercing. The fixes use native source definitions and reusable stat operations;
they do not insert observed ship health or a multiplier fitted to a battle.
Raw player profiles and battle captures remain private. Regression tests use
public catalog rows and synthetic combatants.

The retained client evidence is build 274, with GameAssembly SHA-256
`6b9e080b693ca6de2e2ae12d3ffc5220c116c68672439f579bd8a1f56e1179e5`
and metadata SHA-256
`abd0233dfb3c66eb6a8b70789e0c5d1e5a4b77b7977378768b9eeb497f62601f`.
REA metadata evidence
`ev_072e703b59f08b22c2a1840914c6d7c3760c8b1fba243d9fc441af6d6a0e9117`
binds the inspected methods. Static client code supports source semantics;
saved server battle results provide the separate behavior check.

The static configuration cache is a separate build-273 input, SHA-256
`b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf`.
Keep its rank tables distinct from build-274 method evidence. The reviewed
equipment manifest records cache, upstream detail and individual buff hashes.

## Monaveen: Modernity is Overrated

Native buff `49906243` uses target 1, trigger 25, operation 3 and modifier 2.
Its condition 17 and attribute field 8 select enemy hull faction `2796195869`.
This is the exact Texas-class gate; a matching name or general Federation
association is insufficient. Runtime hostile masks derive this tag from the
native faction, including older normalized records with an empty tag list.
The combat seat also requires an NPC hostile.

The native rank curve is already fractional: level 54 selects 2,375 and level
55 selects 2,435. Do not divide it by 100. The ability uses the existing
component-base weapon damage family, adding `component_base × fraction` before
critical and isolytic processing. It does not multiply the completed profile
weapon damage. Native buff-row SHA-256 is
`d8f451d1571003ab59a76d2e2a92a405284a327659e05495b3a6f3f171039cde`.

The existing component-base family still approximates interactions with legacy
weapon/officer pools. Restoring this ability does not establish complete
agreement for every Monaveen damage channel or critical hit.

The catalog override is the maintained input; the normalized Monaveen record
retains the complete level curve. Catalog regeneration also repairs stale
Omicron and Realta override definitions to preserve their existing modeled
behavior. Kelcie Mae ability `2748355073` remains a no-op: its Capture Node,
Takeover and player-defense requirements are not supported. The generic Apex
heuristic must not turn that ability into unconditional Apex Barrier.

## Health and defense operation order

### Officer synergy

Build-274 `FleetPlayerData.get_LeftSynergyLevel` (`0x181a144f0`) and its right
counterpart (`0x181a14570`) select contribution indices using group and officer
class. Matching groups contribute index 1 for the same class or 2 for a
different class; a repeated matching bridge class reduces the right contribution
to index 1. Empty or unrelated seats contribute nothing.

`Officer.get_CaptainsManeuver` (`0x181ad42d0`) returns `SynergyBuffData`.
Its `CalculateValueAndChance` (`0x181986c90`) adds each nonzero indexed
contribution to entry zero. These entries are not officer-rank samples.
Only source-joined captain rows with matching complete values/chances use this
path. Bridge rank scaling and unknown/custom sources retain their existing
semantics. Native probability fractions saturate at one before generic
probability conversion.

### Launch health and incoming defense

Native launch-time max-HP `MultiplyAdd` percentages share the account percentage
pool. For an unconditional launch ability such as Tendi's hull bonus, use:

```text
(base HP + flat additions)
  × (1 + account HP fractions + officer Health bonus + launch ability fractions)
```

Preserve genuine final multipliers and timed or conditional ability semantics.
The launch fraction is consumed once during scenario construction, for either
player side. Its canonical rank values remain fractions throughout LCARS
conversion; adding one during rank transformation would introduce another error.

Account launch bonuses retain their resolved owner, minimum-grade, hull and
class conditions. Active buff IDs select owned ranks once across the native
research, building and account catalogs. Inventory collisions, unknown predicates
or missing scope inputs do not become unconditional bonuses.

Quantum Slipstream Drive uses source-bound curves for shield HP, critical chance
and opponent shield-mitigation subtraction. Its shield HP native multiplier is
not a percentage-point value to divide by 100. Owned level and unlocked tier
select each coefficient, and explicit equipment selection is mandatory. An owned
item or a buff inventory alone does not establish which ship equipped it.

The native client percentage accumulator corroborates the shared pool:
`BuffService.AddBuffDataToModifierValues` at `0x1817c6840` adds operation 3 and
subtracts operation 4 in the same percentage slot.
`ApplyBuffModifiersToValue` at `0x1817c6670` applies the resulting pool to the
base plus flat additions. These are static client observations, not direct
observation of the server's combat implementation.

Freebooter piercing bonuses must change raw enemy armor piercing, shield
piercing and accuracy before nonlinear mitigation. Multiplying the retired
synthetic damage-through helper leaves those abilities inert. The implementation
retains explicit synthetic fallback behavior separately from raw stat inputs.

Immortal Shield Mitigation Reduction targets the opponent for two rounds. It
must not increase the owner's shield mitigation. KSG's two-round own-shield
bonus is a separate source and retains its own timing.

## Validation boundaries

Compare actual hull and shield removal separately from raw damage counters.
The game can log the rest of a firing volley after the target reaches zero
hull. A matching one-round win therefore does not validate raw outgoing damage.
Fixed seed panels and trace-on/trace-off parity checks make the replay
comparison reproducible without choosing a favorable seed.

The golden snapshot update adds only the two zero-valued raw-piercing diagnostic
fields to existing counter traces. Structural comparison after removing those
fields confirms that every previous field, value and combat outcome is unchanged.

Unknown predicates, missing source ranks and unsupported timing remain
quarantined. The optimizer behavior version changes because launch/scenario
corrections can invalidate saved metrics outside synthetic engine canaries.
