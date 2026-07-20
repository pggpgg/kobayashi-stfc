# PvE crew-search-space reduction — measurement

Measures each search-space reduction's effect on candidate count and projected exhaustive-search
runtime. Regenerate any time the officer catalog, ban list, or eligibility matrix changes:

```bash
cargo build --release
RUST_LOG=error ./target/release/kobayashi search-space-report            # all 12 scenarios, 50 sims/crew
RUST_LOG=error ./target/release/kobayashi search-space-report --sims 1000 --enemy-type red_moving_space
RUST_LOG=error ./target/release/kobayashi search-space-report --json     # machine-readable
```

The numbers below are produced by that command; this doc is a snapshot for reference, not a
hand-maintained table.

## TL;DR

- The optimizer's two catalog-level filters cut the full-catalog PvE crew space by **46×**
  (broad `red_moving_space` hostiles) up to **~5,400×** (assaults).
- The cut is almost entirely the **eligibility matrix** (the per-ability "does this seat work vs this
  enemy?" gate, plus the below-decks heuristic). The **curation ban list** removes only ~1% of the
  full-catalog space — it is a precision opt-out for weak-but-functional officers, not a bulk reducer.
- **Feasibility verdict:** for structured PvE scenarios (assaults, mission bosses, armadas, invasions,
  outposts) bans + eligibility alone bring full-catalog exhaustive search into reach — minutes to a
  couple of hours. For the broad `red_moving_space` catch-all (and PvP) the residual space is still
  9–17 B crews, too large to enumerate exhaustively at realistic confirm depth; there, tiered/genetic
  search and the per-profile **owned-roster** narrowing (already shipped, not measured here) remain
  the practical reducers.

## What is measured

For each combat scenario, officer pools are built with exactly the predicates the optimizer's pool
builder uses ([`build_officer_pools_from_registry`](../src/optimizer/crew_generator.rs)), applied as
three cumulative stages:

1. **raw (legality only)** — every officer that can physically occupy the seat (captain ability /
   bridge / below-decks-slot ability). Below-decks uses `strict` pool membership.
2. **+ ban list** — minus officers opted out per seat/mode in
   [`data/optimizer/officer_ban_list.csv`](../data/optimizer/officer_ban_list.csv).
3. **+ eligibility (matrix + heuristic)** — minus officers the eligibility matrix marks "does not
   work" for the scenario (with the below-decks heuristic fallback for coverage gaps). This is the
   production pool. Stage 3 also re-applies the ban check, so the stages are monotonic — each pool is
   a subset of the previous, and adjacent differences are the marginal effect of that filter.

**Legal crews** at each stage is `captains × C(bridge, 2) × C(below_decks, slots)` — an upper bound
that ignores the small loss from crews reusing one officer across seats (the optimizer's enumerator
prunes those). It is computed in closed form, so it never saturates the way the estimate endpoint's
2M-capped exact counter does, and stays comparable across stages.

**Exhaustive time** projects the final stage as `crews × sims × 4e-9 s` (the
`/api/optimize/estimate` cost model, [`EXHAUSTIVE_SEC_PER_CANDIDATE_SIM`](../src/optimizer/crew_generator.rs)).
It models *simulation* cost only on a typical multi-core machine and is optimistic — it ignores the
cost of enumerating billions of candidate tuples in the first place. Treat it as a lower bound on
wall-clock.

Caveats: this is the **full catalog** (no `roster.imported.json` filter), so it reflects the worst
case "owns everything"; a real roster is far smaller. Counts depend on the current officer catalog
and will drift as officers are added.

## Snapshot (2026-06-24, below-decks slots: 3, pool mode: strict)

Catalog at time of capture: 204 captain-capable, 287 bridge-capable, 84 below-decks officers →
**797.75 B** raw crews.

### Legal crews per stage — exhaustive time @ 50 sims/crew

| Scenario | Raw | + ban list | + eligibility | Reduction | Exhaustive time |
|---|--:|--:|--:|--:|--:|
| pvp_space | 797.75 B | 789.93 B | 9.42 B | 84.7× | 31.4 min |
| pvp_station | 797.75 B | 789.93 B | 10.13 B | 78.8× | 33.8 min |
| red_moving_space | 797.75 B | 789.93 B | 17.27 B | 46.2× | 57.6 min |
| waves | 797.75 B | 789.93 B | 9.14 B | 87.2× | 30.5 min |
| mission_bosses | 797.75 B | 789.93 B | 201.23 M | 3964.4× | 40.2 s |
| q_trial | 797.75 B | 789.93 B | 6.27 B | 127.3× | 20.9 min |
| solo_armadas | 797.75 B | 789.93 B | 2.07 B | 385.7× | 6.9 min |
| group_armadas | 797.75 B | 789.93 B | 1.93 B | 412.9× | 6.4 min |
| assaults | 797.75 B | 789.93 B | 147.17 M | 5420.6× | 29.4 s |
| invading_entities | 797.75 B | 789.93 B | 729.79 M | 1093.1× | 2.4 min |
| outpost_armadas | 797.75 B | 789.93 B | 356.42 M | 2238.2× | 71.3 s |
| outpost_retaliation_attackers | 797.75 B | 789.93 B | 337.47 M | 2363.9× | 67.5 s |

At a realistic confirm depth of **1000 sims/crew**, the same final spaces project to: red_moving_space
19.2 h · pvp_space 10.5 h · q_trial 7.0 h · solo/group armadas ~2.2 h · invading_entities 48.7 min ·
outpost_* ~23 min · mission_bosses 13.4 min · assaults 9.8 min.

### Officer-pool sizes (raw → final)

| Scenario | Captains | Bridge | Below-decks |
|---|--:|--:|--:|
| pvp_space | 204 → 115 | 287 → 152 | 84 → 36 |
| pvp_station | 204 → 122 | 287 → 153 | 84 → 36 |
| red_moving_space | 204 → 102 | 287 → 155 | 84 → 45 |
| waves | 204 → 97 | 287 → 150 | 84 → 38 |
| mission_bosses | 204 → 81 | 287 → 105 | 84 → 15 |
| q_trial | 204 → 97 | 287 → 141 | 84 → 35 |
| solo_armadas | 204 → 92 | 287 → 132 | 84 → 26 |
| group_armadas | 204 → 90 | 287 → 129 | 84 → 26 |
| assaults | 204 → 74 | 287 → 94 | 84 → 15 |
| invading_entities | 204 → 80 | 287 → 102 | 84 → 23 |
| outpost_armadas | 204 → 79 | 287 → 97 | 84 → 19 |
| outpost_retaliation_attackers | 204 → 78 | 287 → 95 | 84 → 19 |

Per-seat cuts compound: `red_moving_space` shrinks each pool ~2× (captains), ~1.85× (bridge), ~1.9×
(below-decks), which multiply to the 46× crew-count reduction. Scenarios with aggressive eligibility
coverage (assaults, mission_bosses) trim below-decks from 84 to 15, and that single ~6.7× factor on
`C(below, 3)` drives most of their thousand-fold reductions.

## Search-quality tradeoff

The reductions are *exclusions of officers whose seat ability does not function against the scenario*
(eligibility) or *deliberate curation opt-outs* (bans) — not statistical pruning of plausibly-good
crews. So the dominant cut (eligibility) should not drop winning crews: an officer the matrix marks
"does not work" contributes no scenario-relevant buff. The residual risk is matrix mislabels; those
are auditable per-ability in [`data/officers/eligibility_matrix.json`](../data/officers/eligibility_matrix.json)
and covered by the eligibility coverage tests. The ban list is the only place a *functional* officer
is removed; it is intentionally tiny (see [OPTIMIZATION_SPECIAL_HEURISTICS.md](OPTIMIZATION_SPECIAL_HEURISTICS.md)).
