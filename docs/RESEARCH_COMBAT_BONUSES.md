# Research catalog — combat bonus improvements

Track progress on research-derived combat bonuses. Sibling docs: [`data/README.md`](../data/README.md) § Research, [`DESIGN.md`](DESIGN.md) §5.4.

**Last reviewed:** 2026-05-27

---

## Current pipeline (reference)

```text
Community Mod sync          Maintainer catalog              Combat
──────────────────          ──────────────────              ──────
research.imported.json  →   research_catalog.json      →   profile.bonuses (flat stats)
(rid + level, all rows)     (935 projects w/ bonuses)       + research_owner_faction_bonuses
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
| Explicit buff-id mappings (`buff_id_to_stat.json`) | 85 keys |
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

## Track D — Flat hull/shield HP integers

**Blocked on explicit conversion rule + calibration** — do not invent silently.

- [ ] Collect in-game anchors: research row + integer value + resulting hull/shield delta on a known ship/tier
- [ ] Agree conversion rule (e.g. integer ÷ reference hull at tier/level)
- [ ] Implement in `normalizeBonusValue` / importer with tests
- [ ] Re-run `--dump-unmapped` and confirm `value_is_percentage: false` hull/shield rows map

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
| D — Flat hull/shield | Blocked | Needs conversion rule |
| E — Validation | Not started | |

Update this table and check boxes as items ship.
