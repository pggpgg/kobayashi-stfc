# Kobayashi Work Queue (Ordered)

1. [x] Build an "unknown mapping" report for canonical officer condition tokens and hostile `upstream_ship_type` values.
2. [ ] Add missing canonical condition token mappings that already have explicit, testable engine meaning.
3. [x] Enumerate and document all known hostile `ship_type` ids; add missing `match` arms and profile fields where needed.
4. [x] Add validation checks to flag normalized hostiles with unmapped `upstream_ship_type`.
5. [ ] Add `bid` coverage (or fallback mapping) to the building index to improve import/sync resolution.
6. [x] Implement a strict validation report for unmapped building buffs and unsupported building conditions.
7. [x] Audit forbidden/chaos tech catalog `fid` coverage and fill missing ids to keep sync application complete.
8. [x] Re-run and review chaos-tech row generation (`build_chaos_tech_csv_rows.mjs`) and fix high-impact heuristic gaps.
9. [x] Refresh research catalog from upstream and resolve unmapped buff/location ids in mapping files.
10. [ ] Extend research stat wiring for any newly mapped combat stats in normalization and combat application paths.
11. [ ] Add/expand recorded-fight fixtures to validate research and forbidden-tech effects against observed outcomes.
12. [ ] Wire stored sync battlelogs into one concrete consumer path (calibration, replay seed, or analysis CLI).
13. [ ] Tighten default "real roster" filters in discovery flows (owned officers, legal seats, unlocked below-decks slots).
14. [ ] Push additional sound constraint-aware narrowing earlier in crew generation without combinatorial blow-up.
15. [ ] Tune analytical prefilter auto-keep behavior by workload profile and candidate counts.
16. [ ] Tune tiered optimization auto-thresholds and default scout/confirm budgets for large candidate searches.
17. [ ] Implement adaptive simulation budget allocation based on scout confidence and variance.
18. [ ] Add matchup-aware pruning priors (captain/bridge synergies, encounter tags, family priors from winners).
19. [ ] Add novelty-aware ranking so top suggestions balance strength with material diversity.
20. [ ] Ship a first-class fast-discovery pipeline (heuristic seeds -> analytical prefilter -> tiered scout -> confirm top K -> optional refinement) with API/UI exposure.