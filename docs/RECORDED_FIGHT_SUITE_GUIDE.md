# Recorded fight suite — curation guide

How to choose **30–50 in-game fights** for a snapshot-bound calibration suite. **Not on the roadmap** — reference material under [NOT_ROADMAP.md](NOT_ROADMAP.md) § [Snapshot-bound calibration](NOT_ROADMAP.md#snapshot-bound-calibration-simulation-fidelity) and the [snapshot-calibration protocol](NOT_ROADMAP.md#snapshot-calibration-protocol) there.

**Audience:** maintainer running the one-time freeze window (see [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md)).

---

## What this suite is (and is not)

| Layer | Location | Role |
| --- | --- | --- |
| **Recorded fight suite** (this doc) | `fight samples/*.csv` → promoted fixtures under `tests/fixtures/recorded_fights/` | End-to-end: frozen profile + real ship/hostile/crew + observed outcome. Composite accuracy score. |
| **Synthetic drift fixtures** | `tests/fixtures/recorded_fights/drift_*.json` (~20 today) | Targeted mechanic regression (mitigation floor, Conqueror Borg beams, research pooling, etc.). Develop the harness *before* the freeze; do **not** count toward the 30–50. |
| **Unit / anchor tests** | e.g. `officer_stat_calibration_anchors.rs`, `data_provenance_tests.rs` | Formula routing and data provenance without full fight logs. |

Recorded fights answer: *“Given my exact game state, does the sim reproduce what I saw?”* Drift fixtures answer: *“When we isolate one mechanic, does the engine still behave?”* You need both; this guide is only for the recorded layer.

---

## Non‑negotiable rules (from the protocol)

1. **One profile, one vintage.** Snapshot research, buildings, forbidden tech, syndicate, officer levels/tiers, and ship tiers/levels into a Kobayashi profile **first**. Every fight in the suite must be recorded **during the same freeze** with **no progression** between exports.
2. **Do not grow the corpus from mixed-vintage logs.** Fights captured months apart at different research tiers will punish or reward the wrong model changes. Wait for the freeze window.
3. **Bind suite ↔ profile.** Commit the profile snapshot (e.g. under `profiles/<id>/`) alongside the fight exports. CI and the composite score must load **that** profile, not `demo`.
4. **Prefer game TSV exports** ([combat log format](combat_log_format.md)). Outcome fields (rounds, total damage, end hull/shield, win/loss) are observable; per-round detail is coarse—that is acceptable for calibration bands.
5. **Resolve defender faction** via export identity (`enemy_player_name`, `enemy_ship_level`) or an explicit slug override when upstream faction mapping is `Unknown` ([HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md)).

---

## Target size: 30–50 fights

| Tier | Count | Purpose |
| --- | ---: | --- |
| **Core** (must record) | 18–22 | Ship classes, outcomes, baseline hostiles |
| **Mechanic stress** | 10–14 | Abilities, hostile effects, research gates you actually have synced |
| **Profile integration** | 4–6 | Forbidden tech, buildings, syndicate bonuses that affect combat |
| **Holdout** | 5–8 | Record during the freeze but **exclude from iteration** until a release review |

Total **37–50** recorded fights; aim for **~40** if the sitting is tight on time.

Synthetic `drift_*.json` scenarios stay separate and can grow without a freeze.

---

## Selection dimensions

Use these as **coverage axes**, not as “record 50 random hostiles.” Each fight should **tag** at least one primary axis (document in filename or a sidecar manifest).

### 1. Ship class (4 classes × depth)

Officer Defense routes differently per class ([OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) §2c):

| Class | Mitigation channel | Record at least |
| --- | --- | --- |
| Battleship | `armor` | 2 fights (early/mid ship + highest-tier battleship you use) |
| Explorer | `shield_deflection` | 2 fights |
| Interceptor | `dodge` | 2 fights (include **dual-weapon** ship if owned) |
| Survey | thirds split | 2 fights |

**Existing anchor:** Realta (explorer) vs Takret Militia 10 — keep as the **low-tier baseline** ([`fight_export_realta_vs_takret_militia_10_matches_simulation`](../tests/recorded_fight_calibration_tests.rs)).

### 2. Fight duration & outcome (6–8 fights)

| Archetype | Why | Example pattern |
| --- | --- | --- |
| 1-round kill | Validates burst damage, no counter-fire accumulation | Realta vs low hostile |
| Short win (2–5 rounds) | Shield depletion + early counter-fire | Mid-tier vs equal-level hostile |
| Long win (10–25 rounds) | Morale procs, stacking, rounding over many rounds | Enterprise-D vs high-level boss |
| **Loss** | Prevents sim from only fitting wins | Same crew vs +5–10 level hostile |
| **Narrow win** | Hull margin & stall sensitivity | Match where you finished &lt;15% hull |
| Timeout / max rounds | Rare; only if you routinely hit round cap | Optional |

**Existing anchor:** U.S.S. Enterprise-D vs V'ger Hurak 59 (23 rounds, morale-heavy) — [`galaxy_ent_d_vs_hurak59_log_calibration.rs`](../tests/galaxy_ent_d_vs_hurak59_log_calibration.rs).

### 3. Hostile ability lanes (8–10 fights)

Prioritize hostiles whose abilities are **modeled** in [`hostile_ability_catalog.json`](../data/upstream/data-stfc-space/hostile_ability_catalog.json) ([HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md)):

| Mechanic | Suggested fights | Notes |
| --- | ---: | --- |
| **Isolytic** damage + defense | 2–3 | Most common high-impact PvE bucket; vary hostile level |
| **Apex barrier** | 2 | Include one fight where barrier materially delays hull damage |
| **Attack multiplier** (incl. hull-breach gates) | 1–2 | Hostile applies `condition_defender_hull_breach` if possible |
| **Burning on attacker** | 1 | Complements `drift_hostile_burning_on_attacker.json` |
| **Hull breach on attacker** | 1 | Complements `drift_hostile_hull_breach_on_attacker.json` |
| **High mitigation + pierce tension** | 1 | Your build vs armored hostile; not duplicate of drift if profile differs |

**Lane B (tag-driven, not catalog):** Conqueror Borg — record **only** if you fight them during the freeze:

- Borg Sphere + Quantum Nullification vs **Suppressor** (instant-loss beam suppressed)
- Borg Sphere vs **Obliterator** (80% HRB path)
- Non-sphere ship vs Suppressor (**loss**, expected instant kill)

**Existing sample:** `borg sphere with forbidden officer vs conqueror borg supressor.csv`.

**Skip for this suite:** Armada-only hostiles, outpost defenders, PvP, Apex Raider solo wave — not on the default ship-vs-hostile path ([NOT_ROADMAP.md](NOT_ROADMAP.md)).

### 4. Ship hull abilities (6–8 fights)

From [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) — favor **modeled** abilities you actually sail:

| Ability style | Suggested fights | Notes |
| --- | ---: | --- |
| **Faction-gated** attack multiplier | 2 + 1 control | e.g. U.S.S. Athena vs **Venari Ral** hostile + same ship vs non-Venari (**negative control**) |
| **Class-gated** vs matching hostile class | 1–2 | Explorer / battleship / interceptor gate |
| **Modeled proc chain** | 1–2 | Hegh'ta “Open the Wound”, Rotarran “Bird of Prey” if in roster |
| **Hostile debuff / shield drain** (D2 ships) | 1–2 | Quv'Sompek, Sanctus, B'Rel, Intrepid-style rows |
| **Conqueror Borg beam suppression** | 1 | Overlaps hostile Lane B; still worth one real export |

**Skip:** mining, loot, hazard resist, armada scope, station/defending clauses — intentionally `combat_noop`.

### 5. Officer & crew effects (5–7 fights)

| Goal | Fights | Crew notes |
| --- | ---: | --- |
| **Officer stat formula anchors** | 3 | Use the **exact** ship + crew from the freeze for Kirk (morale-gated attack), Marla (+officerstatall self), Kras (−officerstatall enemy bridge). Fills `_TBD_` damage anchors in [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md). |
| Morale proc crew | 1–2 | Overlap with long fights is fine; tag separately |
| Reload / extra shot officers | 1 | If synced in profile (e.g. Kuron-style) |
| On-kill hull regen | 1 | Officer or hostile ability that triggers regen |
| Caleb Mir shield restore vs non-Armada | 1 | June 2026 batch — only hostile PvE path |

Avoid crews that are **entirely** Wave Defense, PvP-only, or outpost-gated unless you add a separate non-goal suite later.

### 6. Research & profile gates (4–6 fights)

Only record gates for research **actually completed** in the frozen snapshot ([research_conditional_routing.md](research_conditional_routing.md)):

| Gate type | Fights | Hostile / condition |
| --- | ---: | --- |
| Cross-faction **weapon_damage** | 1–2 | Romulan vs Federation, Federation vs Klingon, Klingon vs Romulan — pick factions you have synced |
| **Burning**-gated weapon damage | 1 | NS burning tree if synced |
| **Morale**-gated hull/shield or weapon damage | 1 | Requires morale proc in fight |
| **KSG incoming shield mitigation** (rounds 1..N) | 1 | Early-round counter-fire sensitivity |
| Dual-gate hull/shield (when upstream adds rows) | 0–1 | Scenario path exists; no owner+defender-only hull/shield rows in catalog yet — add when game ships them |

Also tag whether **`KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL`** vs layered profile fractions changes the outcome for your profile (drift fixtures cover the mechanic; one **real** fight confirms profile wiring).

### 7. Buildings, forbidden tech, syndicate (4–6 fights)

Pick fights where profile merge **materially** moves stats—not every bonus needs its own fight, but major combat seats should appear at least once:

- Borg Alcove / Operating Table / Quantum Slipstream (if synced)
- Ship-class torpedo-family forbidden tech
- Syndicate reputation combat rows
- Building buffs that feed weapon_damage, crit, or mitigation

If a bonus is synced but no fight showcases it, that is a **profile-snapshot completeness** gap to fix before scoring.

---

## Recommended manifest (~40 fights)

Use as a checklist during the freeze sitting. Adjust counts if you lack a ship or hostile; **replace with the closest same-axis fight** and note the swap in the manifest.

| # | Axis tags | Suggested content |
| ---: | --- | --- |
| 1–4 | class baseline | One win per class vs level-appropriate hostile (~same level as ship) |
| 5–8 | class peak | Highest-tier ship you use per class vs challenging hostile |
| 9 | duration | 1-round kill (Realta-tier or equivalent) |
| 10 | duration | 2–5 round win |
| 11 | duration | 10+ round win |
| 12 | outcome | Loss |
| 13 | outcome | Narrow win (&lt;15% hull) |
| 14–15 | isolytic | Two hostiles with isolytic damage/defense, different levels |
| 16–17 | apex | Two apex-barrier hostiles |
| 18 | hull breach gate | Hostile attack multiplier gated on breach |
| 19 | burning | Hostile applies burning to you |
| 20 | hull breach | Hostile applies hull breach to you |
| 21–23 | Conqueror Borg | Suppressor win w/ QNP, Obliterator path, non-sphere loss |
| 24–25 | faction ship ability | Gated ship vs matching faction + negative control |
| 26–27 | class-gated ship ability | Matching + non-matching hostile class |
| 28–29 | proc / debuff ships | Hegh'ta or Rotarran; D2 debuff ship |
| 30–32 | officer anchors | Kirk, Marla, Kras (exact anchor crews) |
| 33 | morale | Crew with frequent morale procs |
| 34 | on-kill regen | Regen officer or hostile |
| 35–36 | research gates | Cross-faction weapon_damage + one conditional (burning/morale) |
| 37 | KSG / incoming SM | If synced |
| 38–40 | profile tech | FT / building / syndicate showcase fights |
| 41–45 | **holdout** | Same axes as above, **not** used in auto-iterate loop |

---

## Fights **not** to include

- **Armada** engagements (e.g. solo armada Borg Type 03) — armada scope is largely `combat_noop` ([HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md)).
- **Outpost / station defense** — explicit non-goal.
- **PvP** — different scenario path; keep a separate future corpus if ever needed.
- **Duplicate axes** — ten Interceptor vs Klingon hostiles at the same level teach little; diversify.
- **Officer ability not modeled** — recording “Genesis Lythe loot bonus” does not calibrate combat.
- **Mixed-vintage replays** — old CSVs from before the freeze belong in `fight samples/` as anecdotes until re-recorded.

---

## Export workflow (per fight)

1. Verify profile snapshot is loaded in Kobayashi and matches in-game state.
2. Fight in-game; export TSV via normal game flow.
3. Save as `fight samples/<ship>_vs_<hostile>_<level>_<outcome>.csv` (lowercase, underscores).
4. Run parser smoke test locally:
   ```bash
   cargo test --test recorded_fight_calibration_tests fight_export
   cargo test --test log_ingest_tests
   ```
5. Add metadata row to the suite manifest ([`recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json)):
   - `profile_id`, `ship_id`, `ship_tier`, `ship_level`
   - `hostile_id` or resolved display name + level
   - `captain`, `bridge`, `below_decks`
   - `primary_axes`: list of tags from sections above
   - `holdout`: bool
6. Promote to `tests/fixtures/recorded_fights/` when the fight enters CI; keep raw CSV in `fight samples/` for provenance.

Defender faction: prefer auto-resolve from export; if `Unknown` (e.g. Takret Militia today), set `--defender-faction` / manifest override once [hostile.rs](../src/data/hostile.rs) mapping is fixed.

---

## Scoring & iteration (after recording)

1. **Composite score** — aggregate per-fight deviation (damage, rounds, outcome, end hull/shield) into one number; shipped as [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md) + `cargo xtask calibration-scoreboard`.
2. **Iterate loop** — engine changes accepted when composite improves **and** no non-holdout fight regresses beyond band.
3. **Holdout** — run full suite including holdout before tagging a release; never tune directly to holdout fights.

Band widths: start loose on first pass (outcome + order-of-magnitude damage), tighten as fidelity improves. Synthetic drift fixtures keep **tight** bands on isolated mechanics.

---

## Relationship to existing samples

| Sample | Status | Suite role |
| --- | --- | --- |
| `realta vs takret militia 10.csv` | In CI | Low-tier explorer baseline; re-record on freeze profile |
| `uss enterprise d vs vger hurak 59.csv` | Partial (outgoing damage fixture) | Long fight / morale; re-record full TSV on freeze |
| `borg sphere with forbidden officer vs conqueror borg supressor.csv` | Not in CI | Promote when recorded on freeze profile |
| `ss revenant … borg type 031 solo armada.csv` | **Do not promote** | Armada — out of scope for PvE suite |

---

## Prep that can ship before the freeze

These shorten the sitting but do not substitute for recorded fights:

- [x] Defender faction for TSV imports (shipped 2026-06-14)
- [ ] Profile-snapshot completeness audit (every combat input capturable in one pass) — not on roadmap; see [NOT_ROADMAP.md](NOT_ROADMAP.md)
- [x] Composite-score harness against existing `drift_*.json` — [`src/calibration/scoreboard.rs`](../src/calibration/scoreboard.rs), `cargo xtask calibration-scoreboard`
- [x] “Add a real fight log in 10 minutes” — [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md) + module doc in [calibration_drift_tests.rs](../tests/calibration_drift_tests.rs)
- [x] Suite manifest schema + CI loader — [`recorded_fight_suite.json`](../tests/fixtures/recorded_fights/recorded_fight_suite.json), [`src/calibration/recorded.rs`](../src/calibration/recorded.rs) (empty fights until freeze)

---

## Quick reference links

- [NOT_ROADMAP.md](NOT_ROADMAP.md) — snapshot-calibration protocol (not planned)
- [CALIBRATION_SCOREBOARD.md](CALIBRATION_SCOREBOARD.md) — regenerated accuracy scoreboard
- [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md) — add a fight workflow
- [HUMAN_INTERVENTION_TASKS.md](HUMAN_INTERVENTION_TASKS.md) — maintainer-only steps
- [combat_log_format.md](combat_log_format.md) — TSV / JSON ingest
- [HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md](HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md) — what hostile abilities matter
- [SHIP_ABILITY_COMBAT_NOOP_AUDIT.md](SHIP_ABILITY_COMBAT_NOOP_AUDIT.md) — what ship abilities matter
- [OFFICER_STAT_FORMULA.md](OFFICER_STAT_FORMULA.md) — officer A/D/H anchors to fill with real fights
