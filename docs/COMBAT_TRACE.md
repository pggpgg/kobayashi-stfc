# Combat trace: reading mitigation and stacking

This document ties Kobayashi’s JSON trace events to **why** a shot dealt a given amount of damage. It complements [`combat_log_format.md`](combat_log_format.md) (log vocabulary) and the implementation plan’s Phase 4 explainability goals.

## Mitigation and pierce (per shot)

For each weapon sub-round, the engine records:

1. **`mitigation_calc`** (phase `defense`)  
   - **`mitigation`**: defender’s scalar mitigation for this combatant (clamped when hostile floor/ceiling apply).
   - **`multiplier`**: `max(0, 1 - mitigation)`. This is the fraction of *pre-pierce* damage that would remain if only that scalar applied.
   - When hostile mitigation params are present (`hostile_mitigation_params`), trace also includes decomposition fields:
     - **ratios**: `armor_ratio`, `shield_ratio`, `dodge_ratio`
     - **component terms**: `f_armor`, `f_shield`, `f_dodge`
     - **weights and weighted terms**: `c_armor`, `c_shield`, `c_dodge`, `weighted_armor`, `weighted_shield`, `weighted_dodge`
     - **mystery + clamp context**: `mystery_mitigation_factor`, `one_minus_mystery`, `mitigation_raw`, `mitigation_floor`, `mitigation_ceiling`

1b. **`mitigation_calc`** (phase `counter`)
   - Defender counter-fire emits player-side mitigation composition for that sub-round:
     - `base_mitigation`
     - `mitigation_additive_bonus`
     - `dodge_bonus`, `dodge_coefficient`, `dodge_mitigation_bonus`
   - Includes the same scalar `mitigation` and `multiplier` fields used by outbound shots.
   - When raw defense stats are available, `base_stat_mitigation` retains the stable
     six-decimal display. `base_stat_mitigation_unrounded` preserves the calculated
     `f64` for replay comparisons with narrower observed intervals. This extra field
     does not change damage, RNG draws, or the existing trace values. For older traces,
     account for display rounding before comparing an interval; a rounded point alone
     cannot establish agreement with a narrower bound.

2. **`pierce_calc`** (phase `attack`)  
   - **`pierce`**: effective additive pierce for this round (ship base + pre-attack pierce bonuses, ×1.10 when Morale fired — Morale also lowers per-shot dynamic mitigation via +10% on all piercing stats).  
   - **`damage_through_factor`**: how much of the attack **gets through** mitigation for damage scaling:  
     `(mitigation_multiplier + pierce + defense_mitigation_bonus).max(0)`  
     where `defense_mitigation_bonus` comes from defense-phase effects on the attacker’s crew (see [`src/combat/damage.rs`](../src/combat/damage.rs)).  
   - Values **can exceed 1.0** when pierce and bonuses add enough on top of `(1 - mitigation)`.

**Reading “why did this much damage get through?”**

- Start from `mitigation_calc.multiplier` (how much would be kept after raw mitigation).
- Then add `pierce_calc.pierce` and any defense-phase mitigation bonus (not always emitted as its own event; when zero, trace only shows the combined `damage_through_factor`).  
- Compare `pierce_calc.damage_through_factor` before/after a round or between runs to see sensitivity to pierce or mitigation changes.

**Uncertainty:** In-game ordering labels may differ; Kobayashi matches the mechanistic pipeline in [`src/combat/engine.rs`](../src/combat/engine.rs).

## Attack scaling (pre-shot)

- **`attack_roll`**: `base_attack`, `effective_attack` after pre-attack multipliers from stacking (`EffectAccumulator::pre_attack_multiplier` and related).  
- **`crit_resolution`**: crit roll, hull breach interaction, resulting multiplier.

Net hull/shield damage also applies apex barrier/shred and shield split; see events around damage application in the same trace.

### Weapon HP removal versus overkill

Outbound `damage_application` events distinguish damage assigned by the model
from HP actually removed on that hit:

| Field | Meaning |
| --- | --- |
| `damage_after_apex` | Resolved weapon pool before shield routing and HP caps. |
| `hull_damage` | Assigned hull share, including overkill. |
| `shield_damage` | Shield HP removed, capped by shields remaining before the hit. |
| `applied_weapon_damage` | Shield HP removed plus hull share capped by hull remaining before the hit. |
| `applied_isolytic_damage` | Proportional isolytic share of applied weapon HP damage; equals all applied weapon damage under Isolytic Vulnerability. |
| `hull_overkill_damage` | Assigned hull damage beyond available hull HP. |
| `discarded_shield_damage` | Shield assignment beyond available shield HP, discarded on that hit. |

For each hit, `applied_weapon_damage + hull_overkill_damage +
discarded_shield_damage = damage_after_apex`, up to floating-point rounding.
Applied damage is measured before any subsequent repair or regeneration. These
are weapon-only gauges: burning and other non-weapon damage are separate events.
Summing them requires a complete trace; a replay with `trace_truncated=true`
cannot provide a complete fight total. Event tracing uses the scalar combat path.

