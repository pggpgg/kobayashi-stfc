# Roadmap

What's shipped and what's planned. Explicit non-goals live in [NOT_ROADMAP.md](NOT_ROADMAP.md).

_Last updated 2026-06-26. The June 2026 audit backlog (engine decomposition, assurance gates, upstream drift automation, June patch content, import faction resolution, and related prep) is **shipped**, as is PvE crew-search-space reduction — captain/below-decks bans, the eligibility-matrix filter, and the [`search-space-report`](PVE_CREW_SEARCH_SPACE_REDUCTION.md) measurement harness (bans + eligibility cut the full-catalog space 46×–5,400× by scenario). **Component upgrades is also shipped** (PR #231, recalibrated PR #235): synced per-component ids from `profiles/*/ships.imported.json` now resolve into stat deltas over the base tier/level row and merge into both attacker and PvP-defender stats at scenario build ([`ship.rs`](../src/data/ship.rs) `apply_component_overrides_to_ship_record`, [`data_registry.rs`](../src/data/data_registry.rs) `resolve_ship_with_tier_level_and_imported_components`, [`scenario.rs`](../src/optimizer/monte_carlo/scenario.rs)). Durable design detail lives in linked docs, not here._

## Planned

- **Product polish & speed (web UI)** — The next main goal. The engine, data, and optimizer are mature (officer modeling fidelity ~maxed, search well-tuned, component upgrades shipped); focus is refining the React SPA. The app is solid at desktop width — incremental refinement, not a rebuild.

  **Shipped (2026-06-26):**
  - **React Router v7 readiness** — opted into `v7_startTransition` / `v7_relativeSplatPath`; console deprecation warnings cleared ([`main.tsx`](../frontend/src/main.tsx)).
  - **Optimize ("Strategy") panel density** — the ~40-control panel now keeps the common path visible (seeds → strategy → tiered → ranking) and collapses the advanced groups (chain grind, novelty/diversity, search constraints, search scope) behind native `<details>` sections ([`OptimizePanel.tsx`](../frontend/src/components/OptimizePanel.tsx); `.opt-section` in [`index.css`](../frontend/src/index.css)).
  - **Narrow / vertical-screen layout** — the side rail collapses to a full-width top bar ≤768px, and the crew + strategy/results columns stack ≤900px, on both the Workspace and PvP pages. Driven by media queries on `.app-shell` / `.rail` / `.workspace-body` / `.optimize-panel` / `.pvp-results`; the relevant layout styles were moved out of inline props so no `!important` is needed.

  Verified at 375px (phone), 850px, and 1280px on both Workspace and PvP: no horizontal page overflow, and the wide recommendations table scrolls within its own `overflow:auto` box.

  **Remaining:**
  - **Inline-style consolidation** — the largest components still build many style objects inline each render (SimResults ~76, OptimizePanel ~58 literals). Continue hoisting shared styles to constants / CSS classes (the responsive work already moved the shell/panel layout styles). Incremental, low risk.

## Optional follow-ups (low priority)

- **Hostile names in data files** — the hostile picker already shows proper names (`/api/hostiles` resolves `display_name` from upstream translations; [HostilePicker](../frontend/src/components/HostilePicker.tsx) labels rows accordingly). On-disk `hostile_name` in `data/hostiles/*.json` still uses `Hostile {id}` placeholders from the normalizer — cosmetic data debt, not a UI gap. Optional: bake resolved names into `normalize_hostiles_stfc_space` and refresh stale notes in [STFC_SPACE_DATA_STRATEGY.md](STFC_SPACE_DATA_STRATEGY.md).

## Assessed, no action planned

From the 2026-06-09 audit, these came back clean or justified — don't re-litigate without new evidence:

- `scenario.rs` (~3,800 lines) is large but well-factored internally; no split needed.
- The 39 `clippy::too_many_arguments` allows are justified by the registry/DTO architecture.
- Error handling: panics are confined to defensive asserts, malformed API input 400s cleanly, job registries are bounded with prune-on-insert.
- Dependencies: minimal, stable, no git deps, no duplicates.
- Station-defense conditions remain a non-goal — see [NOT_ROADMAP.md](NOT_ROADMAP.md).
- Full-catalog exhaustive search over the broad `red_moving_space` hostile category stays impractical at realistic confirm depth (~17 B crews even after bans + eligibility, per [PVE_CREW_SEARCH_SPACE_REDUCTION.md](PVE_CREW_SEARCH_SPACE_REDUCTION.md)). Tiered/genetic search and per-profile owned-roster narrowing are the intended reducers there — not further catalog-wide bans, which would risk dropping functional crews for ~1% space savings.
