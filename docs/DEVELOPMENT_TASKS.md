# Development Tasks

Ordered checklist for tracking near-term Kobayashi simulator and optimizer work.

1. [ ] Review current simulator, optimizer, data, and frontend roadmaps to confirm active priorities.
2. [x] Audit existing support buff data paths from frontend selection through Rust profile serialization.
3. [x] Define the canonical support buff schema, including ids, display names, stat targets, stacking behavior, and provenance notes.
4. [x] Normalize support buff catalog entries so frontend and backend use the same identifiers and metadata.
5. [x] Add validation for support buff catalog data and profile payloads.
6. [x] Extend profile loading and saving tests to cover support buff round trips.
7. [ ] Verify research summary merge behavior against representative HiggsBozo profile data.
8. [x] Add focused parity tests for combat effect specs that combine research and support buffs.
9. [x] Review Monte Carlo scenario construction to ensure support buffs, research buffs, and crew effects are resolved once per simulation request.
10. [x] Improve combat trace output for externally supplied buffs so users can see which bonuses were applied.
11. [ ] Add API contract coverage for optimize and simulate requests that include support buffs.
12. [ ] Regenerate or update frontend API types if request or response schemas changed.
13. [ ] Update the support buff selector UI to group buffs by source and stat category.
14. [ ] Add frontend validation for incompatible, duplicate, or unsupported support buff selections.
15. [ ] Add frontend tests for support buff selection, persistence, and request serialization.
16. [ ] Run targeted Rust tests for profile, research, scenario, and combat parity behavior.
17. [ ] Run targeted frontend tests for support buff selector and API payload generation.
18. [ ] Run full local CI or the closest practical subset before opening a PR.
19. [ ] Update user-facing docs for support buff usage, limitations, and known uncertain mechanics.
20. [ ] Prepare a PR summary with simulator mechanics changed, test coverage, and remaining uncertainty.

## Task 2 Audit: Support Buff Data Path

Completed audit of the existing support buff path from React selection through Rust request handling and scenario-local profile/static buff application.

- Frontend support buff options are bundled from `data/support_buffs.json` in `frontend/src/lib/supportBuffs.ts`, but selectable ids are still an explicit four-id list. Adding catalog entries alone does not make them selectable.
- `frontend/src/components/SupportBuffSelect.tsx` stores selected ids as workspace UI state and allows any checkbox combination; `exclusive_group` resolution is currently server-side only.
- `frontend/src/lib/workspaceRequests.ts`, `frontend/src/lib/api.ts`, `frontend/src/pages/Workspace.tsx`, and `frontend/src/components/SimResults.tsx` pass `support_buffs` through simulate, optimize/start, and compare-crews payloads when selected.
- `frontend/src/lib/useWorkspace.ts` includes selected support buffs in optimize warm-start cache keys, but not in optimize estimate requests or saved presets.
- `src/data/profile.rs` confirms `PlayerProfile` serialization does not persist `support_buffs`. Support buffs are request-scoped and applied in memory during scenario construction.
- `src/data/data_registry.rs`, `src/data/support_buffs.rs`, `src/server/api.rs`, `src/server/api/requests.rs`, and `src/optimizer/monte_carlo/scenario.rs` load the catalog, resolve requested ids, cap/dedupe selections, apply exclusive groups, merge virtual support research and static bonuses into scenario-local state, and surface unknown-id warnings for simulate/compare.

Known follow-up risks are tracked by later checklist items: optimize/replay warning behavior is inconsistent with simulate/compare, missing support-buff catalog data becomes a silent no-op, `apply_support_buffs_for_request` is currently unused, and frontend/backend support id plus gated research rid lists require manual synchronization.

## Task 9 Review: Monte Carlo Scenario Resolution

Reviewed optimize/simulate Monte Carlo entry points around `SharedScenarioData`, `scenario_to_combat_input_from_shared`, and the tiered/exhaustive optimizer paths.

- Support buff id resolution, support static buff aggregation, imported research loading, research profile merge, and support-gated research seat derivation are scenario-level work in `SharedScenarioData`.
- Per-candidate crew LCARS/effect resolution is still done once before that candidate's Monte Carlo trial loop, then reused for each seeded trial.
- Updated exhaustive and heuristic batch paths to reuse already-built shared scenario state instead of rebuilding registry/standalone scenario state per batch.
