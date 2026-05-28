# Research catalog — combat bonus improvements

Track progress on research-derived combat bonuses. Sibling docs: [`data/README.md`](../data/README.md) § Research, [`DESIGN.md`](DESIGN.md) §5.4.

**Last reviewed:** 2026-05-27 (Track D conversion rule clarified)

---

## Current pipeline (reference)

```text
Community Mod sync          Maintainer catalog              Combat
──────────────────          ──────────────────              ──────
research.imported.json  →   research_catalog.json      →   profile.bonuses (flat stats)
(rid + level, all rows)     (960 projects w/ bonuses)       + research_owner_faction_bonuses
                            + buff_id/loca mappings         + research_derived_attack_phase_seats
                            + research_canonical.json         (conditional crit/wd/isolytic/etc.)
```

**Merge rules** (`src/data/research.rs`, `src/data/profile.rs`):

- Levels **1..=synced level** are cumulative; `multiply` ops compose within a project in ascending level order.
- **Unconditional** rows → flat `profile.bonuses`.
- **Owner-faction** rows → `research_owner_faction_bonuses` (applied when ship faction matches).
- **Conditional** rows (class, faction, morale, burning, hull breach) → attack-phase **seats**, not flat profile.
- **Dual-gate hull/shield** (owner faction + defender faction) → scenario path for **faction-only** dual gates.
- **Complex combos** → `data/research_canonical.json` (11 manual overrides today).

**Existing observability:** `GET /api/profile/research-summary`, Roster & Profile UI (`frontend/src/pages/RosterProfile.tsx`).

---

## Baseline metrics (2026-05-27, post–Track B)

| Metric | Value |
|--------|------:|
| Upstream research projects | 2,330 |
| Catalog projects with combat bonuses | 960 (~41%) |
| Skipped (no mapped combat levels) | 1,370 |
| Unmapped buff IDs (`--dump-unmapped`) | 1,219 |
| Conditional bonus rows in catalog | 864+ |
| Explicit buff-id mappings (`buff_id_to_stat.json`) | 102 keys |
| Loca-id mappings (`loca_id_to_stat.json`) | 67 |
| Canonical overrides (`research_canonical.json`) | 11 |
| Officer `officer_*` bonus rows in catalog | 386 |

Refresh after catalog regen — see [research_unmapped_triage.md](research_unmapped_triage.md).

---

## Track A — Profile trust & observability

Low combat risk; makes research debugging practical.

- [x] Show `combat_owner_faction_bonuses_from_research` in Roster & Profile research panel
- [x] Surface conditional-only / seat-derived research in API summary (not just flat `combat_bonuses_from_research`)
- [x] Per-row indicator: flat vs owner-faction vs conditional-only vs unmapped catalog
- [x] Optional API query params for scenario-effective totals (`ship_id`, `hostile_id` or faction/class context)
- [x] Sort profile unmapped `rid`s by synced level (actionable “what am I missing?” list)

---

## Track B — Mapping coverage (data pipeline)

Highest sim impact for synced accounts; mostly mapping + re-import work.

### Triage & process

- [x] Run full `--dump-unmapped` and capture top buff IDs by `count`
- [x] Triage top N buff IDs with `node scripts/suggest_research_buff_mappings.mjs` + in-game tooltips
- [x] Document triage outcome (combat vs economy vs defer) for top unmapped buffs — [research_unmapped_triage.md](research_unmapped_triage.md)

### Explicit mappings

- [x] Add high-value combat rows to `data/research/buff_id_to_stat.json` (faction dual-gate patch, 31 rows)
- [x] Add stable shared loca rows to `data/research/loca_id_to_stat.json` where appropriate (25 officer A/D/H locas)
- [x] Run `node scripts/gen_research_faction_buff_patch.mjs --dry-run`; review and merge faction gates
- [x] Re-import: `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0`
- [x] Update baseline metrics table above after regen

### Scope safety (do not over-map)

- [x] Audit newly mapped lines for Armada-only / PvP-only / station-defense / “when defending” text — attach conditions or exclude
- [x] Avoid widening import heuristics without evidence (officer inference + loca arrays only; no broad heuristics)

---

## Track C — Conditional correctness (engine + canonical)

Smaller scope, higher correctness bar for gated research trees.

### Canonical overrides

- [x] Inventory conditional catalog rows that still flat-merge or lack seats (vs `research_canonical.json` coverage) — [research_conditional_inventory.md](research_conditional_inventory.md); morale `apex_barrier` flat-merge fixed (2026-05-27)
- [x] Add canonical overrides for flagship conditional trees (burning weapon damage, burning+HB isolytic, morale isolytic, etc.) — 11 rids in `research_canonical.json`
- [x] Use `snapshot_by_level` where STFC displays tier snapshots — KSG rid `2392190200` only; others need calibration before override

### Engine gaps

- [x] Document which dual-gate hull/shield combos work today (faction-only) vs need seats — [research_conditional_routing.md](research_conditional_routing.md)
- [x] Add calibration tests for representative conditional research (extend `tests/research_profile_merge_tests.rs` or recorded-fight slices)

