# Canonical officer `conditions` tokens

`data/officers/officers.canonical.json` stores PascalCase `conditions` on abilities. `generate_lcars` maps each token to an LCARS `condition` tree when the combat engine can evaluate it; unmapped tokens log `skipping unmapped canonical condition` and the emitted ability may be **weaker** than in-game (subset `and` of only the mapped arms).

See [DESIGN.md](DESIGN.md) §3.4 for LCARS condition types, `[map_canonical_condition_token](../src/lcars/canonical_conditions.rs)`, and `[resolve_lcars_condition](../src/lcars/resolver.rs)`.

## Triage (frequency × feasibility)

The table below lists **currently unmapped** tokens (57 unique strings in canonical; **39 remain unmapped** after armada / `not` / attacker breach-burn / Tal-bridge mappings). Counts are occurrences in `officers.canonical.json` (not officer cardinality).


| Token                                                                                                              | Count         | Bucket   | Note                                                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------ | ------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TargetNotASB`                                                                                                     | 27            | deferred | No “anti-station” / ASB scenario in ship-vs-hostile core sim.                                                                                                          |
| `EnemyHullFaction`                                                                                                 | 25            | deferred | Needs explicit hostile faction + hull-line semantics beyond `OpponentFactionTag`.                                                                                      |
| `TargetMaxLevel`                                                                                                   | 21            | deferred | Hostile level gate not in `CombatContext`.                                                                                                                             |
| `SelfDefending`                                                                                                    | 16            | deferred | Station / defense context not modeled.                                                                                                                                 |
| `CombatBattleType`                                                                                                 | 15            | deferred | No battle-type enum in scenario (would mis-gate if approximated).                                                                                                      |
| `ModuleKinetic` / `ModuleEnergy`                                                                                   | 11 / 8        | deferred | Weapon module context not in condition evaluation.                                                                                                                     |
| `EnemySentinel`                                                                                                    | 11            | deferred | Sentinel encounter type not represented.                                                                                                                               |
| `SelfAtStation` / `SelfAtSoloArmada` / `SelfAtWaveDefenseChallenge` / `SelfAtAssault2`                             | 8 / 8 / 3 / 1 | deferred | Encounter location / mode not in `CombatContext`.                                                                                                                      |
| `CombatGameContext`                                                                                                | 8             | deferred | Overworld / client context.                                                                                                                                            |
| `TargetHasAssimilated`                                                                                             | 7             | deferred | Assimilate debuff is tracked for **attacker** effectiveness, not exposed as “target has assimilated” on defender; mapping would need a defined PvE/PvP semantics pass. |
| `TargetStateAny`                                                                                                   | 7             | deferred | Composite state bundle not modeled as one predicate.                                                                                                                   |
| `SelfAttacking`                                                                                                    | 6             | deferred | Ambiguous vs default PvE attacker role.                                                                                                                                |
| `HullHealthBelowStartOfCombat` / `HullHealthBelow` / `HullHealthAbove`                                             | 4 / 3 / 1     | deferred | Canonical JSON does not carry thresholds per token; values live in prose — do not invent cutoffs.                                                                      |
| `TargetIsArmadaOrInvadingEntity` / `TargetIsInvadingEntity` / `TargetNotInvadingEntity` / `TargetNotPlayerStation` | 3 / 2 / 1 / 3 | deferred | Invading entity / station predicates not modeled.                                                                                                                      |
| `SelfHull`* / `SelfMining` / `SelfCloaked`                                                                         | various       | deferred | Player hull identity / mining / cloak not in `CombatContext`.                                                                                                          |
| `SelfStateNone`                                                                                                    | 2             | deferred | “No state” bundle not modeled.                                                                                                                                         |
| `HitEnemyWithEnergy` / `HitEnemyWithKinetic`                                                                       | 1 each        | deferred | Per-hit weapon type context not available at generic condition evaluation.                                                                                             |
| `EnemyStronger`                                                                                                    | 1             | deferred | Power comparison not in context.                                                                                                                                       |
| `CargoFull` / `CargoEmpty`                                                                                         | 1 each        | deferred | Cargo not modeled.                                                                                                                                                     |
| `EnemyNotToaTrialHostile`                                                                                          | 1             | deferred | Trial-specific hostile tag not modeled.                                                                                                                                |


### Task 2 audit (engine-ready mappings only)

Re-checked **unmapped** tokens from `cargo run --bin report_unknown_mappings` against resolver / `AbilityCondition` evaluation. `**SelfHasHullBreach`** and `**SelfHasBurning`** were later implemented: `[CombatContext](../src/combat/abilities.rs)` exposes `attacker_hull_breach_active` / `attacker_burning_active`, counter and receive-damage paths update attacker debuff counters, and canonical maps them to LCARS `attacker_hull_breach` / `attacker_burning`. `**SelfOfficerTalNotOnBridge`** maps to LCARS `attacker_officer_tal_not_on_bridge` / `[AbilityCondition::AttackerOfficerTalNotOnBridge](../src/combat/abilities.rs)` using `attacker_tal_assigned_captain_or_bridge` on `[CombatContext](../src/combat/abilities.rs)` (derived from attacker `[CrewConfiguration](../src/combat/abilities.rs)` Captain and Bridge seats vs `[TAL_OFFICER_LCARS_ID](../src/combat/abilities.rs)`). See `[task2_deferred_tokens_remain_unmapped](../src/lcars/canonical_conditions.rs)` for the still-deferred set.

Regression: `[task2_deferred_tokens_remain_unmapped](../src/lcars/canonical_conditions.rs)` asserts the remaining deferred tokens stay unmapped until engine work lands; remove a token from that test when you add a deliberate mapping.

## Mapped today (reference)

Handled in `[map_canonical_condition_token](../src/lcars/canonical_conditions.rs)`: `TargetHasBurning`, `TargetHasHullBreach`, `SelfHasBurning` (LCARS `attacker_burning`), `SelfHasHullBreach` (LCARS `attacker_hull_breach`), `SelfHasMorale`, `SelfOfficerTalNotOnBridge` (LCARS `attacker_officer_tal_not_on_bridge`), `EnemyHostile`, `EnemyPlayer`, `EnemyArmada`, `**TargetIsArmada`** (same as `EnemyArmada`), `**TargetNotArmada`** (LCARS `not` around `defender_ship_type_is` `armada`), `Enemy{Explorer,Battleship,Interceptor,Survey,Surveyor,Armada}`, `Self{…}` hull classes (explorer, …; not `SelfHas`* breach/burn, which are explicit tokens above).

## Updates (engine + LCARS)

- `**not`**: `[AbilityCondition::Not](../src/combat/abilities.rs)` + LCARS `type: not` with exactly one child (`[resolve_lcars_condition](../src/lcars/resolver.rs)`).
- `**TargetIsArmada` / `TargetNotArmada`**: canonical aliases for armada gating (see roadmap “canonical condition tokens”).

## Regenerate unknown-mappings report

Machine-readable Markdown (unmapped canonical tokens + hostile `upstream_ship_type` coverage from `data/hostiles/index.json`):

```bash
cargo run --bin report_unknown_mappings -- --output path/to/report.md
```

Omit `--output` to print to stdout. Paths default to `data/officers/officers.canonical.json` and `data/hostiles/index.json` (relative to the crate root). Run `cargo run --bin report_unknown_mappings -- --help` for flags.