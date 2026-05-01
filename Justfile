# kobayashi-stfc development targets
#
# Usage:
#   just           — list all targets
#   just check     — fastest feedback: type-check Rust only (no tests)
#   just test-quick — fast subset: Rust unit tests only
#   just test      — all Rust tests via nextest
#   just test-all  — everything: Rust + frontend + Python + E2E

# ── Rust ──────────────────────────────────────────────────────────────

# Fastest compilation feedback loop — type-check only, no codegen.
check:
    cargo check

# Rust unit tests only (src/ — no integration tests). Fastest test feedback.
# Skips sync tests (incompatible with nextest's per-process model).
test-quick:
    cargo nextest run --lib

# Alias for test-quick.
test-unit: test-quick

# Integration tests only (tests/ directory).
test-integration:
    cargo nextest run --test '*'

# Full Rust test suite.
# Uses nextest for everything except sync tests (which need single-process
# execution).  Falls back to cargo test for those.
test:
    cargo nextest run
    cargo test server::sync::tests --quiet

# ── Frontend ──────────────────────────────────────────────────────────

# Run frontend Vitest tests.
test-frontend:
    cd frontend && npx vitest run

# Run frontend type-check only.
check-frontend:
    cd frontend && npx tsc -b

# Run frontend linter.
lint-frontend:
    cd frontend && npx biome check .

# ── Python ────────────────────────────────────────────────────────────

# Run Python combat engine tests.
test-python:
    python -m pytest tools/combat_engine/tests/ -v

# ── E2E ───────────────────────────────────────────────────────────────

# Run Playwright E2E smoke tests.
test-e2e:
    npx playwright test

# ── Full suite ────────────────────────────────────────────────────────

# Run every test suite: Rust, frontend, Python, E2E.
test-all: test test-frontend test-python test-e2e

# ── Formatting ────────────────────────────────────────────────────────

# Format Rust code.
fmt:
    cargo fmt --all

# Format frontend code.
fmt-frontend:
    cd frontend && npx biome check --write .

# Run all linters.
lint:
    cargo clippy --all-targets -- -D warnings
    cd frontend && npx biome check .

# ── Development servers ───────────────────────────────────────────────

# Start the backend API server.
serve:
    cargo run -- serve

# Start the frontend dev server (proxies API to backend).
dev-frontend:
    cd frontend && npx vite
