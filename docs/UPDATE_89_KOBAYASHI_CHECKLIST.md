# Update 89 — Kobayashi support checklist

Track implementation against **STFC Update 89** (First Contact Part 2). Source: official news posts (patch notes 2026-03-31, Borg Sphere highlight 2026-03-29, Mara Dalen first look 2026-03-24).

---

## Data & catalog

- [x] Add **Borg Sphere** extended ship data: upstream `ships/2251018025.json` + summary/translations → canonical `borg_sphere` in `data/ships_extended/` (numeric id `2251018025`; upstream `hull_type` 3 → `ship_class` battleship per `player_hull_type_raw_to_ship_class`)
- [x] Borg Sphere **combat-relevant passives** (Omicron Particle Charge, Apex Dispersion Field, Quantum Nullification Pulse — beam disable is a combat-begin flag until beam math exists)
- [ ] Add Borg Sphere refit **Assimilation Protocol** (Assimilate for two rounds at PvP combat start) if/when PvP scenarios are in scope
- [ ] Add **Conqueror Borg Suppressor** & **Conqueror Borg Obliterator** hostiles (G5–G7) with **Evolutionary Assimilation** scaling — *upstream hostiles present; `conqueror_borg` tags curated in `normalize_hostiles_stfc_space` (31 ids from `translations-navigation` `loca_id` 89050–89055); assimilation scaling still pending*
- [ ] Model **Quantum Resonance Beam** / **Hyperthermic Resonance Beam** + **Hyperthermic Decay** (incl. Borg Sphere 80% case); document assumptions until log-backed
- [ ] **Crew nullification**: Kathryn Janeway, Enterprise-E Picard, Christopher Pike have **no effect** vs Suppressor/Obliterator
- [ ] Officer **Mara Dalen** in LCARS: **Right to Protest** (Isolytic Defense vs Group Armadas); **Defy Defeat** (round-start shield repair vs all Armadas)
- [ ] Reconcile **FCM armada synergy** (FCM Data CM shot stacking, Zefram Cochrane Isolytic Cascade) with blog text vs current LCARS; add tests if needed
- [ ] Import/map **Borg Operating Table** (Prototype Forbidden Tech) combat-relevant stats
- [ ] Import/map **Interplexing Beacon** (Chaos Tech): Hyperthermic Stabilizer, crit chance / shield mitigation vs Conqueror Borg
- [ ] **Epic artifacts** if profile affects sim: Exo-suit Helmet, Interplexion Transducer, Cochrane’s Telescope
- [ ] **Maverick research**: new Borg Sphere warp-range node (likely non-combat); confirm task-related research if ever modeled

## Combat engine & semantics

- [x] Hostile-family / tag predicates so Borg Sphere passives apply only vs Conqueror Borg Suppressor & Obliterator (generalizable pattern — `defender_hostile_tag_mask`, `hostile_tags`, catalog conditions)
- [ ] Implement interaction: **Quantum Nullification** disables Suppressor beam; **Hyperthermic Stabilizer** (beacon) counters Obliterator beam — align with stacking order
- [ ] **Assimilate** (2 rounds) aligned with existing assimilated effectiveness behavior where applicable

## Tests & documentation

- [ ] Tests: nullified captains vs tagged hostiles; Mara round-start shield repair; optional calibration if fixtures exist
- [ ] Short **assumptions doc** (or section in this file) for instant-kill beams until recorded fights confirm behavior

## Out of scope (reference)

These are game/client or non-combat; skip unless you explicitly expand scope:

- Mission QoL (170 G3–G4 mission changes)
- Research UI search tab
- Battle passes, event schedules, promotional content
- Client bug fixes from patch notes (chests, Mantis UI, etc.)

---

*Last updated: 2026-04-19 — Borg Sphere passives + hostile `conqueror_borg` tagging (Suppressor + Obliterator ids) marked complete where implemented; beams / Evolutionary Assimilation still open.*
