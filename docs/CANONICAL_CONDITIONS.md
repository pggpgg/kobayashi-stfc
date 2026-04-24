# Canonical officer `conditions` tokens

`data/officers/officers.canonical.json` stores PascalCase `conditions` on abilities. `generate_lcars` maps each token to an LCARS `condition` tree when the combat engine can evaluate it; unmapped tokens log `skipping unmapped canonical condition` and the emitted ability may be **weaker** than in-game (subset `and` of only the mapped arms).

See [DESIGN.md](DESIGN.md) §3.4 for LCARS condition types, `[map_canonical_condition_token](../src/lcars/canonical_conditions.rs)`, and `[resolve_lcars_condition](../src/lcars/resolver.rs)`.

## Triage (frequency × feasibility)

The table below lists **still-unmapped** tokens (60 unique strings in canonical; **12 remain** after the mappings in §Scenario literals and §Engagement / armada helpers). Counts are occurrences in `officers.canonical.json` (not officer cardinality). Run `cargo run --bin report_unknown_mappings` for the live table.


| Token                                                                                                              | Count         | Bucket   | Note                                                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------ | ------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ModuleKinetic` / `ModuleEnergy`                                                                                   | 11 / 8        | deferred | Weapon module context not in condition evaluation.                                                                                                                     |
| `TargetStateAny`                                                                                                   | 7             | deferred | Composite state bundle not modeled as one predicate.                                                                                                                   |
| `SelfHull`* / `SelfMining` / `SelfCloaked`                                                                         | various       | deferred | Player hull identity / mining / cloak not in `CombatContext`.                                                                                                          |
| `SelfStateNone`                                                                                                    | 2             | deferred | “No state” bundle not modeled.                                                                                                                                         |
| `HitEnemyWithEnergy` / `HitEnemyWithKinetic`                                                                       | 1 each        | deferred | Per-hit weapon type context not available at generic condition evaluation.                                                                                             |
| `EnemyStronger`                                                                                                    | 1             | deferred | Power comparison not in context.                                                                                                                                       |
| `CargoFull` / `CargoEmpty`                                                                                         | 1 each        | deferred | Cargo not modeled.                                                                                                                                                     |
| `EnemyNotToaTrialHostile`                                                                                          | 1             | deferred | Trial-specific hostile tag not modeled.                                                                                                                                |


### Task 2 audit (engine-ready mappings only)

Re-checked **unmapped** tokens from `cargo run --bin report_unknown_mappings` against resolver / `AbilityCondition` evaluation. `**SelfHasHullBreach`** and `**SelfHasBurning`** were later implemented: `[CombatContext](../src/combat/abilities.rs)` exposes `attacker_hull_breach_active` / `attacker_burning_active`, counter and receive-damage paths update attacker debuff counters, and canonical maps them to LCARS `attacker_hull_breach` / `attacker_burning`. `**SelfOfficerTalNotOnBridge`** maps to LCARS `attacker_officer_tal_not_on_bridge` / `[AbilityCondition::AttackerOfficerTalNotOnBridge](../src/combat/abilities.rs)` using `attacker_tal_assigned_captain_or_bridge` on `[CombatContext](../src/combat/abilities.rs)` (derived from attacker `[CrewConfiguration](../src/combat/abilities.rs)` Captain and Bridge seats vs `[TAL_OFFICER_LCARS_ID](../src/combat/abilities.rs)`). `**TargetHasAssimilated`** maps to LCARS `defender_assimilated` / `[AbilityCondition::DefenderAssimilated](../src/combat/abilities.rs)` using `defender_assimilated_active` on `[CombatContext](../src/combat/abilities.rs)`; the engine tracks `defender_assimilated_rounds_remaining` from defender crew `Assimilated` procs (PvP: opponent with Borg Assimilate). Ship-vs-hostile stays false unless defender crew data includes assimilate. See `[task2_deferred_tokens_remain_unmapped](../src/lcars/canonical_conditions.rs)` for the still-deferred set.

Regression: `[task2_deferred_tokens_remain_unmapped](../src/lcars/canonical_conditions.rs)` asserts the remaining deferred tokens stay unmapped until engine work lands; remove a token from that test when you add a deliberate mapping.

## Mapped today (reference)

Handled in `[map_canonical_condition_token](../src/lcars/canonical_conditions.rs)` and/or **`generate_lcars` attribute merge** (see `[effect_condition_from_canonical](../src/bin/generate_lcars.rs)`): `TargetHasBurning`, `TargetHasAssimilated` (LCARS `defender_assimilated`), `TargetHasHullBreach`, `SelfHasBurning` (LCARS `attacker_burning`), `SelfHasHullBreach` (LCARS `attacker_hull_breach`), `SelfHasMorale`, `SelfOfficerTalNotOnBridge` (LCARS `attacker_officer_tal_not_on_bridge`), `CombatBattleType` (LCARS `combat_battle_type_any` with `battle_types=[...]` from canonical `attributes`; currently lenient when scenario battle type is unknown), `TargetMaxLevel` (LCARS `defender_level_at_most` with `max_level=` from canonical `attributes`; currently lenient when scenario defender level is unknown), `HullHealthBelowStartOfCombat` / `HullHealthBelow` / `HullHealthAbove` (LCARS `stat_below` / `stat_above` with `percentage=` from canonical `attributes`), **`EnemyHullFaction`** (LCARS `defender_hull_faction_id` with `faction_id` parsed from ability `attributes`; evaluated against `[CombatContext::defender_hull_faction_id](../src/combat/abilities.rs)` from hostile `faction.id` via `[SimulationConfig::defender_hull_faction_id](../src/combat/types.rs)`), `EnemyHostile`, `EnemyPlayer`, `EnemyArmada`, `**TargetIsArmada`** (same as `EnemyArmada`), `**TargetNotArmada`** (LCARS `not` around `defender_ship_type_is` `armada`), `Enemy{Explorer,Battleship,Interceptor,Survey,Surveyor,Armada}`, `Self{…}` hull classes (explorer, …; not `SelfHas`* breach/burn, which are explicit tokens above), plus §Scenario literals and §Engagement / armada helpers below.

### Scenario literals (ship-vs-hostile)

These tokens have **no dedicated `CombatContext` field** (or no field for the overworld nuance); they are mapped to LCARS `literal_true` / `literal_false` and [`AbilityCondition::LiteralBool`](../src/combat/abilities.rs) with fixed truth values for the **default Kobayashi engagement** (player ship attacks an NPC hostile in space, not station / capture-node / takeover / mining overworld). This removes `generate_lcars` noise and tightens gates versus silently dropping the token. Revisit if the engine gains those encounter contexts.

| Canonical token | Value | Rationale (short) |
| --- | :---: | --- |
| `TargetNotASB` | true | Anti-station-boss nuance not modeled; treat as non-blocking in hostile fights. |
| `SelfAttacking` | true | Attacker role matches ship-vs-hostile optimize path. |
| `TargetNotPlayerStation` | true | Defender is an NPC hostile, not a player station. |
| `TargetNotInvadingEntity` | true | Invading-entity encounter not distinguished on defender; treat as non-blocking for ship-vs-hostile. |
| `SelfDefending` | false | Defense / capture-node defender context not active when the player is the attacker. |
| `EnemySentinel` | false | Defense-platform / station-sentinel fights not the default defender model. |
| `CombatGameContext` | false | Capture node, mining, takeover, etc. not represented in `CombatContext`. |
| `SelfAtStation` / `SelfAtWaveDefenseChallenge` / `SelfAtAssault2` | false | Location / mode not active on the default path. |
| `TargetIsInvadingEntity` | false | Invading entity tag not modeled separately from hull class. |

### Engagement / armada helpers

- **`SelfAtSoloArmada`** → LCARS `engagement_includes` with `enemy_type: solo_armadas` → [`AbilityCondition::EngagementIncludes`](../src/combat/abilities.rs) with [`EnemyType::SoloArmadas`](../src/combat/types.rs). True when the hostile row supplies `engagement_enemy_types` including that tag (see [`HostileRecord::engagement_enemy_types`](../src/data/hostile.rs)); otherwise false in combat.
- **`TargetIsArmadaOrInvadingEntity`** → LCARS `defender_ship_type_is` `armada` (same as armada hull class). The **invading-entity** half is still not a separate predicate; non-armada invading targets are not distinguished yet.

## Updates (engine + LCARS)

- **`literal_true` / `literal_false`**: fixed boolean gates for scenario-literal canonical tokens above; resolve to `[AbilityCondition::LiteralBool](../src/combat/abilities.rs)` (`[resolve_lcars_condition](../src/lcars/resolver.rs)`).
- **`engagement_includes`**: already used for `TargetNotSoloArmada` / `EnemyGroupArmadas` (group armadas); extended for **`SelfAtSoloArmada`** → `solo_armadas`.
- `**not`**: `[AbilityCondition::Not](../src/combat/abilities.rs)` + LCARS `type: not` with exactly one child (`[resolve_lcars_condition](../src/lcars/resolver.rs)`).
- `**TargetIsArmada` / `TargetNotArmada`**: canonical aliases for armada gating (see roadmap “canonical condition tokens”).
- `**TargetHasAssimilated`**: LCARS `defender_assimilated`; opponent-side assimilate duration (`defender_assimilated_active`), driven by defender crew assimilate procs in `[engine.rs](../src/combat/engine.rs)` (canonical data pairs with `EnemyPlayer` for PvP vs assimilating opponents).
- **`EnemyHullFaction`**: not in `map_canonical_condition_token` alone; `generate_lcars` strips the token, reads `faction_id=` from `attributes`, and emits LCARS `defender_hull_faction_id`. This matches upstream hostile `faction.id` exactly (distinct from coarse `[OpponentFactionTag](../src/combat/types.rs)` / `defender_faction_is`, where e.g. Eclipse may be `Unknown`).

## Regenerate unknown-mappings report

Machine-readable Markdown (unmapped canonical tokens + hostile `upstream_ship_type` coverage from `data/hostiles/index.json`):

```bash
cargo run --bin report_unknown_mappings -- --output path/to/report.md
```

Omit `--output` to print to stdout. Paths default to `data/officers/officers.canonical.json` and `data/hostiles/index.json` (relative to the crate root). Run `cargo run --bin report_unknown_mappings -- --help` for flags.