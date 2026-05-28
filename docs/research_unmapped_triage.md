# Research unmapped buff triage

Generated from `node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped | node scripts/triage_research_unmapped.mjs --json` after Track B mapping work (2026-05-27).

## Summary

| Metric | Before Track B | After Track B |
|--------|----------------:|--------------:|
| Catalog research projects | 935 | 960 |
| Catalog bonus rows | 9,129 | 9,505 |
| Upstream nodes skipped (no combat levels) | 1,395 | 1,370 |
| Unmapped buff ids | 1,244 | 1,219 |

## What shipped in Track B

1. **Faction gates** — `node scripts/gen_research_faction_buff_patch.mjs` merged `attacker_faction` / `defender_faction` onto 31 existing `data/research/buff_id_to_stat.json` rows (dual-gate Fed/Klg/Rom hull vs same-faction defender lines).
2. **Officer stat loca mappings** — 25 entries in `data/research/loca_id_to_stat.json` for officer Attack/Defense/Health research (`officer_attack`, `officer_defense`, `officer_health` profile keys).
3. **Inference** — `scripts/lib/research_stat_inference.mjs` maps single-axis officer description lines; triple-stat lines remain loca-array overrides only.
4. **Triage script** — `scripts/triage_research_unmapped.mjs` categorizes remaining unmapped buff ids (read-only).

## Unmapped categories (1,219 buff ids)

| Category | Count | Action |
|----------|------:|--------|
| `economy_meta` | 585 | **Exclude** — generators, storage, reputation, rewards |
| `no_description` | 268 | **Defer** — no `research_project_description` for buff `loca_id` |
| `other_unmapped` | 240 | **Review individually** — mostly non-combat wording not covered by inference |
| `armada_scope` | 53 | **Exclude** until armada mode exists |
| `station_defense_scope` | 40 | **Exclude** — station / first-round defending |
| `pvp_scope` | 18 | **Exclude** — vs player ships |
| `officer_stats` | 8 | **Defer** — remaining officer lines with non-standard copy |
| `ship_specific` | 7 | **Exclude** — single-ship bonuses (Stella, Botany Bay, …) |

## Top unmapped by occurrence (combat-adjacent — still defer)

| buff_id | count | category | note |
|---------|------:|----------|------|
| 3447359332 | 6 | `no_description` | “Automated Defenses” ship projects; buff `values` are all **0** upstream — nothing to merge |
| (most others) | 1 | varies | Long tail of one-off economy / scope-specific lines |

## Scope safety audit (new mappings)

- **Faction patch:** only added owner + defender faction on rows already mapped to combat stats; no new global weapon_damage from scoped text.
- **Officer loca:** global officer A/D/H — matches in-game “all Officers” research; does not model below-decks-only gates (one loca mapped to `officer_attack` with “below deck” copy — acceptable approximation until officer-slot modeling improves).
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
