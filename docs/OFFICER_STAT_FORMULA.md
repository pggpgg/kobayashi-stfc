# Officer-stat formula (STFC)

Canonical reference for how per-officer Attack / Defense / Health stats sum across a crew and feed into a ship's effective combat stats. Authored by the maintainer; consumed by the engine extension that adds first-class officer A/D/H runtime support (see `/Users/pgagnong/.claude/plans/what-should-we-work-recursive-lobster.md`).

Status: **specified and implemented**. The formula below is locked from in-game observation (Cerritos / Realta+Ghrush experiments), and the engine path is live: Phases 1–4 have shipped (per-side accumulators, breakpoint wiring, `officerstat*` ability modifiers, and Phase 4d dynamic 3-axis per-round gates including attack, defense, and health). The LCARS→engine routing is covered by [`tests/officer_stat_calibration_anchors.rs`](../tests/officer_stat_calibration_anchors.rs), [`tests/officer_kirk_morale_stat.rs`](../tests/officer_kirk_morale_stat.rs), [`tests/officer_round_start_stat.rs`](../tests/officer_round_start_stat.rs), and the unit tests in [`src/data/profile.rs`](../src/data/profile.rs). The one remaining gap is the **in-game expected damage deltas** for the three anchor cases in the "Examples to validate" section below (still `_TBD_` — they need observed numbers to convert the anchor tests from direction/routing checks into exact-magnitude calibration).

---

## 1. Crew slot weights

**Slot position does not affect officer-stat weighting.** Every crewed officer contributes their A/D/H at the same weight regardless of whether they sit in the captain seat, a bridge seat, or a below-decks seat. The captain seat still grants its captain ability (in addition to its bridge ability), and below-decks seats still grant only their below-decks ability — those are ability-firing rules, not stat-weighting rules.

| Slot | Attack weight | Defense weight | Health weight |
|---|---|---|---|
| Captain | 1.0 | 1.0 | 1.0 |
| Bridge officer 1..N | 1.0 | 1.0 | 1.0 |
| Below-decks 1..N | 1.0 | 1.0 | 1.0 |

An officer's A/D/H values themselves are determined primarily by **officer level**, resolved against the LCARS `stats` array via `resolve_level()` / `stats_at_level()` ([src/lcars/parser.rs:54-73](../src/lcars/parser.rs)). Per-side totals are therefore:

```
A_sum = Σ over crewed officers of stats_at_level(officer.level).attack
D_sum = Σ over crewed officers of stats_at_level(officer.level).defense
H_sum = Σ over crewed officers of stats_at_level(officer.level).health
```

(Where "crewed officers" = captain + bridge slots + below-decks slots actually filled for the engagement.)

---

## 2. Sum → ship-stat formula

Renaming the per-side sums for clarity, matching the in-game terminology:

```
attack_rating  = Σ crewed officer Attack
defense_rating = Σ crewed officer Defense
health_rating  = Σ crewed officer Health
```

**Each rating produces two effects: a percentage bonus via per-ship breakpoints, AND a raw additive contribution to the matching ship stat.**

### 2a. Breakpoint mapping (per ship, per rating)

Each ship has three breakpoint tables — one each for attack/defense/health — that map a rating value to a bonus percentage.

**The mapping is a step function**, not piecewise-linear interpolation: when the rating crosses a breakpoint value, the bonus jumps to that breakpoint's value and holds until the next breakpoint is crossed. Below the first breakpoint, the bonus is 0.

```
bonus(rating) = max({ bp.bonus for bp in table if rating >= bp.value }, default=0)
```

**Source data — already in upstream cache, NOT yet in ships_extended.** Every ship JSON under [data/upstream/data-stfc-space/ships/<id>.json](../data/upstream/data-stfc-space/ships/) carries:

```json
"officer_bonus": {
  "attack":  [{"value": 700, "bonus": 0.4}, {"value": 1400, "bonus": 0.8}, ..., {"value": 21000, "bonus": 4.0}],
  "defense": [...],
  "health":  [...]
}
```

