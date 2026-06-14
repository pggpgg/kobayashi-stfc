# Research unmapped buff triage

Generated from `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped | node scripts/triage_research_unmapped.mjs --json` after task 5 (2026-06-14).

## Summary

| Metric | Before Track B (2026-05-27) | After task 5 (2026-06-14) |
|--------|--------------------------:|--------------------------:|
| Catalog research projects | 960 | 758 |
| Catalog bonus rows | 9,505 | 7,485 |
| Upstream nodes skipped (no combat levels) | 1,370 | 1,666 |
| Unmapped buff ids | 1,219 | 1,418 |

Note: project/bonus counts dropped because import now skips more non-combat upstream trees; unmapped count rose with fuller upstream cache — not a regression in mapping coverage for combat rows.

## What shipped in task 5

1. **Cross-faction weapon_damage fixes** — corrected `attacker_faction` + `defender_faction` on `982655355` (Romulan vs Federation), `2982312380` (Federation vs Klingon), `4009387266` (Klingon vs Romulan).
2. **Faction patch refresh** — `node scripts/gen_research_faction_buff_patch.mjs` merged 22 additional defender/owner gates on existing combat rows.
3. **Conditional hull/shield seats** — `HullHpMultiplier` / `ShieldHpMultiplier` compile from catalog `add` rows; morale → `RoundStart`, burning/HB/faction → `AttackPhase`; applied per round in `engine.rs`.
4. **Dual-gate hull/shield scenario path** — verified by unit/integration tests; **no upstream hull/shield projects** with owner+defender faction only (audit 2026-06-14).
5. **Baseline refresh** — `data/research/mapping_gaps_baseline.json` → 1,418 unmapped, 0 suspect global scopes.

## Unmapped categories (1,418 buff ids)

| Category | Count | Action |
|----------|------:|--------|
| `economy_meta` | 609 | **Exclude** — generators, storage, reputation, rewards |
| `no_description` | 278 | **Defer** — no `research_project_description` for buff `loca_id` |
| `other_unmapped` | 271 | **Review individually** — mostly non-combat wording not covered by inference |
| `armada_scope` | 82 | **Exclude** until armada mode exists |
| `station_defense_scope` | 61 | **Exclude** — station / first-round defending |
| `pvp_scope` | 77 | **Exclude** — vs player ships |
| `officer_stats` | 8 | **Defer** — remaining officer lines with non-standard copy |
| `ship_specific` | 13 | **Exclude** — single-ship bonuses |
| `wave_defense_scope` | 15 | **Exclude** |
| `non_armada_hostile_scope` | 3 | **Exclude** |
| `hostile_and_armada_scope` | 1 | **Exclude** |

## `other_unmapped` review (271 ids)

No additional combat-stat inference was added in this pass: triage found **zero** unmapped buff ids whose descriptions infer `hull_hp`/`shield_hp` (`triage_research_hull_shield_unmapped.mjs` empty). Remaining `other_unmapped` lines are non-combat wording (loot, repair speed, away teams, mining modifiers, etc.) or ambiguous copy — keep deferred until a specific fight/profile gap surfaces.

## Scope safety audit

- **Cross-faction weapon fixes:** only touched buff ids whose upstream `research_project_description` explicitly names both owner and defender factions.
- **Hull/shield:** owner-faction-only mappings unchanged (correct for “all Federation ships” copy).
- **Not mapped:** Armada, PvP, station defense, ship-specific flat integers, economy lines.

## Repeat triage

```bash
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped 2>/dev/null \
  | node scripts/triage_research_unmapped.mjs

# JSON for docs / CI artifacts:
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped 2>/dev/null \
  | node scripts/triage_research_unmapped.mjs --json
```

After editing `data/research/buff_id_to_stat.json` or `data/research/loca_id_to_stat.json`:

```bash
node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
cargo test --test research_profile_merge_tests
cargo test --test scenario_research_integration_tests
```