The legacy result fields `total_damage` and `total_isolytic_damage` retain their
existing overkill accounting. They are not target HP-pool gauges, and the latter
is not interchangeable with the game's raw isolytic export column. Mixed standard
and isolytic attribution is a proportional accounting convention after their
shared routing, not an independently verified server rule.

The modeled isolytic leg uses the weapon damage before conventional mitigation,
then applies isolytic defense. The standard leg uses conventional mitigation;
the final Apex factor applies to their combined pool. This follows Scopely's
[isolytic description](https://startrekfleetcommand.com/news/update-55-voyager-part-2/)
and [Apex description](https://startrekfleetcommand.com/news/new-battle-stat-apex-barrier-modifier/).
These modeled values do not by themselves reproduce the game's raw telemetry columns.

## Stack resolution (crew buffs)

Static and timed effects are composed in [`src/combat/stacking.rs`](../src/combat/stacking.rs) (base → modifier → flat → multiply → cap). Per-round, [`EffectAccumulator`](../src/combat/effect_accumulator.rs) maps LCARS stats into keys such as:

- Pre-attack damage / pierce  
- Attack-phase damage modifiers  
- Defense mitigation bonus (feeds `damage_through_factor`)  
- Apex, regen, isolytic, etc.

Ability activation traces (`ability_activation` / related event types) show **which** abilities were considered for a timing window; the numeric composition for a stat is the composed stack for that key, not always a separate per-contributor breakdown in the trace.

### `stack_resolution` (per shot, trace mode)

After pre-attack damage is folded into the stacking model and attack-phase damage is composed, the engine emits **`stack_resolution`** (phase `attack`, `weapon_index` set when applicable). Fields include:

- **`pre_attack_multiplier`**, **`attack_phase_damage_multiplier`**, **`round_end_damage_multiplier`** — channel-level multipliers (`1 +` sum of `AttackMultiplier`-style contributions for that channel where applicable).  
- **`stacks`** — object keyed by stack name (e.g. `pre_attack_damage`, `defense_mitigation_bonus`). Each entry has **`base`**, **`modifier_sum`**, **`flat`**, and **`composed`** (`base * (1 + modifier_sum) + flat` per [`CategoryTotals`](../src/combat/stacking.rs)). Only stacks with any non-zero component are listed.  
- **`pre_attack_damage_composed`** / **`damage_after_attack_phase_compose`** — numeric results after the pre-attack and attack-phase channels respectively (before isolytic / apex on hull).
- **`effect_contributions`** — optional array of per-effect “why” lines (only present when trace mode is enabled). Each line captures one contribution in a stable machine-readable form:
  - **`source`**: `{ "ability": "<name>", "officer_id": "<id or null>" }` (ship abilities use `ability` names like `ship_ability:<id>`).
  - **`target`**: a string key identifying the accumulator field being changed (e.g. `stack:pre_attack_damage:flat`, `attack_phase_damage_modifier_sum`, `pre_attack_pierce_bonus`).
  - **`delta`**: the applied numeric change (already scaled/normalized to engine units).
  - **`timing`**: the [`TimingWindow`](../src/combat/abilities.rs) that applied the effect.

**Reading “why did my attack multiplier end up at X?”**

1. Find the `stack_resolution` event for the round/weapon.
2. Look at `stacks.<key>` for the math decomposition (`base`, `modifier_sum`, `flat`, `composed`).
3. Use `effect_contributions` to see which abilities moved which `target` fields, then reconcile those deltas with the final `stacks` totals.

**When you need a table, not a single fight:** use the CLI `kobayashi mitigation-sensitivity <ship_id> <hostile_id> [--delta-pct <f64>]` (from the project root, with data loaded — ids are the same as in `data/ships_extended` / `data/hostiles`, e.g. `uss_enterprise` and `2918121098` (data.stfc.space numeric hostile id)), or the library helpers in [`src/combat/mitigation_sensitivity.rs`](../src/combat/mitigation_sensitivity.rs) to sweep baseline stats with small deltas.

## Structured state snapshots (`state_snapshot`)

When `SimulationConfig.emit_state_snapshots` is enabled together with `TraceMode::Events`, the engine emits additional `state_snapshot` rows carrying a serialized [`CombatStateSnapshot`](../src/combat/snapshot.rs) under `values.snapshot` (hull/shield remaining, flags, optional attacker stacking summary). See [`combat_log_format.md`](combat_log_format.md) schema_version 3 and pairing rules for ingest.

## Officers: one seat each

The game does not allow duplicate officers on a crew. The optimizer and [`resolve_crew_to_buff_set`](../src/lcars/resolver.rs) enforce **at most one contribution per officer id** (captain, then bridge order, then below decks). [`apply_duplicate_officer_policy`](../src/combat/abilities.rs) drops duplicate `officer_id` groups if malformed input ever reaches the engine.