(Concrete example from ship `1027217748.json`: 10 breakpoints, max at rating 21,000 → bonus 4.0 = 400%. The Cerritos has its own table topping out at 45,000 → 500%.)

**Engine implication:** [src/bin/normalize_data_stfc_space.rs](../src/bin/normalize_data_stfc_space.rs) must be extended to copy `officer_bonus` from the upstream JSON into the normalized `data/ships_extended/<id>.json` schema. Today the normalized format ([data/ships_extended/admonition.json](../data/ships_extended/admonition.json) etc.) has only `id, ship_name, ship_class, faction, tiers, levels, crew_slots, abilities` — `officer_bonus` is dropped during normalization. Re-running the normalizer is part of Phase 1.

### 2b. Attack — bonus is a multiplier on the weapon damage channel only

`attack_rating` is purely a rating value consumed by the breakpoint lookup. **It does not add flat damage anywhere.** The `(N Damage)` string in the tooltip (e.g. *"Your Officers provide an Attack Bonus of 100% (91,354 Damage)"*) is informational — it just echoes the rating value (= Σ crewed officers' Attack stats), not a damage delta added to the round.

The full formula:

```
per_round_damage = ship_weapon_damage × (1 + W) × (1 + attack_bonus)
                   + flat_additives_unaffected_by_attack_bonus
```

where:
- `W` = profile's `weapon_damage` multiplier (research + buildings + forbidden tech)
- `attack_bonus` = breakpoint lookup on `attack_rating` (per §2a)
- `flat_additives` = isolytic damage + research/building flat-attack additives (per the user's §2 insight about small ships); these are unaffected by `attack_bonus`

**Empirical confirmation (Realta T4 L20 controlled experiment):**

| Setup | crew | attack_rating | attack_bonus | damage/round | Δ |
|---|---|---|---|---|---|
| 1 | none | 0 | 0% | 7,069 | — |
| 2 | Ghrush L30 alone | 91,354 | 100% | 7,218 | **+149** |

The delta is **149 damage/round**, not 91,354 × 2 shots = 182,708. So `attack_rating` is *not* added per shot. Decomposing: `ship_weapon × (1 + W) × 1 + flat = 7069` and `ship_weapon × (1 + W) × 2 + flat = 7218`, subtracting gives `ship_weapon × (1 + W) = 149`. With Realta raw weapon = 88, that yields `(1 + W) ≈ 1.69` (W ≈ 69%, reasonable from research/buildings), and `flat ≈ 6,920`. Consistent.

The Cerritos + Ghrush observation ("6.08M damage/round at 500% bonus") is also consistent — the larger absolute numbers reflect Cerritos' much higher base weapon damage, not any per-shot mechanic.

### 2c. Defense — bonus × per-ship channel constant, routed by ship class

`defense_rating` selects a `defense_bonus` via breakpoints. The contribution is **`ship.mit_per_bonus[channel] × defense_bonus`**, additive to the ship's primary mitigation channel, where the channel is determined by ship class:

| Ship class | channel |
|---|---|
| Battleship | `armor` |
| Explorer | `shield_deflection` |
| Interceptor | `dodge` |
| Survey | even thirds: ⅓ armor + ⅓ shield_deflection + ⅓ dodge |

**Resolved during Phase 1.** The Cerritos anomaly led to discovering a wrong `HULL_TO_CLASS` mapping in [scripts/build_ship_registry.py](../scripts/build_ship_registry.py): hull_types 0 / 2 / 3 were all incorrectly labeled. The fixed mapping (verified against 10 spot-checked ships across all 4 classes — Borg Cube, D'Deridex, U.S.S. Crozier as battleship; Jellyfish, Enterprise NX-01, U.S.S. Cerritos as explorer; Gorn Eviscerator, D4 Class, SS Revenant as interceptor; Nova as survey) is:

```python
HULL_TO_CLASS = {0: "interceptor", 1: "survey", 2: "explorer", 3: "battleship", 4: "survey", 5: "survey"}
```

Both `data/upstream/data-stfc-space/ship_id_registry.json` and all 113 `data/ships_extended/<id>.json` files have been regenerated with the correct classes.

**Per-ship channel constants are taken from existing upstream component fields:**

| Ship class | channel | upstream field | engine stat |
|---|---|---|---|
| Battleship | armor | `Armor.plating` (tier component) | `armor` (extracted) |
| Explorer | shield_deflection | `Shield.absorption` (tier component) | `shield_deflection` (extracted) |
| Interceptor | dodge | `Impulse.dodge` (tier component) | `dodge` (extracted) |
| Survey | even thirds | matching field per channel | each routed via the rule above |

Naming: the in-game stat is **Shield Deflection**, and the engine's `shield_deflection` field names it directly. Upstream's `Shield.absorption` is that platform's legacy field name for the same stat. The upstream `Deflector.deflection` field — a stale constant `120` on every ship — maps to no in-game concept; an earlier normalizer version read it, and [src/bin/normalize_data_stfc_space.rs](../src/bin/normalize_data_stfc_space.rs) now ignores it and sources `shield_deflection` from `Shield.absorption` per tier. Verified for the Cerritos: tier 12 = **13,338**, exactly matching the observed defense-channel constant (anchored by `cerritos_tier12_shield_deflection_matches_in_game_observation` in [tests/data_provenance_tests.rs](../tests/data_provenance_tests.rs); `validate_ships_extended_dataset` errors on the stale-120 signature).

**Defense additive routes to the primary mitigation stat** (not to `shield_mitigation`):
- explorer: `shield_deflection += ship.shield_deflection × defense_bonus`
- battleship: `armor += ship.armor × defense_bonus`
- interceptor: `dodge += ship.dodge × defense_bonus`
- survey: ⅓ of `defense_bonus` applied to each channel above with the matching ship constant.

Empirical confirmation (Cerritos, both observations):

| Crew | defense_rating | defense_bonus | shield_added | implied `shield_per_bonus` |
|---|---|---|---|---|
| Sesha L15 | 4,374 | 100% (1.0) | 13,338 | **13,338** |
| Ghrush L30 | 93,209 | 500% (5.0) | 66,690 | **13,338** |

Both rows imply identical `shield_per_bonus = 13,338` for the Cerritos. **Confirmed:** `mitigation_added = ship.mit_per_bonus[channel] × defense_bonus`. `defense_rating` itself only feeds the breakpoint lookup.

**Critical:** `defense_rating` (and its bonus) **do not** influence `shield_mitigation` (the multiplicative damage-after-shield knob). An earlier `shield_mitigation += officer_defense` line in [src/data/profile.rs](../src/data/profile.rs) was incorrect per this spec and has been removed (see the §2c comment in `apply_profile_to_attacker`); officer Defense now routes exclusively through the breakpoint + ship-class-mitigation pathway.

### 2d. Health — bonus × per-ship hull/shield constants

`health_rating` selects a `health_bonus` via breakpoints. The contribution is:

```
hull_added   = ship.hull_per_bonus   × health_bonus
shield_added = ship.shield_per_bonus × health_bonus
```

Empirical confirmation (Cerritos, three observations across three bonus tiers):

| Crew | health_bonus | hull_added | implied `hull_per_bonus` | shield_added | implied `shield_per_bonus` |
|---|---|---|---|---|---|
| Sesha L15 | 0% (0.0) | 0 | — (trivially consistent) | 0 | — |
| Chen | 250% (2.5) | 435,133 | **174,053** | 589,361 | **235,744** |
| Ghrush L30 | 500% (5.0) | 870,265 | **174,053** | 1,178,721 | **235,744** |

Cerritos' constants are exact and stable across bonus tiers: `Cerritos.hull_per_bonus = 174,053` and `Cerritos.shield_per_bonus = 235,744`. Hull/shield ratio shield/hull ≈ 1.354 — presumably ship-type-specific (Cerritos is shield-routed for Defense, so it makes sense for it to favor shield in Health as well).

### 2e. The "post-crew" delta is just the player profile's existing officer_* bonuses

Earlier mystery — Ghrush gained "+1148" on every stat when crewed, Sesha gained "+22/+54/+10". Normalized:

| Officer | ΔA / base_A | ΔD / base_D | ΔH / base_H |
|---|---|---|---|
| Sesha L15 | +1.29% | +1.25% | +1.21% |
| Ghrush L30 | +1.27% | +1.25% | +1.27% |

Uniform **~+1.25% multiplicative bonus** on each per-officer stat *before* aggregation into ratings. This is consistent with the player profile having an `officer_attack` / `officer_defense` / `officer_health` bonus of ~0.0125 each from research/syndicate/buildings — the existing profile-bonus pathway. The "displayed officer card" stat is the raw L15/L30 LCARS value; the "after crewing" stat is `raw × (1 + officer_bonus_from_profile)`.

**Engine implication for Phase 2:** the existing `officer_attack` / `officer_defense` / `officer_health` profile keys keep their semantics, but they now apply to **per-officer A/D/H before aggregation** rather than to **ship-derived stats**. Concretely:

```
per_officer_A_effective = officer.lcars_stats.attack × (1 + profile.officer_attack)
                          × (1 + ability_buffs_to_officer_attack)
attack_rating           = Σ per_officer_A_effective
attack_bonus            = breakpoint_lookup(ship, attack_rating)
ship.effective_attack   = ship.base_attack × (1 + weapon_damage_buff) × (1 + attack_bonus)
damage_per_round_raw   += attack_rating          // new additive channel
```

This **answers Section 4**: the existing `officer_*` profile keys are **kept**, with **migrated semantics** (pre-aggregation multiplier on per-officer stats, not post-aggregation multiplier on ship stats).

### 2f. Things that are explicitly NOT affected

- `shield_mitigation` (the multiplicative damage-through-shield knob) is **not** influenced by officer Attack / Defense / Health. Only by `shieldmitigation` tags from officer abilities and the corresponding profile bonus key.
- Other engine stats (crit_chance, crit_damage, pierce, accuracy) are not influenced by officer A/D/H.

---

## 3. `target: self` vs `target: enemy` scoping for officer-stat buffs

**Scope: crew-wide, pre-sum.** Officer-stat buffs apply to every crewed officer's A/D/H on the target side, *before* the per-side ratings are summed.

```
for officer in target_side.crewed_officers {
    officer.effective_attack  *= (1 + Σ buffs_to_officer_attack)
    officer.effective_defense *= (1 + Σ buffs_to_officer_defense)
    officer.effective_health  *= (1 + Σ buffs_to_officer_health)
}
attack_rating  = Σ officer.effective_attack
defense_rating = Σ officer.effective_defense
health_rating  = Σ officer.effective_health
```

Examples:

- **Cadet Kirk captain "Motivational" `target: self +8%`** — every crewed officer on Kirk's side gets +8% to each of A/D/H. Kirk is one of the affected officers (his +8% applies to himself like to anyone else).
- **Kras bridge "Know Your Enemy" `target: enemy_bridge -20%`** — captain + the two bridge officers on the *defender's* side get -20% to each of A/D/H; below-decks officers are unaffected. (On the player-vs-hostile path the defender is a hostile with no crewed officers — no-op.)
- **Marla bridge "Let Me Help You" `target: self +50%`** — same crew-wide-pre-sum scope as Kirk's, just larger.

**The scoping rule is the same regardless of trigger phase.** Passive, on_combat_start, on_round_start, conditional — all apply to per-officer A/D/H before the rating sum. Differences between triggers only affect *when* the buff is active, not *who* it covers.

**Hostile-side defender: no-op.** When the defender is an NPC hostile, `target: enemy` officer-stat buffs apply to nothing — hostiles don't have crewed officers in the LCARS sense. Kras's "Know Your Enemy" is PvP-only by design (vs player ships and player stations); the engine treating it as a no-op on PvE matches the in-game behavior.

Phase 3 wiring: when `target: enemy` resolves to `DefenderOpponent` on a PvE engagement, the modifier silently drops. No special drop_report entry needed — it's the documented intended behavior, not a coverage gap.

---

## 4. Existing `officer_attack` / `officer_defense` / `officer_health` profile bonus keys

**Decision: KEEP the keys, MIGRATE the semantics.** The Section 2e finding ("crewing an officer multiplies their LCARS stats by ~1.25% from the player profile") is exactly what these profile bonuses are — the existing values from Syndicate Reputation / research / buildings are correct **inputs**; only their application point changes.

**Today (incorrect):** applied post-aggregation, on ship combat stats:

```rust
// src/data/profile.rs:1938-1945
attack            = ship.base_attack       × (1 + officer_attack)
hull_health       = ship.base_hull_health  × (1 + officer_health)
shield_mitigation = ship.base_shield_mit + ... + officer_defense   // WRONG: officer Defense
                                                                    // does not affect shield_mitigation
```

**After this change:** applied pre-aggregation, on each crewed officer's LCARS A/D/H:

```rust
for officer in crewed_officers {
    officer.effective_attack  = officer.lcars_stats.attack  × (1 + profile.officer_attack)
    officer.effective_defense = officer.lcars_stats.defense × (1 + profile.officer_defense)
    officer.effective_health  = officer.lcars_stats.health  × (1 + profile.officer_health)
}
attack_rating  = Σ officer.effective_attack    // then feeds the §2b channels
defense_rating = Σ officer.effective_defense   // then feeds the §2c channels
health_rating  = Σ officer.effective_health    // then feeds the §2d channels
```

**Producers that already feed these keys keep working unchanged:**

- Syndicate Reputation ([src/data/syndicate_combat.rs:50-56](../src/data/syndicate_combat.rs))
- Research ([src/data/research.rs:668-685](../src/data/research.rs))
- Buildings ([src/data/profile.rs:1287](../src/data/profile.rs))

**Removed:** the `shield_mitigation += officer_defense` line at [src/data/profile.rs:1945](../src/data/profile.rs). Officer Defense never had any business there.

---

## Examples to validate the spec against

These three anchors are tied to their real production officers and exercised by
[`tests/officer_stat_calibration_anchors.rs`](../tests/officer_stat_calibration_anchors.rs). That
test currently asserts the **LCARS→engine routing and the direction** of each effect (and records
the engine's predicted bonus in its assertion messages). The **expected in-game damage delta** for
each is still `_TBD_` — supply observed numbers (plus the exact ship + crew used) to upgrade these
from direction/routing checks to exact-magnitude calibration. After the snapshot freeze, record
anchor fights with `officer_anchor` set to `kirk`, `marla`, or `kras` in
[`recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json) and
copy observed damage deltas into this section and the manifest `bands`.

- Cadet Kirk `cadet-kirk-a80563` captain "Motivational" +8% all stats (crew-wide, `target: self`),
  vs a fixed hostile: expected damage delta = _TBD_. *(Routing locked: `officer_stat_all += 0.08`.)*
- Marla `marla-9732c7` "Let Me Help You" +50% all stats (bridge, crew-wide, `target: self`),
  captained: expected damage delta = _TBD_. *(Routing locked: `officer_stat_all += 0.50`.)*
- Kras `kras-a47042` "Know your Enemy" -20% all stats on the enemy bridge
  (`target: enemy_bridge`, gated on `DefenderIsPlayerShip` → no-op vs NPC hostiles, active in PvP):
  expected damage delta = _TBD_. *(Routing locked: enemy-bridge pending contribution, value 0.20.)*

These are the calibration anchors. The formula is empirically pinned (Cerritos / Realta+Ghrush) and
implemented; the anchors above are what convert "implemented" into "confirmed against the live game."

---

## Phase 2b — formula resolved by the Realta T4 L20 + Ghrush experiment

The "Tiny ship + biggest officer" experiment definitively pinned the attack formula. The §2b open question (whether `attack_rating` adds per-shot raw damage) is resolved as **no** — `attack_rating` is purely a rating consumed by the breakpoint lookup, and `attack_bonus` is the only mechanism by which officer Attack affects damage. See updated §2b above.

Phase 2b can now proceed with the simpler model:
- `attack_bonus` → multiplier on weapon damage channel
- `defense_bonus` → additive to ship-class-primary mitigation stat (already confirmed in §2c)
- `health_bonus` → multiplier on hull AND shield HP (already confirmed in §2d)
- No `bonus_damage_per_shot` field, no per-shot raw damage in the engine.

---

## Phase 4d — per-round officer-stat conditions (3-axis breakpoint path) — **completed (2026-06-16)**

Officer-stat rows whose LCARS `trigger` is **`on_round_start`**, or whose `condition` depends on **round state** (morale, hull breach, burning, `round_range`, …), cannot be evaluated only at fight setup.

**Resolver path** ([`collect_dynamic_officer_stat_contributions`](../src/lcars/resolver.rs)): per-round `officerstat*` rows are stored on [`BuffSet::dynamic_officer_stat_contributions`](../src/lcars/resolver.rs) with a compiled runtime [`AbilityCondition`](../src/combat/abilities.rs) when present (including duration `RoundRange` when present). **`on_combat_start`** rows with fight-setup-only gates (Dezoc armada+faction, Kras PvP, …) stay in [`PendingOfficerStatContribution`](../src/lcars/resolver.rs).

**Combat path** ([`OfficerStatRoundContext`](../src/data/officer_stat_round.rs)): each round after morale/state gates refresh, active per-round rows are merged into [`compute_officer_stat_runtime_bonus_with_round`](../src/data/profile.rs) and the delta vs fight-setup baseline is applied:

- [x] **Attack:** `attack_pre_mult_add` → outbound `pre_attack_multiplier` (proper breakpoint lookup, not a flat `weapon_damage` proxy).
- [x] **Defense:** additive armor / shield_deflection / dodge on inbound counter-fire mitigation.
- [x] **Health:** `health_max_mult` scales max hull/shield at round start; absolute remaining HP is preserved when the gate expires at round end (proportional apply on activation — assumption pending in-game confirmation).

**Production examples:**

| Officer | Row | Path |
|---|---|---|
| [`kirk-1323b6`](../data/officers/officers.lcars.yaml) captain "Leader" | `officerstatall` +40%, `morale_active`, `on_round_start`, 1R | Per-round dynamic (morale gate) |
| [`kumak-c5b0db`](../data/officers/officers.lcars.yaml) captain "Discipline" | `officerstatall` +5%, unconditional `on_round_start` | Per-round dynamic |
| [`strike-team-una-5ec6f6`](../data/officers/officers.lcars.yaml) captain "Team Profiling" | `officerstatall`, `on_round_start`, PvP + defender Explorer | Per-round dynamic (static gates compiled to runtime) |
| [`dezoc-381416`](../data/officers/officers.lcars.yaml) bridge "Chokepoint" | `officerstatall`, `on_combat_start`, solo_armadas + defender hull faction | Pending at fight setup (not Phase 4d) |

Kirk bridge "Inspirational" morale proc is separate (standard `Morale` seat).

**Tests:** [`tests/officer_kirk_morale_stat.rs`](../tests/officer_kirk_morale_stat.rs); [`tests/officer_round_start_stat.rs`](../tests/officer_round_start_stat.rs); resolver unit tests `phase4d_*` in [`src/lcars/resolver.rs`](../src/lcars/resolver.rs); [`src/data/officer_stat_round.rs`](../src/data/officer_stat_round.rs).

**Still deferred:** PvP defender-side dynamic `target: enemy` officer-stat debuffs (no prod LCARS cases today).
