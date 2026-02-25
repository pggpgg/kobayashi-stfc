# Contributing to the LCARS Officer Database

This guide explains how to add or update officer definitions in KOBAYASHI's LCARS format.

## Overview

LCARS (Language for Combat Ability Resolution & Simulation) is the declarative YAML format for officer abilities. Officer definitions live in `data/officers/*.lcars.yaml`, grouped by faction.

## File Organization

- `federation.lcars.yaml` — Federation officers
- `klingon.lcars.yaml` — Klingon officers
- `romulan.lcars.yaml` — Romulan officers
- `independent.lcars.yaml` — Independent, Unknown, and other factions
- `neutral.lcars.yaml` — Neutral faction
- `section31.lcars.yaml` — Section 31 officers

## Officer Structure

```yaml
officers:
  - id: officer-id-with-suffix
    name: "Display Name"
    faction: Federation
    rarity: epic
    group: "Group Name"

    captain_ability:
      name: "Ability Name"
      effects:
        - type: stat_modify
          stat: shield_pierce
          target: self
          operator: add
          value: 0.30
          trigger: passive
          duration: permanent
          scaling:
            base: 0.20
            per_rank: 0.025
            max_rank: 5

    bridge_ability:
      name: "Bridge Ability Name"
      effects: [...]

    below_decks_ability:
      name: "Below Decks Ability Name"
      effects: [...]
```

## Modifier Mapping (Canonical → LCARS)

When converting from game data or spreadsheets:

| Game Modifier | LCARS stat / effect |
|---------------|---------------------|
| CritChance | stat_modify → crit_chance |
| CritDamage | stat_modify → crit_damage |
| AllDamage, OfficerStatAttack | stat_modify → weapon_damage |
| ShipArmor, AllDefenses, OfficerStatDefense | stat_modify → armor / shield_mitigation |
| ArmorPiercing, AllPiercing | stat_modify → shield_pierce |
| ShieldHPMax | stat_modify → shield_hp (multiply) |
| HullHPMax | stat_modify → hull_hp (multiply) |
| ApexShred, ApexBarrier | stat_modify → apex_shred / apex_barrier |
| IsolyticDamage, IsolyticDefense | stat_modify → isolytic_damage / isolytic_defense |
| ShieldHPRepair, HullHPRepair | stat_modify → shield_regen / hull_hp_repair |
| AddState (morale) | effect type: morale |
| AddState (assimilated/hull breach/burning) | effect type: assimilated / hull_breach / burning |
| MiningRate, CargoCapacity, etc. | type: tag (non-combat) |

## Trigger Mapping

| Game Trigger | LCARS trigger |
|--------------|----------------|
| ShipLaunched | passive |
| CombatStart | on_combat_start |
| RoundStart | on_round_start |
| EnemyTakesHit, HitTaken | on_hit |
| ShieldsDepleted | on_shield_break |
| Kill, EnemyKilled | on_kill |

## Validation

Run validation before submitting:

```bash
kobayashi validate data/officers
```

Validation checks:
- Required fields (id, name)
- Duplicate IDs
- Schema (stat names, trigger/duration combos)
- Mechanics matrix (flags partial/planned support)

## Regenerating from Canonical

To regenerate LCARS files from `officers.canonical.json`:

```bash
kobayashi generate-lcars data/officers/officers.canonical.json --output data/officers
```

Or use the standalone binary:

```bash
cargo run --bin generate_lcars -- data/officers/officers.canonical.json --output data/officers
```

## Using LCARS in Simulation

Set the environment variable to use LCARS as the officer source:

```bash
KOBAYASHI_OFFICER_SOURCE=lcars kobayashi optimize --ship saladin --hostile explorer_30 --sims 5000
```

When unset, the simulator uses the canonical JSON format (default).

## Full Schema

See [DESIGN.md §3 LCARS Language Specification](../DESIGN.md#3-lcars-language-specification) for the complete schema, including conditions, scaling, and effect types.
