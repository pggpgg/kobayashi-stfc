# Contributing to Kobayashi

## First-time setup (the minimal path)

Do these steps after you clone the repository:

```bash
# from repo root
cargo test
(cd frontend && npm ci && npm run build)
cargo build --release
./target/release/kobayashi serve
```

- Open `http://localhost:3000` after the server starts.
- Start the server from the root of the repository. The server finds `frontend/dist` and
  `data/` from there.
- For the quick start for players, refer to [README.md](README.md#quick-start).

## Full local parity with CI

Run these commands from the root of the repository:

```bash
chmod +x scripts/local-ci.sh   # first time only
./scripts/local-ci.sh
```

This script runs the same checks as the Rust job and the Frontend job in CI. It does not
run the Python combat engine tests, and it does not run the Playwright end-to-end tests.
Run those tests separately when you need them:

```bash
pip install -r tools/combat_engine/requirements-test.txt
python -m pytest tools/combat_engine/tests/ -v

npm ci && npx playwright install --with-deps chromium
cargo build --release && (cd frontend && npm ci && npm run build) && npm run test:e2e
```

You can also run `npm run verify` from the root of the repository. This command runs the
combined verification pipeline. For more data, refer to [scripts/README.md](scripts/README.md).

## The CI workflow

Each pull request runs [.github/workflows/ci.yml](.github/workflows/ci.yml). The name of
the workflow is **CI**. It contains these jobs:

| Job                        | What it runs                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **Rust**                   | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`, `cargo audit`             |
| **Frontend**               | `npm ci`, `npm audit --audit-level=high`, `npm run lint`, `npm run typecheck`, `npm run test`, `npm run build` (in `frontend/`)    |
| **Combat engine (Python)** | `pytest` under `tools/combat_engine/tests/`                                                                                     |
| **E2E smoke (Playwright)** | The release build, the frontend build, and `npm run test:e2e` (health plus the workspace simulate, optimize, preset, and results flows). A failed run uploads the `playwright-report` and `test-results` artifacts. |

### Branch protection (in the repository settings)

To stop a merge when CI fails, a maintainer must turn on branch protection for `main` or
`master`. The maintainer must then make these **status checks** mandatory. The names are
the names in the GitHub picker "Require status checks to pass":

- `CI / Rust`
- `CI / Frontend`
- `CI / Combat engine (Python)`
- `CI / E2E smoke (Playwright)`

> CI also runs **`CI / Rust coverage`** and **`CI / Upstream summary drift`**. In most
> conditions these two checks stay optional. The coverage check is for information only.
> The drift check can fail because of a change in the upstream data.stfc.space catalogs
> that has no relation to the pull request. Refer to *Upstream summary drift* below.

Make a pull request mandatory before a merge if this agrees with the project policy.

## Pre-commit hooks (recommended)

The project uses [pre-commit](https://pre-commit.com/) with
[.pre-commit-config.yaml](.pre-commit-config.yaml). Your local commits then run the same
Rust `fmt` checks and `clippy` checks before you push. The hooks also run Biome when you
change a file in `frontend/`.

```bash
pip install pre-commit   # or: brew install pre-commit
pre-commit install         # once per clone
```

To run all the hooks manually:

```bash
pre-commit run --all-files
```

## Maintenance tasks (`cargo xtask`)

Run `cargo xtask --help` from the root of the repository. The output lists the wrappers for
the ship refresh, the hostile refresh, the research refresh, `validate_data`,
`generate_lcars`, `npm run data:refresh`, and `npm run verify`. For the commands below
these wrappers, refer to [CLAUDE.md](CLAUDE.md) § Maintenance (`cargo xtask`) and to
[scripts/README.md](scripts/README.md).

## Scheduled data pull requests

The workflow [.github/workflows/data-refresh.yml](.github/workflows/data-refresh.yml) can
open a pull request with a large difference in `data/`. Treat it as a usual pull request.
Review the scope, run `cargo test` locally if you need it, and merge the pull request when
you are satisfied.

**Upstream summary drift.** The CI job `upstream_drift` compares the live data.stfc.space
catalogs for ships, hostiles, and research with the committed files in
`data/upstream/data-stfc-space/summary-*.json`. The job fails when they are different.
When this job fails, merge the
open automated data refresh pull request. As an alternative, run
`cargo xtask check-upstream-drift` locally and refresh the data before you merge feature
work. This prevents a merge conflict in `data/`.

## Convenience npm script (frontend only)

Run this command from the root of the repository:

```bash
npm run ci:frontend
```

It runs a clean-install frontend pipeline that agrees with the Frontend job in CI. For more
data, refer to the root [package.json](package.json).

## Documentation style

The user-facing documents follow ASD-STE100 Simplified Technical English. Before you change
one of these documents, read [docs/STYLE_STE100.md](docs/STYLE_STE100.md). Section 1 of that
guide lists the documents in scope.
