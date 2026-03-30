# Kobayashi improvement tasks (codebase audit)

Checklist derived from a codebase audit (not from existing roadmaps or `POTENTIAL_TASKS`). **Preferred order is top to bottom.**

## Security, reliability, and operations

- [x] **1. HTTP body size limits** — Add Axum/tower limits on large POST routes (`/api/sync/ingress`, `/api/optimize`, `/api/simulate`, imports) to reduce DoS and memory exhaustion risk.
- [x] **2. Optimize job store robustness** — Harden `Mutex` usage in `src/server/api/execution.rs` (async optimize jobs); avoid assumptions that locks never fail.
- [x] **3. React error boundary** — Add a top-level boundary (Shell or `App`) with recovery UI so render failures don’t blank the whole app.
- [x] **4. Structured logging** — Introduce `tracing` (or similar) with env filtering; replace ad-hoc `eprintln!` on critical server/sync/CPU paths.
- [x] **5. Health / status metadata** — Enrich `/api/health` or add `/api/status` with build identity, data/version signals, and effective `KOBAYASHI_MAX_CONCURRENT_CPU_JOBS`.
- [x] **6. Deployment threat model** — Document local vs LAN vs internet; clarify that `X-Profile-Id` / `?profile=` is not strong auth. Optionally gate mutating routes behind a shared secret when exposed beyond localhost.

## API contracts and CI quality

- [x] **7. Machine-readable API contracts** — OpenAPI 3 or JSON Schema for heavy payloads (optimize, simulate, compare, sync).
- [x] **8. CI gates** — `cargo fmt --check`; frontend linter (ESLint or Biome); align TypeScript strictness policy with CI.
- [x] **9. Frontend test coverage** — Expand Vitest beyond current files: `useWorkspace`, profile/preset flows, `ResultsLibrary`, navigation.
- [x] **10. Long optimize UX** — SSE/stream reconnect with backoff; survive refresh where feasible; clearer cancel and error states.

## Tooling and combat fidelity

- [x] **11. Python combat validation in CI** — Run `tools/combat_engine/` tests (or scheduled job) so mechanics checks don’t only run manually.
- [x] **12. Mechanics coverage as backlog** — Use `/api/mechanics/coverage` to drive an ordered combat/LCARs gap list (single source of truth for fidelity work).

## Performance and repo structure

- [x] **13. Static asset compression and caching** — Compression for `frontend/dist`; cache-control for hashed Vite assets.
- [x] **14. Route-level code splitting** — `React.lazy` + `Suspense` for secondary routes (`/results`, `/roster`, `/data`).
- [x] **15. Duplicate tree (`gpu-accel-wt/`)** — Merge, subtree, archive, or exclude from active work to avoid drift and double maintenance.

## UX, hygiene, and long-term

- [x] **16. Accessibility pass** — Modal focus traps, visible focus, keyboard flows for run sim/optimize beyond existing `aria-*` usage.
- [x] **17. Supply-chain checks** — `cargo audit` / `npm audit` in CI with an explicit fail vs warn policy.
- [x] **18. Profile backup/restore UX** — Export/import or zip flow for `profiles/` so users can recover from disk mistakes.
- [x] **19. Single data-refresh entrypoint** — One orchestrated script (Make/Just/npm) that chains import steps in the documented order.
- [ ] **20. i18n scaffolding** — String extraction / message catalog if non-English UI is planned.
