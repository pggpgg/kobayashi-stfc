# Contribute to the LCARS officer database

This guide tells you how to add an officer definition to KOBAYASHI, and how to update one.
The officers use the LCARS format.

## Overview

LCARS (Language for Combat Ability Resolution and Simulation) is the declarative
model for officer abilities. Edit `data/officers/officers.canonical.json` for
routine ability changes. The runtime builds the LCARS model from this catalog
in-process. It does not require a generated YAML file.

Use `cargo xtask regen-lcars` to export `data/officers/officers.lcars.yaml` for
inspection or explicit file-based workflows. Do not edit that generated file for
routine fixes. Shared authoring rules are in [AGENTS.md](../AGENTS.md#data-and-officer-definitions).

The ability display names come from `translations-officer_buffs.json`. The key is
the `loca_id` from `summary-officer.json`. The model builder puts those names in
the LCARS abilities. For the join model and for Ahvix as an example, refer
to [OFFICER_TRANSLATIONS_MAPPING.md](OFFICER_TRANSLATIONS_MAPPING.md).

Run `cargo run --bin generate_lcars` or `kobayashi generate-lcars`. By default the tool
fills each ability `name:` field from those two files. Use `--no-ability-names` to get the
older placeholder names in the form `{Officer} (Captain)`. The options `--summary` and
`--translations` set different paths below `data/upstream/data-stfc-space/`.

## The organization of the files

- **The name of an exported file.** When the LCARS parser reads an explicit directory, it loads
  only the files that match `*.lcars.yaml` or `*.lcars.yml`. It ignores the other YAML files
  in the same folder, for example a configuration file. Use this name format for each LCARS
  officer file.
- The exported `officers.lcars.yaml` holds all the officers of all the factions. Each officer has an
  `id`, a `name`, and up to three ability blocks: captain, bridge, and below decks. An
  officer can also have a `faction`, a `rarity`, and a `group`.

## The structure of an officer

This example shows the exported LCARS model. Make routine changes in the canonical
JSON catalog, then generate the export to inspect the result.

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

## The mapping of the modifiers (canonical to LCARS)

Use this table when you convert data from the game or from a spreadsheet:

| Game modifier                              | LCARS stat / effect                              |
| ------------------------------------------ | ------------------------------------------------ |
| CritChance                                 | stat_modify → crit_chance                        |
| CritDamage                                 | stat_modify → crit_damage                        |
| AllDamage, OfficerStatAttack               | stat_modify → weapon_damage                      |
| ShipArmor, AllDefenses, OfficerStatDefense | stat_modify → armor / shield_mitigation          |
| ArmorPiercing, AllPiercing                 | stat_modify → shield_pierce                      |
| ShieldHPMax                                | stat_modify → shield_hp (multiply)               |
| HullHPMax                                  | stat_modify → hull_hp (multiply)                 |
| ApexShred, ApexBarrier                     | stat_modify → apex_shred / apex_barrier          |
| IsolyticDamage, IsolyticDefense            | stat_modify → isolytic_damage / isolytic_defense |
| ShieldHPRepair, HullHPRepair               | stat_modify → shield_regen / hull_hp_repair (flat) or shield_regen_max_fraction / hull_hp_repair_max_fraction (% of max) |
| AddState (morale)                          | effect type: morale                              |
| AddState (assimilated/hull breach/burning) | effect type: assimilated / hull_breach / burning |
| MiningRate, CargoCapacity, and others      | type: tag (not for combat)                       |

## The mapping of the triggers

| Game trigger            | LCARS trigger   |
| ----------------------- | --------------- |
| ShipLaunched            | passive         |
| CombatStart             | on_combat_start |
| RoundStart              | on_round_start  |
| EnemyTakesHit, HitTaken | on_hit          |
| ShieldsDepleted         | on_shield_break |
| Kill, EnemyKilled       | on_kill         |

## Validation

Validate the data before you send your contribution:

```bash
kobayashi validate data/officers
```

The validation checks these items:

- The mandatory fields (`id` and `name`).
- Duplicate ids.
- The schema (the names of the statistics, and the combinations of trigger and duration).
- The mechanics matrix. The validation shows the mechanics with partial support and the
  mechanics that are planned.

## How to generate the LCARS file from the canonical file

`generate_lcars` writes one file, `officers.lcars.yaml`, below the path of `--output`. The
file holds all the officers, sorted by id.

After you refresh the upstream officer JSON files, first run
`python3 scripts/normalize_officer_id_strings.py`. The decimal ids and the below decks
`slot` fields then agree with `data/upstream/data-stfc-space/officers/*.json`.

```bash
python3 scripts/normalize_officer_id_strings.py
kobayashi generate-lcars data/officers/officers.canonical.json --output data/officers
```

You can also use the separate binary:

```bash
cargo run --bin generate_lcars -- data/officers/officers.canonical.json --output data/officers
```

## How to use LCARS in a simulation

The simulator builds LCARS from the canonical catalog by default. No source-selection
environment variable or generated YAML file is needed.

```bash
kobayashi optimize --ship uss_saladin --hostile 2918121098 --sims 5000
```

## The full schema

For the full schema, refer to
[DESIGN.md §3 LCARS Language Specification](DESIGN.md#3-lcars-language-specification). That
section describes the conditions, the scaling, and each effect type.
