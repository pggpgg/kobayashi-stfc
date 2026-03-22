# Combat trace: reading mitigation and stacking

This document ties Kobayashi’s JSON trace events to **why** a shot dealt a given amount of damage. It complements [`combat_log_format.md`](combat_log_format.md) (log vocabulary) and the implementation plan’s Phase 4 explainability goals.

## Mitigation and pierce (per shot)

For each weapon sub-round, the engine records:

1. **`mitigation_calc`** (phase `defense`)  
   - **`mitigation`**: defender’s scalar mitigation for this combatant (already includes hostile floor/ceiling and pre-combat math when the scenario built the defender).  
   - **`multiplier`**: `max(0, 1 - mitigation)`. This is the fraction of *pre-pierce* damage that would remain if only that scalar applied.

2. **`pierce_calc`** (phase `attack`)  
   - **`pierce`**: effective additive pierce for this round (ship base + pre-attack pierce bonuses + morale primary-piercing when it fired).  
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

**When you need a table, not a single fight:** use the CLI `kobayashi mitigation-sensitivity <ship_id> <hostile_id> [--delta-pct <f64>]` (from the project root, with data loaded — ids are the same as in `data/ships_extended` / `data/hostiles`, e.g. `uss_enterprise` and `2918121098` (data.stfc.space numeric hostile id)), or the library helpers in [`src/combat/mitigation_sensitivity.rs`](../src/combat/mitigation_sensitivity.rs) to sweep baseline stats with small deltas.

## Officers: one seat each

The game does not allow duplicate officers on a crew. The optimizer and [`resolve_crew_to_buff_set`](../src/lcars/resolver.rs) enforce **at most one contribution per officer id** (captain, then bridge order, then below decks). [`apply_duplicate_officer_policy`](../src/combat/abilities.rs) drops duplicate `officer_id` groups if malformed input ever reaches the engine.
