# Development Tasks

Ordered checklist for tracking near-term Kobayashi simulator and optimizer work.

1. [ ] Review current simulator, optimizer, data, and frontend roadmaps to confirm active priorities.
2. [ ] Audit existing support buff data paths from frontend selection through Rust profile serialization.
3. [x] Define the canonical support buff schema, including ids, display names, stat targets, stacking behavior, and provenance notes.
4. [x] Normalize support buff catalog entries so frontend and backend use the same identifiers and metadata.
5. [ ] Add validation for support buff catalog data and profile payloads.
6. [ ] Extend profile loading and saving tests to cover support buff round trips.
7. [ ] Verify research summary merge behavior against representative HiggsBozo profile data.
8. [ ] Add focused parity tests for combat effect specs that combine research and support buffs.
9. [ ] Review Monte Carlo scenario construction to ensure support buffs, research buffs, and crew effects are resolved once per simulation request.
10. [ ] Improve combat trace output for externally supplied buffs so users can see which bonuses were applied.
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

