# Structured audit: hostile `ability[]` coverage

This document expands on [ROADMAP.md](ROADMAP.md) §6 — hostile-ability coverage audit.

**Catalog revision (2026-07-16 / isolytic value-scale audit):** There are **982** unique upstream hostile ability ids across **2,901** hostiles with non-empty `ability[]`. The regenerated [`hostile_ability_catalog.json`](../data/upstream/data-stfc-space/hostile_ability_catalog.json) classifies all ids: **521** modeled for defender-side counter-fire (`defender_crew`), **461** `combat_noop`. Regenerator: `python3 scripts/generate_full_hostile_ability_catalog.py`.

**Isolytic value-scale audit (2026-07-16, backlog #13):** 157 catalog ids re-scoped, all inside the isolytic family (85 damage→damage, 70 defense→defense, 1 defense→`combat_noop`, 1 damage→`isolytic_cascade`); pinned in `tests/isolytic_value_scale.rs`. Ground-truthed conventions:

- **`%`-format placeholders are fractions regardless of the upstream flag.** `{0:0.#%}` renders the raw number ×100, so flag=true rows were being divided by 100 wrongly. "Something To Prove" (13 ids, 173 hostiles): 68.1 → +6,810% isolytic damage, previously 0.681 (100× too small).
- **Multi-stat texts reuse `values[0].chance` as placeholder `{1}`** (new catalog field `value_source: "chance"`; the seat's proc chance is then 100%). "Double Down" (13 ids, 173 hostiles): `value` 36,250 is the flat apex barrier (`{0:#.#}`), `chance` 16.25 is the isolytic defense fraction (`{1:0.#%}`) — the old seat used the barrier value as defense (+36,250% on flag=false rows). Also emits the barrier and the hardcoded crit-damage floor (100%) as extra seats; "Something To Prove" emits `apex_shred` from `chance` (+740%).
- **Hardcoded texts pin `value_override`.** Isolytic Dampeners bundles ("increases its Isolytic Defense by 1000%", 9 ids, ~190 hostiles incl. ACAD wave-defense drones): defense 10.0 (upstream values 0.5–10,000 were passed through raw or ÷100 — up to +1,000,000% on the Apex Defense variant, whose `{0:#}` value is actually a flat apex barrier, now a separate seat). Isolytic Simulator also emits `hostile_isolytic_vulnerability` ("can only be damaged by Isolytic Damage" — previously dropped) and the 8–10% round-1 `hostile_hyperthermic_decay` companions. Interdimensional Threat I/II/III keep their (already correct) damage fractions as explicit overrides plus new `apex_barrier` / `crit_damage` extra seats; Take the Shot maps to `isolytic_cascade` 1.0 (was flag-driven 0.01).
- **Programmable Matter (4 ids, 91 hostiles)** re-modeled: `{0:#.#%}` (0.5 / 0.99) is a **final-damage reduction**, not isolytic defense → new engine hook `hostile_final_damage_reduction` (multiplies the player's outbound post-apex pool by `1 − X`, riding the per-shot apex factor; counter-fire unaffected); "completely drains your shields" → `hostile_attacker_shield_mitigation_zero` (Strike Down hook); dampener defense 10.0 + round-1 hyperthermic as above.
- **Conditional self-debuffs** ("Isolytic Defense is reduced/lowered by X% when in combat with a battleship/explorer", 42 ids, 56 hostiles): negative seats (new `negate_value`) gated by new `condition_attacker_ship_type` → `AttackerShipTypeIs` (previously unconditional positive buffs).
- **Left as-is (documented unvalidated):** no-placeholder, no-number multi-stat texts keep the legacy flag-driven scale — Black Market Armaments (6 ids, 71 hostiles), Krenim Temporal Core (4 ids, ~84), Static Displacer/Collider (2 ids, 24; also under-modeled: text is cumulative-per-round, seat is static +100%), Judicious Preparation / Isolytic Maul-style single-stat proxies. Replicated Honorguard Apex (1 id, 30 hostiles) → `isolytic_multi_review` noop: 4 stats, no numbers, upstream value 0.01 unattributable (old seat was +0.01% defense — noise). Hostile-side piercing lines (Isolytic Maul "+500% all piercing") stay unmodeled: counter-fire mitigation carries no hostile pierce term (DESIGN.md §3.5).

**Fidelity pass (2026-07-08):** Four engine/resolver fixes plus five newly modeled text families:

- **Proc chance `1` = 100%.** All 22 live `attack_multiplier`/`pierce_bonus` catalog rows carry upstream `values[].chance: 1` ("always active"); `normalize_probability` previously folded `1.0` → `0.01`, so these buffs procced on 1% of counter hits. Upstream convention (4,537 of ~4,924 rows use `chance: 1`): values ≤ 1.0 are fractions, values > 1.0 are percent-scaled.
- **`attack_multiplier` value is a bonus fraction.** `ProcAttackMultiplier` now composes as `×(1 + value)`; the curated Xindi bundle overrides (`value_override` 125 / 66 / 60 / 50) match their upstream texts (+12500% / +6600% / +6000% / +5000%) exactly. `2560325528` gained the missing override (+5000% weapon damage + 150% crit floor extra seat) its siblings already had.
- **`round_cap` on hostile catalog entries** ("for the first N rounds" texts) maps to an `AbilityCondition::RoundRange { 1, N }` seat condition, evaluated by the standard per-round defender effect filtering.
- **Upstream `{0:#.#%}` placeholders are fractions.** The generator now detects the C# percent-format placeholder (value 0.75 renders as "75%") and passes such values through raw instead of dividing by 100.

| Newly modeled family | Ability ids | Hostiles | Mapping |
| --- | ---: | ---: | --- |
| Ruthless Pursuit / Deadly Strike / Predator Instincts (`390948510`) | 1 | 53 | `crit_chance` +100% `round_cap: 4` + extra seats `crit_damage` +350%, `hostile_crit_damage_floor` 0.5 |
| Persistence Hunter (`986116981`) | 1 | 53 | `burning` at `combat_begin`, 100% / 6 rounds — engine rolls defender combat-begin `Burning` onto the **player** (`attacker_burning_rounds`; 1% max-hull tick per round) |
| Pen of Kahless | 82 | 142 | `hostile_counter_pierce_multiplier` (+X% of counter pierce, `round_cap: 5`) — new effect; flat `pierce_bonus` would be inert against the fraction-scale pierce-through term |
| Revolutionary Spirit | 82 | 142 | `crit_damage` fraction with `round_cap: 5` |
| Psionic Assault | 18 | 108 | `hostile_hyperthermic_decay` at `round_start` (X% of player max hull per round) — was misclassified `attack_multiplier` |

**Faction-gated lethal strikes (2026-07-08 follow-up):** Five `other_review` noops → `hostile_lethal_unless_attacker_faction` (bucket `faction_gate_lethal`). Wrong hull design faction → pre-combat instant loss (`rounds_simulated == 0`), same path as Conqueror Borg beams. Gate uses `SimulationConfig.attacker_owner_faction` (`ShipRecord::faction`); Q texts also allow `uss_vengeance` by ship id. Both Q abilities emit `hostile_crit_damage_floor` 3.0 (text "300%"); Strike Down adds `hostile_attacker_shield_mitigation_zero` (forces incoming SM to 0% for the fight). **Uncertainty:** ~49 ships lack a `faction` slug (including Vengeance, which relies on the id exception); missing faction → instant loss unless exempt — fill ship `faction` data when known.

| Ability id | Hostiles | Gate | Extra seats |
| --- | ---: | --- | --- |
| `2518573064` | 30 | Fed \| Klingon | — |
| `1651219904` | 30 | Fed \| Romulan | — |
| `1088929105` | 30 | Klingon \| Romulan | — |
| `1206267116` | 17 | Fed \| Rom \| Klingon **or** `uss_vengeance` | crit floor 3.0 |
| `1567589326` | 16 | same | crit floor 3.0 + SM→0 |

Coverage: `tests/hostile_fidelity_new_mechanics.rs`; resolver units in `src/data/hostile_ability_resolve.rs` and `src/data/ship_ability_resolve.rs`.

**Per-hit stacking counter buffs (2026-07-08 follow-up):** Critical Breach / Rising Fire leave `other_review` → `defender_on_hit_*_stack` (bucket `defender_on_hit_stack`). Each defender weapon hit while the player-state gate holds pushes one stack lasting 2 rounds (Seska/shots-bonus expiry: active while `round_index <= expires_round`; stacks earned on hit N boost hit N+1 same round). Companions Hole Puncher / Immolator leave `pvp_player_target` via narrow combat-start exceptions → `hull_breach` / `burning` for `MAX_COMBAT_ROUNDS` on the player. Critical Breach also emits `hostile_crit_damage_floor` 1.5.

| Ability id | Hostiles | Mapping |
| --- | ---: | --- |
| `3358683912` | 17 | `defender_on_hit_crit_chance_stack` (gate: player hull breach) + crit floor 1.5 |
| `3353377682` | 17 | `defender_on_hit_weapon_damage_stack` (gate: player burning) |
| `3503588487` | 17 | `hull_breach` combat_begin → player, duration 100 |
| `3687094821` | 17 | `burning` combat_begin → player, duration 100 |

**Uncertainty:** stack expiry off-by-one vs client is unconfirmed (Seska convention chosen). True PvP "enemy player" rows (Deadlock / Dismantlement) stay noop.

**Dilithium Destabilization (2026-07-08 follow-up):** Two `other_review` noops → `hostile_lethal_combat_begin` (bucket `dilithium_destabilization`). Once per trial at combat begin, roll upstream `values[0].chance` (fraction; **not** `values[0].value`, which is a flag `1`); on success, same instant-loss early return as faction-gate / Conqueror Borg beams (`rounds_simulated == 0`). RNG draws only when a seat exists (after Denticle Blade + defender Burning + HullBreach rolls); `roll_proc_chance_short_circuit` skips the draw at chance 0/1.

| Ability id | Hostiles | Chance |
| --- | ---: | ---: |
| `167520385` | 23 | 0.9 (90%) |
| `3566779117` | 4 | 0.3 (30%) |

Coverage: `tests/hostile_fidelity_new_mechanics.rs` (`dilithium_*`); resolver unit in `src/data/hostile_ability_resolve.rs`.

**Intraluminary (2026-07-09 follow-up):** One `other_review` noop → `hostile_self_morale` (bucket `hostile_self_morale`). At combat begin, sets `defender_morale_rounds_remaining` to `MAX_COMBAT_ROUNDS` (100) with no RNG. Modeled combat benefit: +10% counter-fire pierce for any hull class via `defender_morale_adjusted_pierce` (Morale boosts all piercing stats; the player-inbound mitigation path has no per-channel hostile piercing stats, so the bonus collapses onto the aggregate counter pierce scalar). The 17 Assimilated Coryn-class Explorer carriers get the counter pierce boost as well.

| Ability id | Hostiles | Mapping |
| --- | ---: | --- |
| `4021963607` | 17 | `hostile_self_morale` combat_begin, `duration_rounds: 100` |

Coverage: `tests/hostile_fidelity_new_mechanics.rs` (`intraluminary_*`); resolver unit in `src/data/hostile_ability_resolve.rs`.

**Plausible Deniability (2026-07-15 follow-up):** 82 identical-text `other_review` noops → `shield_regen_max_fraction` (bucket `shield_regen_combat`, 140 hostile instances — S31-era hostiles). Text: "Recovers {0:#.#%} of total SHP for the first 5 rounds of combat"; upstream value is a fraction (`{0:#.#%}` convention, e.g. `0.2` renders "20%"). Maps to a defender `ShieldRegenMaxFraction` seat at **round end** (a recovery per round while the fight is under way; the round-start alternative would waste round 1 on full shields) gated to rounds 1..=5 via `round_cap` → `AbilityCondition::RoundRange`. The defender round-end regen path (`composed_shield_regen_max_fraction` over condition-filtered round-end effects, `src/combat/engine.rs`) was already wired — this is a catalog + resolver-mapping change only (`shield_regen_max_fraction` added to `ship_ability_effect_from_catalog`, same stat naming as the LCARS adapter). Backlog #8 estimated ~5 ids × 3 hostiles; text-match enumeration found 82 ids.

| Ability id family | Ids | Hostiles | Mapping |
| --- | ---: | ---: | --- |
| Plausible Deniability (e.g. `932011628`, `3926823774`) | 82 | 140 | `shield_regen_max_fraction` @ `round_end`, `round_cap: 5`, value = upstream fraction of max SHP per round |

Coverage: `tests/hostile_first_rounds_shield_regen.rs`; resolver unit in `src/data/hostile_ability_resolve.rs`.

**Q Junior's Twist (2026-07-15 follow-up):** The Q Trials Borg texts leave `other_review`. Only the loca 73055 variant (`755115993`, 23 hostiles) carries a mechanical clause — "Just defeat the Borg Polygon **within 20 rounds**" — and maps to the new `hostile_engagement_round_limit` (bucket `engagement_limit_combat`): the engine caps `rounds_to_simulate` at the limit, and a hostile still alive at the cap is a **timeout loss** (DESIGN.md §4.4 — matches the official Q's Trials rule that the trial fails when the target is not destroyed; scopely.helpshift.com Q's Trials FAQ). The loca 73051 variant (`1104294321`, 23 hostiles, a **disjoint** hostile set) is the 1v1 restriction only — no modelable single-ship mechanic, kept noop under the dedicated `q_trials_flavor` bucket. Backlog #7 assumed both ids carried the limit; the ability texts say otherwise.

| Ability id | Hostiles | Mapping |
| --- | ---: | --- |
| `755115993` | 23 | `hostile_engagement_round_limit` @ `combat_begin`, `value_override: 20` (generator parses "within N rounds") |
| `1104294321` | 23 | `combat_noop` (`q_trials_flavor`) — 1v1 restriction, out of scope |

Coverage: `tests/hostile_engagement_round_limit.rs`; resolver unit in `src/data/hostile_ability_resolve.rs`.

**Xindi (2026-06-16):** Fixed a PvP classifier false positive on NPC text (`enemy players ship` ≠ PvP `enemy player`). Modeled ability ids:

| Ability id | Hostiles | Primary effect | Notes |
| --- | ---: | --- | --- |
| `1271329828` | 45 | `hostile_crit_damage_reduction` + lethal `extra_seat` | Doomed Species (2R stack) + Xindi Weaponry particle beam (round-end lethal) |
| `1408273502` | 25 | `hostile_crit_damage_reduction` | Be Like Water; Xindi Might text = 9×20B weapon only |
| `141924765` | 14 | `hostile_crit_damage_reduction` + Denticle extra seat | Be Like Water + **Denticle Blade** (combat-start 30% proc gates weapon slot 5); Xindi Might = weapon only |
| `2665723295` | 6 | `hostile_lethal_end_of_round` (`round_interval: 8`) | No Mercy — assimilated prevents 100% |
| `3981152012` | 6 | `hostile_kemocite_weaponry` @ `round_end` | Kemocite — +30%/stack at round end; burning prevents 100% |

See `tests/xindi_hostile_abilities.rs` and [DESIGN.md](DESIGN.md) §3.6 for lethal/crit approximations.

**Be Like Water crit debuff:** upstream value `25` (−2500% UI) subtracts 25 percentage points from the player's outbound crit bonus (typical high-crit builds → ×1.0 before floor). **Critical Damage Floor** then clamps the post-debuff multiplier (`after_mult.max(crit_damage_floor)` in [`crit.rs`](../src/combat/crit.rs)), so outbound crits can still exceed ×1.0 base when floor research is high — e.g. Enterprise-D vs Aquatic Cruiser L51 fight sample crit/non-crit hull ~×1.61 is consistent with BLW collapse plus a floor near that value.

Descriptions are keyed by `translations-ship_buffs.json` (`key: ship_ability_desc`, `id` = per-row `loca_id` from `hostiles/*.json ability[]`).

---

## 1. Three modeling lanes

Hostile combat behavior is **not** a single pipeline. The audit tracks three lanes:

| Lane | Source | Runtime path | Catalog? |
| --- | --- | --- | --- |
| **A — Catalog → defender crew** | `HostileRecord.ability[]` | [`hostile_abilities_to_defender_crew`](../src/data/hostile_ability_resolve.rs) → [`scenario.rs`](../src/optimizer/monte_carlo/scenario.rs) `defender_crew` → counter-fire effect accumulator | Yes — this doc |
| **B — Tag-driven mechanics** | Curated `hostile_tags` on normalized records | [`conqueror_borg_beams.rs`](../src/combat/conqueror_borg_beams.rs), [`evolutionary_assimilation.rs`](../src/combat/evolutionary_assimilation.rs) | No — hardcoded in normalizer + engine |
| **C — Base stats / components** | Normalized hull/shield/weapon fields | [`defender_combatant_from_hostile_record`](../src/optimizer/monte_carlo/scenario.rs) | Out of scope for `ability[]` catalog |

**Trace naming:** `EventSource.hostile_ability_id` in combat traces (e.g. `{defender_id}_mitigation`) labels **mechanics already running** — it is **not** the upstream catalog id. Do not use trace ids for coverage accounting.

**Attacker ship abilities vs hostiles** (Crozier CDR, Track D2 debuffs) live in the **ship** catalog — see [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md).

### Lane B — Conqueror Borg (modeled outside catalog)

32 upstream hostile ids receive tags in [`normalize_hostiles_stfc_space.rs`](../src/bin/normalize_hostiles_stfc_space.rs) (`curated_hostile_tags_for_upstream`):

- `conqueror_borg_suppressor` → Quantum Resonance Beam instant loss vs non–Borg-Sphere attackers
- `conqueror_borg_obliterator` → Hyperthermic Resonance Beam (80% vs Borg Sphere hull)
- `conqueror_borg` + forbidden officers → Evolutionary Assimilation instant loss

Attacker-side **Quantum Nullification Pulse** is a **ship** hull ability (`2425475474`, `conqueror_borg_beam_suppression`), not a hostile catalog row.

Calibration fixtures: `tests/fixtures/recorded_fights/drift_conqueror_borg_*.json`.

---

## 2. Inventory

| Metric | Count |
| --- | ---: |
| Unique upstream ability ids | 982 |
| Modeled (`effect_type` ≠ `combat_noop`) | 521 |
| `combat_noop` (catalogued, inert in sim) | 461 |

**Modeled effect types (521 ids, refreshed 2026-07-16):**

| `effect_type` | Ids |
| --- | ---: |
| `isolytic_damage` | 87 |
| `isolytic_defense` | 81 |
| `isolytic_cascade` | 1 |
| `shield_regen_max_fraction` | 82 |
| `hostile_counter_pierce_multiplier` | 82 |
| `crit_damage` | 82 |
| `apex_barrier` | 54 |
| `hostile_hyperthermic_decay` | 22 |
| `hostile_lethal_unless_attacker_faction` | 5 |
| `attack_multiplier` | 4 |
| `hostile_crit_damage_reduction` | 3 |
| `hostile_lethal_combat_begin` | 2 |
| `crit_chance` (+ `crit_damage` / `hostile_crit_damage_floor` extra seats) | 2 |
| `hostile_crit_damage_floor` | 2 |
| `burning` | 2 |
| `shield_mitigation_bypass` | 2 |
| `hostile_isolytic_vulnerability` | 1 |
| `hostile_engagement_round_limit` | 1 |
| `hostile_lethal_end_of_round` | 1 |
| `defender_on_hit_weapon_damage_stack` | 1 |
| `defender_on_hit_crit_chance_stack` | 1 |
| `hull_breach` | 1 |
| `hostile_kemocite_weaponry` | 1 |
| `hostile_self_morale` | 1 |

**Not yet modeled (high instance count, remain `combat_noop`):**

- PvP player targeting (2 ids, 388 instances): Deadlock / Dismantlement — default PvE path is ship vs NPC hostile (Hole Puncher / Immolator modeled 2026-07-08)
- Armada scope (125 ids, 260 instances)
- Outpost scope (56 ids, 163 instances)
- `other_review` (272 ids, 361 instances): Temporal Dreadnought regen, etc. (Plausible Deniability + Q Junior's Twist modeled 2026-07-15)

Full regen-safe noop id list: run `python3 scripts/generate_full_hostile_ability_catalog.py` and filter `effect_type == combat_noop` in the catalog JSON.

---

## 3. Buckets (generator heuristics)

| Bucket | Unique ids | Hostile instances | Decision |
| --- | ---: | ---: | --- |
| Isolytic combat-start | 170 | 1,498 | **Modeled** — `combat_begin` + `isolytic_damage` / `isolytic_defense` (+ `isolytic_cascade`, `apex_shred`/`apex_barrier`/`hostile_final_damage_reduction` extra seats); value scales ground-truthed 2026-07-16, see header; Honorguard → `isolytic_multi_review` noop |
| Apex barrier | 54 | 368 | **Modeled** — `combat_begin` + `apex_barrier` |
| Crit multi-stat | 2 | 378 | **Modeled** — Critical Training + Ruthless Pursuit emit `crit_chance` plus `crit_damage` / `hostile_crit_damage_floor` extra seats |
| Crit damage floor | 2 | 273 | **Modeled** — Diverted Power emits `hostile_crit_damage_floor` |
| Pierce first-N-rounds | 82 | 142 | **Modeled (2026-07-08)** — Pen of Kahless → `hostile_counter_pierce_multiplier` + `round_cap` (was "Defense stat review") |
| Crit first-N-rounds | 82 | 142 | **Modeled (2026-07-08)** — Revolutionary Spirit → `crit_damage` + `round_cap` |
| Hyperthermic decay per-round | 18 | 108 | **Modeled (2026-07-08)** — Psionic Assault → `round_start` + `hostile_hyperthermic_decay` |
| Burning at combat start | 1 | 53 | **Modeled (2026-07-08)** — Persistence Hunter → `combat_begin` + `burning` on the player |
| Faction-gated lethal strike | 5 | 123 | **Modeled (2026-07-08)** — Tal Shiar / Mo'Kai / S31 / Q → `hostile_lethal_unless_attacker_faction` (+ Q crit floor / Strike Down SM→0) |
| Defender per-hit stacks | 2 | 34 | **Modeled (2026-07-08)** — Critical Breach / Rising Fire → `defender_on_hit_*_stack` |
| Dilithium Destabilization | 2 | 27 | **Modeled (2026-07-08)** — chance-gated combat-begin instant kill → `hostile_lethal_combat_begin` (chance from upstream `values[].chance`) |
| Intraluminary self-morale | 1 | 17 | **Modeled (2026-07-09)** — combat-begin `hostile_self_morale` → defender Morale for rest of combat (+10% counter pierce, any hull class) |
| Shield regen first-N-rounds | 82 | 140 | **Modeled (2026-07-15)** — Plausible Deniability → `shield_regen_max_fraction` @ `round_end` + `round_cap: 5` (fraction of max SHP per round) |
| Engagement round limit | 1 | 23 | **Modeled (2026-07-15)** — Q Junior's Twist (loca 73055) → `hostile_engagement_round_limit` 20; still-alive hostile at the cap = timeout loss |
| Q Trials flavor (1v1) | 1 | 23 | **Keep noop** — Q Junior's Twist 1v1 variant (loca 73051), no modelable single-ship mechanic |
| Player hull breach at combat start | 1 | 17 | **Modeled (2026-07-08)** — Hole Puncher → `hull_breach` on the player for rest of combat |
| Player burning at combat start (Immolator) | 1 | 17 | **Modeled (2026-07-08)** — Immolator → `burning` on the player for rest of combat |
| Weapon damage conditional | 1 | 13 | **Partial** — `attack_multiplier` where text matches; hull-breach gates use `condition_defender_hull_breach` |
| PvP enemy player | 2 | 388 | **Keep noop** on default ship-vs-hostile path (Deadlock / Dismantlement) |
| Armada | 125 | 260 | **Keep noop** — no armada scenario |
| Outpost | 56 | 163 | **Keep noop** — station/outpost scope |
| Hyperthermic review | 3 | 15 | **Keep noop** — resonance-beam / non-uniform value scales, manual review |
| Economy | 1 | 30 | **Keep noop** |
| Other / review | 272 | 361 | **Shard triage** — extend generator or overrides per pattern |

---

## 4. Top 20 by hostile count

| Ability id | Hostiles | Bucket | Catalog `effect_type` | Text (plain snippet) |
| --- | ---: | --- | --- | --- |
| `2291206649` | 325 | crit_multi_stat_modeled | `crit_chance` + `crit_damage` / `hostile_crit_damage_floor` extra seats | Critical Training — crit chance + damage + floor at combat start |
| `849650945` | 194 | pvp_player_target | `combat_noop` | Deadlock — hull breach enemy player |
| `910140799` | 194 | pvp_player_target | `combat_noop` | Dismantlement — weapon damage if enemy player hull breached |
| `2486538514` | 162 | crit_floor_modeled | `hostile_crit_damage_floor` | Diverted Power — crit damage floor |
| `788454016` | 111 | crit_floor_modeled | `hostile_crit_damage_floor` | Diverted Power — crit damage floor |
| `3172395625` | 90 | isolytic_combat | `isolytic_damage` | Elite Assassin Training — isolytic at combat start |
| `2747222231` | 82 | outpost_scope | `combat_noop` | Diverted Power (outpost) |
| `1782396999` | 69 | apex_combat | `apex_barrier` | Not So Wounded — apex barrier |
| `3257135627` | 69 | isolytic_combat | `isolytic_damage` | Augmented Force — isolytic at combat start |
| `390948510` | 53 | crit_multi_stat_modeled | `crit_chance` (`round_cap: 4`) + `crit_damage` / `hostile_crit_damage_floor` extra seats | Ruthless Pursuit — +100% crit chance first 4 rounds, +350% crit damage, 50% crit floor |
| `658066283` | 53 | isolytic_combat | `hostile_isolytic_vulnerability` | Isolytic Vulnerability |
| `986116981` | 53 | burning_combat_start | `burning` | Persistence Hunter — 100% burning on the player for 6 rounds at combat start |
| `1745201100` | 53 | isolytic_combat | `isolytic_damage` | Isolytic Maul |
| `1271329828` | 45 | xindi_crit_debuff | `hostile_crit_damage_reduction` + lethal extra seat | Doomed Species + Xindi Weaponry particle beam |
| `141924765` | 14 | xindi_crit_debuff | `hostile_crit_damage_reduction` + Denticle extra seat | Be Like Water + Denticle Blade (30% proc gates weapon slot 5) |
| `3445799437` | 45 | hostile_shield_bypass | `shield_mitigation_bypass` | Blade's Tip — 100% bypass of player shield mitigation on counter |
| `2936293636` | 44 | isolytic_combat | `isolytic_defense` | Programmable Matter — dampener defense 10.0 + `hostile_final_damage_reduction` 0.5 + SM→0 + round-1 hyperthermic (2026-07-16) |
| `3196612078` | 39 | hostile_shield_bypass | `shield_mitigation_bypass` | Strength of the Ibix — 100% bypass (10 shots are weapon components, not this seat) |
| `1088929105` | 30 | faction_gate_lethal | `hostile_lethal_unless_attacker_faction` | S31 Elite — Klingon/Romulan designed ships only |
| `1539285779` | 30 | armada_scope | `combat_noop` | Armada isolytic defense |
| `1651219904` | 30 | faction_gate_lethal | `hostile_lethal_unless_attacker_faction` | Mo'Kai Elite — Fed/Romulan designed ships only |

---

## 5. Drift control (regeneration)

1. Run from repo root: `python3 scripts/generate_full_hostile_ability_catalog.py`
2. **Overrides:** After heuristics, merges [`hostile_ability_catalog_overrides.json`](../data/upstream/data-stfc-space/hostile_ability_catalog_overrides.json) (`entries`: ability id → full catalog row).
3. **Audit metadata:** `hostile_ability_audit_meta.json` (bucket + hostile counts per id; not consumed at runtime).
4. **Diff:** Compare regenerated catalog to previous commit; port intentional deltas into the Python classifier or overrides file. The nine hand-maintained aggregation/offense/isolytic rows currently live in the overrides file so regeneration is idempotent.
5. **Parity test:** `cargo test --test hostile_ability_catalog_parity` — catalog keys must cover every upstream ability id.

---

## 6. Maintenance

When upstream hostiles refresh:

- Re-run the generator after `fetch_stfcspace_hostiles.mjs` / normalize.
- New combat-relevant patterns → extend `classify_hostile_ability` in the generator; one-offs → overrides JSON.
- Extend [`hostile_ability_effect_from_catalog`](../src/data/hostile_ability_resolve.rs) when adding new `effect_type` values (prefer delegating to ship resolver for shared effects).
- Label approximations in [DESIGN.md](DESIGN.md) §3.6 (Hostile hull abilities).

**Uncertainty:** Only the first `values[]` entry is used (same as ships). Per-level ability curves are not modeled. Percentage semantics follow catalog `value_is_percentage` + `ignore_upstream_value_is_percentage` — verify with overrides when upstream marks fractional values as percentage (Track D2 lesson).
