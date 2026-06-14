# Contributing to Kobayashi

## First-time setup (minimal path)

If you just cloned the repo and want a fast contributor start:

```bash
# from repo root
cargo test
(cd frontend && npm ci && npm run build)
cargo build --release
./target/release/kobayashi serve
```

- Open `http://localhost:3000` after `serve` starts.
- Run the server from the repository root so it can resolve `frontend/dist` and `data/`.
- See [README.md](README.md#quick-start) for user-facing quick start details.

## Full local parity with CI

From the repository root:

```bash
chmod +x scripts/local-ci.sh   # first time only
./scripts/local-ci.sh
```

This mirrors the Rust and Frontend CI jobs. It does not run the Python combat-engine tests or Playwright E2E; run those separately when needed:

```bash
pip install -r tools/combat_engine/requirements-test.txt
python -m pytest tools/combat_engine/tests/ -v

npm ci && npx playwright install --with-deps chromium
cargo build --release && (cd frontend && npm ci && npm run build) && npm run test:e2e
```

You can also run `npm run verify` from the repo root for the combined verification pipeline documented in [scripts/README.md](scripts/README.md).

## CI workflow

Pull requests run [.github/workflows/ci.yml](.github/workflows/ci.yml) (workflow name **CI**). It includes:


| Job                        | What it runs                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **Rust**                   | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`, `cargo audit`             |
| **Frontend**               | `npm ci`, `npm audit --audit-level=high`, `npm run lint`, `npm run typecheck`, `npm run test`, `npm run build` (in `frontend/`)    |
| **Combat engine (Python)** | `pytest` under `tools/combat_engine/tests/`                                                                                     |
| **E2E smoke (Playwright)** | Release build, frontend build, `npm run test:e2e` (health + workspace sim/optimize/preset/results); failed runs upload `playwright-report` + `test-results` artifacts |

### Branch protection (repository settings)

To block merges when CI fails, a maintainer should enable branch protection on `main` / `master` and require these **status checks** to pass (names as shown in GitHub’s “Require status checks to pass” picker):

- `CI / Rust`
- `CI / Frontend`
- `CI / Combat engine (Python)`
- `CI / E2E smoke (Playwright)`

Also require pull requests before merging if that matches project policy.

## Pre-commit hooks (recommended)

We use [pre-commit](https://pre-commit.com/) with [.pre-commit-config.yaml](.pre-commit-config.yaml) so local commits run the same Rust `fmt`/`clippy` checks (and Biome when `frontend/` files change) before push.

```bash
pip install pre-commit   # or: brew install pre-commit
pre-commit install         # once per clone
```

Run all hooks manually:

```bash
pre-commit run --all-files
```

## Maintenance tasks (`cargo xtask`)

From the repo root, `cargo xtask --help` lists wrappers for ship/hostile/research refresh, `validate_data`, `generate_lcars`, `npm run data:refresh`, and `npm run verify`. See [CLAUDE.md](CLAUDE.md) § Maintenance (`cargo xtask`) and [scripts/README.md](scripts/README.md) for the underlying commands.

## Scheduled data PRs

The workflow [.github/workflows/data-refresh.yml](.github/workflows/data-refresh.yml) can open pull requests with large `data/` diffs. Treat them like any other PR: review scope, run local `cargo test` if needed, and merge when satisfied.

**Upstream summary drift:** CI job `upstream_drift` fails when live data.stfc.space ship/hostile/research catalogs differ from committed `data/upstream/data-stfc-space/summary-*.json`. When it fails, merge the open automated data refresh PR or run `cargo xtask check-upstream-drift` locally and refresh before merging feature work (avoids `data/` merge conflicts).

## Convenience npm script (frontend only)

From the repo root:

```bash
npm run ci:frontend
```

Runs a clean-install frontend pipeline aligned with the CI frontend job (see root [package.json](package.json)).