### Importer one-offs

- [x] Review special cases in `normalizeBonusValue` (e.g. buff `1898558353`); generalize pattern if repeated — `show_percentage` percentage points in `scripts/lib/research_normalize_bonus_value.mjs`

---

## Track D — Hull / shield HP from research (import parity with buildings)

**Conversion rule agreed (2026-05-27):** research `hull_hp` / `shield_hp` use the **same combat semantics as buildings** — not absolute flat HP added to the combatant, and not “÷ reference ship hull at tier.”

### Runtime (already implemented)

1. Cumulative **additive fractions** merge into `profile.bonuses["hull_hp"]` / `["shield_hp"]` (and owner-faction slices where gated).
2. Scenario build applies via [`apply_profile_to_attacker`](../src/data/profile.rs):

   ```text
   hull_health   × (1 + hull_hp_bonus)
   shield_health × (1 + shield_hp_bonus)
   ```

3. **Large stacked totals are normal.** End-game profiles commonly reach **+1,000%** and **+10,000%** combined hull or shield (engine fraction `10.0` → ×11, `100.0` → ×101). Do not cap or “sanity shrink” catalog values for being large.

Buildings already store fractional bonuses (e.g. Parsteel Generator `hull_hp: 0.01` = +1% at level 1). Research should land in the catalog in the **same units**.

### Remaining work (import + mapping only)

- [x] Agree conversion rule — same as buildings: additive fraction → `(1 + bonus)` multiplier (see above)
- [x] Extend `show_percentage` percentage-point normalization to `hull_hp` / `shield_hp` in [`scripts/lib/research_normalize_bonus_value.mjs`](../scripts/lib/research_normalize_bonus_value.mjs) — tests in [`scripts/test/research_normalize_bonus_value.test.mjs`](../scripts/test/research_normalize_bonus_value.test.mjs); large non-pct integer points (÷100 up to 10,000) for hull/shield only
- [x] Map shared building hull/shield buff IDs into research map — [`scripts/gen_research_hull_shield_building_buff_patch.mjs`](../scripts/gen_research_hull_shield_building_buff_patch.mjs) (+17 keys); re-import: `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0`
- [x] Re-run `--dump-unmapped` + [`scripts/triage_research_hull_shield_unmapped.mjs`](../scripts/triage_research_hull_shield_unmapped.mjs) — **0** inferrable hull/shield rows still unmapped (2026-05-27)
- [x] Merge test: owner-faction hull cumulative fractions — `merge_research_hull_hp_stacks_fractions_like_buildings` in [`tests/research_profile_merge_tests.rs`](../tests/research_profile_merge_tests.rs)
- [ ] Spot-check synced profile totals vs in-game ship sheet (optional calibration; validates **normalization**, not magnitude ceiling)

**Out of scope for Track D:** inventing a ship-base HP divisor for standard % hull/shield lines. **Conditional** owner-faction / dual-gate hull/shield routing stays in [Track C / research_conditional_routing.md](research_conditional_routing.md).

**Uncertainty:** upstream rows that are truly **absolute integer HP** (not percentage copy) still need a per-row anchor before mapping — most mapped combat hull/shield research uses fractional / percentage-point upstream shapes like buildings.

---

## Track E — Validation hygiene

- [ ] Extend `report_unknown_mappings` (or sibling report) with research section: unmapped buff IDs, suspect global scopes
- [ ] Wire research mapping check into `cargo xtask validate` / data-refresh workflow (optional fail on regression)
- [ ] CI: confirm `tests/scenario_research_integration_tests.rs` runs against populated catalog

---

## Explicit non-goals (for now)

- [ ] ~~Map all 2,330 upstream projects~~ — most are economy/meta; target high-investment combat trees
- [ ] ~~Generic `timing` field on every catalog bonus~~ — seat vs flat split already exists; fix row-by-row
- [ ] ~~Alliance research in this doc~~ — separate data source from `research_catalog.json`

---

## Maintainer inputs (unblocks prioritization)

- [ ] Export or paste unmapped `rid` list from active profile (`research.imported.json` vs catalog)
- [ ] Choose primary goal: **absolute DPS accuracy** vs **optimizer ranking stability** (sets Track B vs C priority)
- [ ] Flag research trees you run often (faction crit, NS burning, morale isolytic, etc.) for Track C first

---

## Progress summary

| Track | Status | Notes |
|-------|--------|-------|
| A — Observability | **Done** | API + Roster & Profile (2026-05-27) |
| B — Mapping coverage | **Done** (2026-05-27) | +25 projects, officer loca + faction gates; see triage doc |
| C — Conditional correctness | **Done** (2026-05-27) | Inventory + routing doc, tests, `show_percentage` normalize; morale `apex_barrier` seats fixed |
| D — Hull/shield import | **Done** (2026-05-27) | Building-parity normalize + 17 explicit buff ids; optional in-game spot-check remains |
| E — Validation | Not started | |

Update this table and check boxes as items ship.
