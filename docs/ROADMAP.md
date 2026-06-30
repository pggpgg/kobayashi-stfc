# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

_Last updated 2026-06-30. The June 2026 audit backlog (engine decomposition, assurance gates, upstream drift automation, June patch content, import faction resolution, and related prep) is **shipped**, as is PvE crew-search-space reduction — captain/below-decks bans, the eligibility-matrix filter, and the [`search-space-report`](PVE_CREW_SEARCH_SPACE_REDUCTION.md) measurement harness (bans + eligibility cut the full-catalog space 46×–5,400× by scenario). **Component upgrades is also shipped** (PR #231, recalibrated PR #235): synced per-component ids from `profiles/*/ships.imported.json` now resolve into stat deltas over the base tier/level row and merge into both attacker and PvP-defender stats at scenario build ([`ship.rs`](../src/data/ship.rs) `apply_component_overrides_to_ship_record`, [`data_registry.rs`](../src/data/data_registry.rs) `resolve_ship_with_tier_level_and_imported_components`, [`scenario.rs`](../src/optimizer/monte_carlo/scenario.rs)). Durable design detail lives in linked docs, not here._

## Planned

- **Product polish & speed (web UI)** — The next main goal. The engine, data, and optimizer are mature (officer modeling fidelity ~maxed, search well-tuned, component upgrades shipped); focus is refining the React SPA. The app is solid at desktop width — incremental refinement, not a rebuild.

  **Shipped (2026-06-26):**
  - **React Router v7 readiness** — opted into `v7_startTransition` / `v7_relativeSplatPath`; console deprecation warnings cleared ([`main.tsx`](../frontend/src/main.tsx)).
  - **Optimize ("Strategy") panel density** — the ~40-control panel now keeps the common path visible (seeds → strategy → tiered → ranking) and collapses the advanced groups (chain grind, novelty/diversity, search constraints, search scope) behind native `<details>` sections ([`OptimizePanel.tsx`](../frontend/src/components/OptimizePanel.tsx); `.opt-section` in [`index.css`](../frontend/src/index.css)).
  - **Narrow / vertical-screen layout** — the side rail collapses to a full-width top bar ≤768px, and the crew + strategy/results columns stack ≤900px, on both the Workspace and PvP pages. Driven by media queries on `.app-shell` / `.rail` / `.workspace-body` / `.optimize-panel` / `.pvp-results`; the relevant layout styles were moved out of inline props so no `!important` is needed.
  - **Component-upgrade visibility (2026-06-27)** — a workspace-header chip shows whether the active profile's synced ship components add stats over the base hull tier (amber "⬆ +deltas" with a full-breakdown tooltip) or match it (muted), via the new profile-aware `GET /api/ships/:id/component-overrides`. The override path already computed the deltas and discarded them; it now returns a [`ComponentOverrideSummary`](../src/data/ship.rs) so the UI confirms the shipped component-upgrades backend is actually applied to sim/optimize ([`WorkspaceHeader.tsx`](../frontend/src/components/WorkspaceHeader.tsx)).
  - **Inline-style consolidation (2026-06-29)** — repeated inline `style={{…}}` objects hoisted into typed module-scope `styles` constants across the four heaviest SPA views: [`SimResults`](../frontend/src/components/SimResults.tsx) (76→58), [`OptimizePanel`](../frontend/src/components/OptimizePanel.tsx) (58→44), [`RosterProfile`](../frontend/src/pages/RosterProfile.tsx) (152→94), [`Sensitivity`](../frontend/src/pages/Sensitivity.tsx) (76→30). 136 inline-style sites removed, net −214 source lines; the hoisted objects are also no longer re-allocated per render. Behavior-preserving (value-identical replacement), verified by `tsc` / `biome` / 155 Vitest tests ([PR #239](https://github.com/pggpgg/kobayashi-stfc/pull/239)).
  - **Sensitivity-table de-dup (2026-06-30)** — [`SobolResults`](../frontend/src/components/SobolResults.tsx) and [`MorrisResults`](../frontend/src/components/MorrisResults.tsx) were genuine near-duplicates (identical `fmtFloat` helper, sort-button row, and zebra-striped table shell) and now share a generic [`SortableStatTable`](../frontend/src/components/SortableStatTable.tsx); `fmtFloat`/`fmtPct` moved to [`lib/sensitivityFormat.ts`](../frontend/src/lib/sensitivityFormat.ts), also adopted by `SobolPairs` and `SensitivityResults`. SobolResults 298→225 lines, MorrisResults 255→184. Scoped down from this roadmap's original "Shared `ResultsTable<T>`" framing — on inspection `SimResults` doesn't paginate like the other two and isn't a near-duplicate of them (row-selection, real pagination, a compare-distributions panel, three mode-dependent column sets), so it was excluded rather than forced into the same abstraction. Behavior-preserving: existing Vitest coverage for both components passed unmodified, plus new tests for the extracted pieces.

  Verified at 375px (phone), 850px, and 1280px on both Workspace and PvP: no horizontal page overflow, and the wide recommendations table scrolls within its own `overflow:auto` box.

  **Remaining (optional, larger increments):** what's left is bigger and discretionary.
  - **Split `RosterProfile.tsx`** — at ~1,455 lines (still ~94 inline styles after #239) it's the largest component; extracting the stat / comparator / search blocks into subcomponents would aid testability. Med risk.
  - **Design tokens** — promote the now-hoisted spacing/radius literals to CSS custom properties in [`index.css`](../frontend/src/index.css) (alongside the existing color tokens) so the `styles` constants reference `var(--space-*)` rather than magic numbers.

## Optional follow-ups (low priority)

- ~~**Hostile names in data files**~~ *Shipped 2026-06-30* — `normalize_hostiles_stfc_space` now resolves `hostile_name` via [`hostile_loca::resolve_hostile_display_name`](../src/data/hostile_loca.rs), the same `loca_id` → string map already used by `/api/hostiles`'s `display_name` field. All 5,420 on-disk `data/hostiles/*.json` regenerated; 0 fall back to the `Hostile {id}` placeholder. [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md) notes refreshed to match.

## Assessed, no action planned

From the 2026-06-09 audit, these came back clean or justified — don't re-litigate without new evidence:

- `scenario.rs` (~3,800 lines) is large but well-factored internally; no split needed.
- The 39 `clippy::too_many_arguments` allows are justified by the registry/DTO architecture.
- Error handling: panics are confined to defensive asserts, malformed API input 400s cleanly, job registries are bounded with prune-on-insert.
- Dependencies: minimal, stable, no git deps, no duplicates.
- Station-defense conditions remain a non-goal — see [NOT_ROADMAP.md](NOT_ROADMAP.md).
- Full-catalog exhaustive search over the broad `red_moving_space` hostile category stays impractical at realistic confirm depth (~17 B crews even after bans + eligibility, per [PVE_CREW_SEARCH_SPACE_REDUCTION.md](PVE_CREW_SEARCH_SPACE_REDUCTION.md)). Tiered/genetic search and per-profile owned-roster narrowing are the intended reducers there — not further catalog-wide bans, which would risk dropping functional crews for ~1% space savings.
