# Canonical officer `conditions` tokens

`data/officers/officers.canonical.json` stores PascalCase `conditions` on abilities. `generate_lcars` maps each token to an LCARS `condition` tree when the combat engine can evaluate it; unmapped tokens log `skipping unmapped canonical condition` and the emitted ability may be **weaker** than in-game (subset `and` of only the mapped arms).

See [DESIGN.md](DESIGN.md) §3.4 for LCARS condition types, `[map_canonical_condition_token](../src/lcars/canonical_conditions.rs)`, and `[resolve_lcars_condition](../src/lcars/resolver.rs)`.

## Triage (frequency × feasibility)

There are **60** distinct non-empty canonical tokens; `cargo run --bin report_unknown_mappings` should show **0** still-unmapped for the officer LCARS pipeline (each token resolves via `map_canonical_condition_token` and/or `generate_lcars` attribute merge). The table below is the **token-only** gap list: tokens that intentionally have **no** entry in `map_canonical_condition_token` because `generate_lcars` builds their LCARS from ability `attributes` instead.

| Token                                                                                                              | Count         | Bucket   | Note                                                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------ | ------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CombatBattleType` / `TargetMaxLevel` / `HullHealthBelowStartOfCombat` / `HullHealthBelow` / `HullHealthAbove`   | (varies)      | merged   | Emitted from canonical `attributes` by `generate_lcars`, not `map_canonical_condition_token` alone.                                                                    |


### Task 2 audit (engine-ready mappings only)

Re-checked **unmapped** tokens from `cargo run --bin report_unknown_mappings` against resolver / `AbilityCondition` evaluation. `**SelfHasHullBreach`** and `**SelfHasBurning`** were later implemented: `[CombatContext](../src/combat/abilities.rs)` exposes `attacker_hull_breach_active` / `attacker_burning_active`, counter and receive-damage paths update attacker debuff counters, and canonical maps them to LCARS `attacker_hull_breach` / `attacker_burning`. `**SelfOfficerTalNotOnBridge`** maps to LCARS `attacker_officer_tal_not_on_bridge` / `[AbilityCondition::AttackerOfficerTalNotOnBridge](../src/combat/abilities.rs)` using `attacker_tal_assigned_captain_or_bridge` on `[CombatContext](../src/combat/abilities.rs)` (derived from attacker `[CrewConfiguration](../src/combat/abilities.rs)` Captain and Bridge seats vs `[TAL_OFFICER_LCARS_ID](../src/combat/abilities.rs)`). `**TargetHasAssimilated`** maps to LCARS `defender_assimilated` / `[AbilityCondition::DefenderAssimilated](../src/combat/abilities.rs)` using `defender_assimilated_active` on `[CombatContext](../src/combat/abilities.rs)`; the engine tracks `defender_assimilated_rounds_remaining` from defender crew `Assimilated` procs (PvP: opponent with Borg Assimilate). Ship-vs-hostile stays false unless defender crew data includes assimilate. See `[task2_deferred_tokens_remain_unmapped](../src/lcars/canonical_conditions.rs)` for the still-deferred set.

Regression: `[task2_attribute_merged_hull_health_tokens_remain_token_only_unmapped](../src/lcars/canonical_conditions.rs)` asserts hull-health tokens stay absent from token-only mapping (they use attribute merge).

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

### Module line, opponent “any state”, cargo, and other approximations

| Canonical token | LCARS / condition | Fidelity note |
| --- | --- | --- |
| `ModuleKinetic` / `ModuleEnergy` | `literal_true` | Weapon module type is not evaluated; mapping is **lenient** so the token is not dropped from multi-token `and` rows (kinetic-only debuffs may over-apply vs in-game energy-only builds). |
| `TargetStateAny` | `or` of `defender_burning`, `defender_hull_breach`, `defender_assimilated` | Matches STFC “opponent has a debuff-style state” for the three flags the engine tracks. **Defender morale** is not included (morale is modeled on the attacker as `morale_active`). |
| `SelfStateNone` | `not` → `or`(`attacker_burning`, `attacker_hull_breach`) | Approximation for “no debuff state on self”; omits assimilate / morale and other STFC “states”. |
| `SelfCloaked` / `SelfMining` | `literal_false` | Cloak and mining overworld context are not on the default ship-vs-hostile path. |
| `CargoEmpty` / `EnemyNotToaTrialHostile` | `literal_true` | Lenient non-blockers (cargo and ToA trial tags not modeled). |
| `CargoFull` / `EnemyStronger` / `HitEnemyWithEnergy` / `HitEnemyWithKinetic` | `literal_false` | Conservative when the engine has no cargo, strength comparison, or per-hit weapon-type gate. |

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