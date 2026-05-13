# Roadmap

Planned features and priorities for Kobayashi.

## Codex Speed Demon

Speeding up crew discovery is primarily a search-efficiency problem, not a raw simulator-throughput problem. The simulator is already fast; the roadmap here is about spending Monte Carlo budget on the most promising crews first and learning from prior runs. For the product framing of that pipeline (seed → prune → scout → confirm → learn), see [README.md](../README.md#the-optimizer).

### Below-decks pool heuristics

- **Per-matchup below-decks pool sizing:** Today the pool narrows globally. A hostile with high mitigation may reward different below-decks stat priorities (e.g., pierce officers) than a glass-cannon hostile (e.g., hull HP officers). Compute stat profiles of top historical crews for a match-up and use them to weight below-decks officer scores for future runs, so the pool narrows intelligently rather than uniformly.

### Analytical prefilter improvements

- **Analytical damage floor per hostile tier:** The `prune_analytical_hull_fraction` (drop crews whose expected damage < X% of defender hull) uses a user-supplied fraction. Compute a sensible default per hostile tier: tougher hostiles can tolerate a smaller fraction (0.01), glass-cannon hostiles need a larger fraction (0.10) since the fight is short and every damage point matters. Derive from the hostile's hull-to-attack ratio in the shared scenario data.
